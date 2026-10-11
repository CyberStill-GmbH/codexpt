use std::{net::SocketAddr, path::PathBuf};

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use chrono::{SecondsFormat, Utc};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use uuid::Uuid;

use crate::identity::{
    AuthenticatedPrincipal, IdentityStore, InviteRole, OidcConfig, OidcState, WorkspaceRole,
    derive_csrf_token, oidc, oidc::cookie_value,
};
use crate::{
    AppError, CheckpointArtifact, CheckpointContent, ContextCheckpoint, EvidenceRef, HumanDecision,
    Message, MessageStatus, Store, Task, TaskEvent, TaskStatus,
    domain::{CONTEXT_CHECKPOINT_SCHEMA, CONTEXT_CHECKPOINT_VERSION, MESSAGE_SCHEMA_VERSION},
};

const MAX_REQUEST_BYTES: usize = 32 * 1024;
const PROXY_SECRET_HEADER: &str = "x-codexpt-proxy-key";

#[derive(Clone)]
struct AppState {
    database_path: PathBuf,
    port: u16,
    identity: Option<IdentityStore>,
    oidc: Option<OidcConfig>,
    api_host: Option<String>,
    web_origin: Option<String>,
    proxy_secret: Option<String>,
}

#[derive(Clone, Default)]
struct RequestContext {
    principal: Option<AuthenticatedPrincipal>,
    workspace_id: Option<String>,
    role: Option<WorkspaceRole>,
}

#[derive(Debug, Serialize)]
struct TaskDetail {
    task: Task,
    messages: Vec<Message>,
    events: Vec<TaskEvent>,
    pending_decision: Option<HumanDecision>,
}

#[derive(Debug, Deserialize)]
struct CreateTaskRequest {
    title: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct CreateMessageRequest {
    sender: String,
    recipient: String,
    body: String,
    #[serde(default)]
    evidence_refs: Vec<EvidenceRef>,
}

#[derive(Debug, Deserialize)]
struct TransitionRequest {
    expected: TaskStatus,
    next: TaskStatus,
    actor: String,
    #[serde(default)]
    expected_version: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RequestDecisionRequest {
    question: String,
    actor: String,
}

#[derive(Debug, Deserialize)]
struct CreateInviteRequest {
    email: String,
    role: InviteRole,
}

#[derive(Debug, Deserialize)]
struct AcceptInviteRequest {
    token: String,
}

#[derive(Debug, Deserialize)]
struct ChangeMemberRoleRequest {
    role: WorkspaceRole,
}

#[derive(Debug, Deserialize)]
struct TransferOwnershipRequest {
    new_owner_user_id: String,
}

#[derive(Debug, Deserialize)]
struct AnswerDecisionRequest {
    answer: String,
}

#[derive(Debug, Deserialize)]
struct CreateContextCheckpointRequest {
    checkpoint_id: String,
    source: String,
    goal: String,
    decisions: String,
    completed: String,
    blocked: String,
    next_actions: String,
    constraints: String,
    #[serde(default)]
    artifacts: Vec<CheckpointArtifact>,
}

#[derive(Debug, Serialize)]
struct MutationResult<T: Serialize> {
    data: T,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

struct ApiError(AppError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self.0 {
            AppError::InvalidInput(message) => (StatusCode::BAD_REQUEST, message),
            error @ (AppError::TaskNotFound(_)
            | AppError::MessageNotFound(_)
            | AppError::DecisionNotPending(_)) => (StatusCode::NOT_FOUND, error.to_string()),
            AppError::HumanApprovalRequired => (
                StatusCode::FORBIDDEN,
                "a verified workspace owner must perform this decision".into(),
            ),
            error @ (AppError::InvalidTransition { .. }
            | AppError::StaleState { .. }
            | AppError::PendingDecision
            | AppError::IdempotencyConflict(_)) => (StatusCode::CONFLICT, error.to_string()),
            AppError::Database(_) | AppError::Serialization(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "local storage operation failed".into(),
            ),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "local worker failed".into(),
            ),
        };
        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(value: AppError) -> Self {
        Self(value)
    }
}

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct FrontendAssets;

pub async fn serve(database_path: PathBuf, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let loopback_test = oidc_loopback_test_mode()?;
    let oidc_config = OidcConfig::from_env();
    let (address, api_host, web_origin, proxy_secret) = if let Some(config) = oidc_config.as_ref() {
        let bind = std::env::var("CODEXPT_BIND_ADDR")
            .map_err(|_| "CODEXPT_BIND_ADDR is required when OIDC is enabled")?
            .parse::<SocketAddr>()?;
        let api_host = std::env::var("CODEXPT_API_HOST")
            .map_err(|_| "CODEXPT_API_HOST is required when OIDC is enabled")?;
        let web_origin = std::env::var("CODEXPT_WEB_ORIGIN")
            .map_err(|_| "CODEXPT_WEB_ORIGIN is required when OIDC is enabled")?;
        let proxy_secret = match std::env::var("CODEXPT_PROXY_SECRET") {
            Ok(secret) if !loopback_test => {
                validate_proxy_secret(&secret)?;
                Some(secret)
            }
            Ok(_) => return Err("CODEXPT_PROXY_SECRET is not valid in loopback test mode".into()),
            Err(std::env::VarError::NotPresent) if loopback_test => None,
            Err(std::env::VarError::NotPresent) => {
                return Err("CODEXPT_PROXY_SECRET is required when hosted OIDC is enabled".into());
            }
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err("CODEXPT_PROXY_SECRET must be valid UTF-8".into());
            }
        };
        if loopback_test {
            validate_loopback_test_origin(config, bind, &api_host, &web_origin)?;
        } else {
            validate_hosted_origin(config, &api_host, &web_origin, true)?;
        }
        (bind, Some(api_host), Some(web_origin), proxy_secret)
    } else {
        if loopback_test {
            return Err("CODEXPT_OIDC_LOOPBACK_TEST requires a complete OIDC configuration".into());
        }
        if std::env::var_os("CODEXPT_BIND_ADDR").is_some() {
            return Err(
                "CODEXPT_BIND_ADDR is only valid with a complete OIDC configuration".into(),
            );
        }
        if std::env::var_os("CODEXPT_PROXY_SECRET").is_some() {
            return Err("CODEXPT_PROXY_SECRET requires hosted OIDC configuration".into());
        }
        (SocketAddr::from(([127, 0, 0, 1], port)), None, None, None)
    };
    if let Some(parent) = database_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let identity = if let Ok(database_url) = std::env::var("CODEXPT_DATABASE_URL") {
        if oidc_config.is_some() {
            return Err("OIDC currently requires the shared SQLite store; a split PostgreSQL identity database is not supported".into());
        }
        IdentityStore::connect(&database_url, 5).await?
    } else {
        IdentityStore::connect_sqlite(&database_path, 5).await?
    };
    let hosted = oidc_config.is_some() && !loopback_test;
    let listener = tokio::net::TcpListener::bind(address).await?;
    let app = router_with_identity_and_origins(
        database_path,
        port,
        Some(identity),
        oidc_config,
        api_host,
        web_origin,
        proxy_secret,
    );
    if loopback_test {
        eprintln!("CodeXPT OIDC loopback test listening on {address}");
        eprintln!("The test server is restricted to IPv4 loopback. Press Ctrl+C to stop.");
    } else if hosted {
        eprintln!("CodeXPT hosted API listening on {address}");
    } else {
        eprintln!("CodeXPT local dashboard: http://{address}");
        eprintln!("Only loopback clients are accepted. Press Ctrl+C to stop.");
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn validate_hosted_origin(
    config: &OidcConfig,
    api_host: &str,
    web_origin: &str,
    proxied: bool,
) -> Result<(), &'static str> {
    if api_host.is_empty()
        || !api_host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        || api_host.starts_with('.')
        || api_host.ends_with('.')
        || api_host.contains("..")
    {
        return Err("hosted OIDC requires a valid public host without scheme or path");
    }
    let public_host = web_origin
        .strip_prefix("https://")
        .filter(|host| valid_public_host(host))
        .ok_or("hosted OIDC requires a public HTTPS browser origin without a path")?;
    if !proxied && web_origin != format!("https://{api_host}") {
        return Err(
            "hosted beta requires the API host and browser origin to share one HTTPS origin",
        );
    }
    if config.redirect_uri != format!("https://{public_host}/api/auth/callback") {
        return Err("OIDC redirect URI must match the public browser callback exactly");
    }
    if config.success_url != web_origin && config.success_url != format!("{web_origin}/") {
        return Err("OIDC success URL must return to the hosted browser origin");
    }
    Ok(())
}

fn valid_public_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.starts_with('-')
        && !host.ends_with('-')
        && !host.contains("..")
}

fn validate_proxy_secret(secret: &str) -> Result<(), &'static str> {
    if secret.len() < 32
        || secret.trim() != secret
        || secret.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("CODEXPT_PROXY_SECRET must contain at least 32 non-whitespace bytes");
    }
    Ok(())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn oidc_loopback_test_mode() -> Result<bool, &'static str> {
    match std::env::var("CODEXPT_OIDC_LOOPBACK_TEST") {
        Ok(value) => parse_oidc_loopback_test_mode(Some(&value)),
        Err(std::env::VarError::NotPresent) => parse_oidc_loopback_test_mode(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("CODEXPT_OIDC_LOOPBACK_TEST must be the literal true or false")
        }
    }
}

fn parse_oidc_loopback_test_mode(value: Option<&str>) -> Result<bool, &'static str> {
    match value {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => Err("CODEXPT_OIDC_LOOPBACK_TEST must be the literal true or false"),
    }
}

fn validate_loopback_test_origin(
    config: &OidcConfig,
    bind: SocketAddr,
    api_host: &str,
    web_origin: &str,
) -> Result<(), &'static str> {
    let SocketAddr::V4(bind) = bind else {
        return Err("OIDC loopback test requires an exact 127.0.0.1 IPv4 bind");
    };
    if bind.ip() != &std::net::Ipv4Addr::LOCALHOST || bind.port() == 0 {
        return Err("OIDC loopback test requires an exact 127.0.0.1 bind and a fixed port");
    }
    let host = format!("127.0.0.1:{}", bind.port());
    let origin = format!("http://{host}");
    if api_host != host || web_origin != origin {
        return Err(
            "OIDC loopback Host and Origin must exactly match the configured loopback bind",
        );
    }
    if config.redirect_uri != format!("{origin}/api/auth/callback") {
        return Err("OIDC loopback callback must exactly match the configured loopback bind");
    }
    if config.success_url != origin && config.success_url != format!("{origin}/") {
        return Err("OIDC loopback success URL must return to the configured loopback origin");
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let terminate = async {
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(mut signal) => {
                    signal.recv().await;
                }
                Err(error) => {
                    eprintln!("could not register SIGTERM handler: {error}");
                    std::future::pending::<()>().await;
                }
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
fn router(database_path: PathBuf, port: u16) -> Router {
    router_with_identity(database_path, port, None, None)
}

#[cfg(test)]
fn router_with_identity(
    database_path: PathBuf,
    port: u16,
    identity: Option<IdentityStore>,
    oidc_config: Option<OidcConfig>,
) -> Router {
    router_with_identity_and_origins(database_path, port, identity, oidc_config, None, None, None)
}

fn router_with_identity_and_origins(
    database_path: PathBuf,
    port: u16,
    identity: Option<IdentityStore>,
    oidc_config: Option<OidcConfig>,
    api_host: Option<String>,
    web_origin: Option<String>,
    proxy_secret: Option<String>,
) -> Router {
    let state = AppState {
        database_path,
        port,
        identity,
        oidc: oidc_config,
        api_host,
        web_origin: web_origin.clone(),
        proxy_secret,
    };
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/health/ready", get(readiness))
        .route("/api/auth/status", get(auth_status))
        .route("/api/auth/login", get(auth_login))
        .route("/api/auth/callback", get(auth_callback))
        .route("/api/auth/me", get(auth_me))
        .route("/api/auth/logout", post(auth_logout))
        .route("/api/workspaces", get(list_workspaces))
        .route(
            "/api/workspaces/{workspace_id}/members",
            get(list_workspace_members),
        )
        .route(
            "/api/workspaces/{workspace_id}/invites",
            post(create_workspace_invite),
        )
        .route(
            "/api/workspaces/{workspace_id}/members/{user_id}",
            patch(change_workspace_member).delete(revoke_workspace_member),
        )
        .route(
            "/api/workspaces/{workspace_id}/transfer-ownership",
            post(transfer_workspace_ownership),
        )
        .route("/api/invites/accept", post(accept_workspace_invite))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/{task_id}", get(get_task))
        .route(
            "/api/tasks/{task_id}/checkpoints",
            get(list_context_checkpoints).post(create_context_checkpoint),
        )
        .route(
            "/api/tasks/{task_id}/checkpoints/{checkpoint_id}/resume",
            post(acknowledge_context_checkpoint),
        )
        .route("/api/tasks/{task_id}/messages", post(create_message))
        .route("/api/tasks/{task_id}/transition", post(transition_task))
        .route("/api/tasks/{task_id}/decisions", post(request_decision))
        .route(
            "/api/tasks/{task_id}/decisions/{decision_id}/answer",
            post(answer_decision),
        )
        .route("/", get(frontend_index))
        .route("/login", get(frontend_index))
        .route("/invite", get(frontend_index))
        .route("/codexpt.svg", get(frontend_favicon))
        .route("/favicon.svg", get(frontend_favicon_simple))
        .route("/assets/{*path}", get(frontend_asset))
        .route("/{asset}", get(frontend_public_asset))
        .fallback(get(not_found))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(middleware::from_fn_with_state(state, local_request_guard));
    if let Some(origin) = web_origin.and_then(|origin| HeaderValue::from_str(&origin).ok()) {
        return app.layer(
            CorsLayer::new()
                .allow_origin(origin)
                .allow_credentials(true)
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PATCH,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([
                    header::CONTENT_TYPE,
                    axum::http::HeaderName::from_static("x-csrf-token"),
                    axum::http::HeaderName::from_static("x-codexpt-workspace-id"),
                ]),
        );
    }
    app
}

fn oidc_state(state: &AppState) -> Option<OidcState> {
    Some(OidcState {
        store: state.identity.clone()?,
        config: state.oidc.clone(),
    })
}

async fn auth_status(State(state): State<AppState>) -> Response {
    let configured = state.identity.is_some() && state.oidc.is_some();
    let provider = state
        .oidc
        .as_ref()
        .map(oidc::OidcConfig::provider_label)
        .unwrap_or("proveedor de identidad");
    Json(serde_json::json!({"configured":configured,"provider":provider})).into_response()
}

async fn auth_login(State(state): State<AppState>) -> Response {
    match oidc_state(&state) {
        Some(state) => oidc::start(State(state)).await,
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "identity provider unavailable",
        )
            .into_response(),
    }
}

async fn auth_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<oidc::CallbackQuery>,
) -> Response {
    match oidc_state(&state) {
        Some(state) => oidc::callback(State(state), headers, Query(query)).await,
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "identity provider unavailable",
        )
            .into_response(),
    }
}

async fn auth_me(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match oidc_state(&state) {
        Some(state) => oidc::me(State(state), headers).await,
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn auth_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match oidc_state(&state) {
        Some(state) => oidc::logout(State(state), headers).await,
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

async fn local_request_guard(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if let Some(expected_secret) = state.proxy_secret.as_deref() {
        let supplied_values = request.headers().get_all(PROXY_SECRET_HEADER);
        let supplied_secret = if supplied_values.iter().count() == 1 {
            supplied_values
                .iter()
                .next()
                .and_then(|value| value.to_str().ok())
        } else {
            None
        };
        // Railway's public liveness probe must not reveal application readiness details.
        let is_public_liveness = request.method() == Method::GET && path == "/api/health";
        if !is_public_liveness
            && !matches!(supplied_secret, Some(value) if constant_time_eq(value.as_bytes(), expected_secret.as_bytes()))
        {
            return guarded_error(StatusCode::FORBIDDEN, "trusted application proxy required");
        }
    }
    let expected_host = state
        .api_host
        .clone()
        .unwrap_or_else(|| format!("127.0.0.1:{}", state.port));
    let alternate_host = if state.api_host.is_none() {
        Some(format!("localhost:{}", state.port))
    } else {
        None
    };
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if !matches!(host, Some(value) if value == expected_host || alternate_host.as_deref() == Some(value))
    {
        return guarded_error(StatusCode::FORBIDDEN, "invalid Host header");
    }

    if matches!(
        request.method(),
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    ) {
        let expected_origin = state
            .web_origin
            .clone()
            .unwrap_or_else(|| format!("http://{expected_host}"));
        let alternate_origin = format!("http://localhost:{}", state.port);
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok());
        if !matches!(origin, Some(value) if value == expected_origin || (state.web_origin.is_none() && value == alternate_origin))
        {
            return guarded_error(StatusCode::FORBIDDEN, "request origin is not allowed");
        }

        let content_type = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        if !matches!(content_type, Some(value) if value.split(';').next().is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json")))
        {
            return guarded_error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "application/json is required",
            );
        }
    }

    let mut context = RequestContext::default();
    let task_api = path.starts_with("/api/tasks");
    let workspace_api = path.starts_with("/api/workspaces/");
    let invite_accept = path == "/api/invites/accept";
    let logout_api = path == "/api/auth/logout";
    if state.oidc.is_some() && (task_api || workspace_api || invite_accept || logout_api) {
        let Some(identity) = state.identity.as_ref() else {
            return guarded_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "identity storage unavailable",
            );
        };
        let secure = state.oidc.as_ref().is_some_and(OidcConfig::secure_cookies);
        let session_cookie = if secure {
            "__Host-codexpt_session"
        } else {
            "codexpt_session"
        };
        let Some(token) = cookie_value(request.headers(), session_cookie) else {
            return guarded_error(StatusCode::UNAUTHORIZED, "sign in required");
        };
        let principal = match identity.resolve_session(&token).await {
            Ok(Some(principal)) => principal,
            Ok(None) => return guarded_error(StatusCode::UNAUTHORIZED, "sign in required"),
            Err(_) => {
                return guarded_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "session validation failed",
                );
            }
        };
        let needs_workspace = task_api || workspace_api;
        let workspace_id = if needs_workspace {
            let Some(workspace_id) = request
                .headers()
                .get("x-codexpt-workspace-id")
                .and_then(|value| value.to_str().ok())
            else {
                return guarded_error(StatusCode::BAD_REQUEST, "workspace selection required");
            };
            Some(workspace_id.to_owned())
        } else {
            None
        };
        let membership = if let Some(workspace_id) = workspace_id.as_deref() {
            match identity.membership(&principal.user_id, workspace_id).await {
                Ok(Some(membership)) => Some(membership),
                Ok(None) => return guarded_error(StatusCode::NOT_FOUND, "workspace not found"),
                Err(_) => {
                    return guarded_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "membership lookup failed",
                    );
                }
            }
        } else {
            None
        };
        let is_write = matches!(
            request.method(),
            &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
        );
        if is_write
            && membership
                .as_ref()
                .is_some_and(|membership| membership.role == WorkspaceRole::Viewer)
        {
            return guarded_error(StatusCode::FORBIDDEN, "workspace role cannot make changes");
        }
        if is_write
            && workspace_api
            && membership
                .as_ref()
                .is_some_and(|membership| membership.role != WorkspaceRole::Owner)
        {
            return guarded_error(StatusCode::FORBIDDEN, "workspace owner role required");
        }
        if is_write {
            let csrf_name = if secure {
                "__Host-codexpt_csrf"
            } else {
                "codexpt_csrf"
            };
            let cookie = cookie_value(request.headers(), csrf_name);
            let header_value = request
                .headers()
                .get("x-csrf-token")
                .and_then(|value| value.to_str().ok());
            let expected = derive_csrf_token(&token);
            if cookie.as_deref() != Some(expected.as_str())
                || header_value != Some(expected.as_str())
            {
                return guarded_error(StatusCode::FORBIDDEN, "CSRF validation failed");
            }
        }
        context = RequestContext {
            principal: Some(principal),
            workspace_id,
            role: membership.map(|membership| membership.role),
        };
    }
    request.extensions_mut().insert(context);
    let mut response = next.run(request).await;
    apply_security_headers(response.headers_mut());
    response
}

async fn list_workspaces(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (Some(identity), Some(config)) = (state.identity, state.oidc) else {
        return Json(Vec::<serde_json::Value>::new()).into_response();
    };
    let cookie_name = if config.secure_cookies() {
        "__Host-codexpt_session"
    } else {
        "codexpt_session"
    };
    let Some(token) = cookie_value(&headers, cookie_name) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let principal = match identity.resolve_session(&token).await {
        Ok(Some(principal)) => principal,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    match identity.list_workspaces_for_user(&principal.user_id).await {
        Ok(workspaces) => Json(workspaces.into_iter().map(|(workspace, role)| serde_json::json!({
            "workspace_id":workspace.workspace_id,"name":workspace.name,"role":role.as_str()
        })).collect::<Vec<_>>()).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn list_workspace_members(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(workspace_id): Path<String>,
) -> Response {
    if context.workspace_id.as_deref() != Some(workspace_id.as_str()) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(identity) = state.identity else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match identity.list_members(&workspace_id).await {
        Ok(members) => Json(members).into_response(),
        Err(error) => identity_error_response(error),
    }
}

async fn create_workspace_invite(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(workspace_id): Path<String>,
    Json(input): Json<CreateInviteRequest>,
) -> Response {
    let Some(principal) = context.principal else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if context.workspace_id.as_deref() != Some(workspace_id.as_str())
        || context.role != Some(WorkspaceRole::Owner)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(identity) = state.identity else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match identity
        .create_invite(
            &principal.user_id,
            Some(&principal.session_id),
            &workspace_id,
            &input.email,
            input.role,
        )
        .await
    {
        Ok(invite) => (StatusCode::CREATED, Json(invite)).into_response(),
        Err(error) => identity_error_response(error),
    }
}

async fn accept_workspace_invite(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<AcceptInviteRequest>,
) -> Response {
    let Some(principal) = context.principal else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(identity) = state.identity else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match identity
        .accept_invite(
            &input.token,
            &principal.user_id,
            Some(&principal.session_id),
        )
        .await
    {
        Ok(membership) => (StatusCode::CREATED, Json(membership)).into_response(),
        Err(error) => identity_error_response(error),
    }
}

async fn change_workspace_member(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path((workspace_id, user_id)): Path<(String, String)>,
    Json(input): Json<ChangeMemberRoleRequest>,
) -> Response {
    let Some(principal) = context.principal else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if context.workspace_id.as_deref() != Some(workspace_id.as_str())
        || context.role != Some(WorkspaceRole::Owner)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(identity) = state.identity else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match identity
        .change_membership_role(
            &principal.user_id,
            Some(&principal.session_id),
            &workspace_id,
            &user_id,
            input.role,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => identity_error_response(error),
    }
}

async fn revoke_workspace_member(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path((workspace_id, user_id)): Path<(String, String)>,
) -> Response {
    let Some(principal) = context.principal else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if context.workspace_id.as_deref() != Some(workspace_id.as_str())
        || context.role != Some(WorkspaceRole::Owner)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(identity) = state.identity else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match identity
        .revoke_membership(
            &principal.user_id,
            Some(&principal.session_id),
            &workspace_id,
            &user_id,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => identity_error_response(error),
    }
}

async fn transfer_workspace_ownership(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(workspace_id): Path<String>,
    Json(input): Json<TransferOwnershipRequest>,
) -> Response {
    let Some(principal) = context.principal else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if context.workspace_id.as_deref() != Some(workspace_id.as_str())
        || context.role != Some(WorkspaceRole::Owner)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(identity) = state.identity else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match identity
        .transfer_ownership(
            &principal.user_id,
            Some(&principal.session_id),
            &workspace_id,
            &input.new_owner_user_id,
        )
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => identity_error_response(error),
    }
}

fn identity_error_response(error: crate::identity::IdentityError) -> Response {
    use crate::identity::IdentityError;
    let status = match error {
        IdentityError::Invalid(_) => StatusCode::BAD_REQUEST,
        IdentityError::NotFound => StatusCode::NOT_FOUND,
        IdentityError::Forbidden => StatusCode::FORBIDDEN,
        IdentityError::Conflict => StatusCode::CONFLICT,
        IdentityError::Database(_) | IdentityError::Migration(_) | IdentityError::Random => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };
    (status, "identity operation could not be completed").into_response()
}

fn guarded_error(status: StatusCode, message: &'static str) -> Response {
    let mut response = (status, message).into_response();
    apply_security_headers(response.headers_mut());
    response
}

fn apply_security_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}

async fn readiness(State(state): State<AppState>) -> Response {
    let (Some(identity), Some(_oidc)) = (state.identity.as_ref(), state.oidc.as_ref()) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                serde_json::json!({"status": "not_ready", "reason": "identity is not configured"}),
            ),
        )
            .into_response();
    };

    if identity.ping().await.is_err() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"status": "not_ready", "reason": "identity storage is unavailable"})),
        )
            .into_response();
    }

    let database_path = state.database_path.clone();
    let task_store_is_available = tokio::task::spawn_blocking(move || {
        Store::open(database_path).and_then(|store| store.ping())
    })
    .await
    .is_ok_and(|result| result.is_ok());
    if !task_store_is_available {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "not_ready",
                "reason": "task storage is unavailable"
            })),
        )
            .into_response();
    }

    Json(serde_json::json!({"status": "ready", "mode": "hosted"})).into_response()
}

async fn list_tasks(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
) -> Result<Json<Vec<Task>>, ApiError> {
    let tasks = with_store(state, context.workspace_id, |store| store.list_tasks()).await?;
    Ok(Json(tasks))
}

async fn get_task(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskDetail>, ApiError> {
    let detail = with_store(state, context.workspace_id, move |store| {
        let task = store
            .get_task(&task_id)?
            .ok_or_else(|| AppError::TaskNotFound(task_id.clone()))?;
        Ok(TaskDetail {
            messages: store.list_messages(&task_id)?,
            events: store.list_events(&task_id)?,
            pending_decision: store.get_pending_decision(&task_id)?,
            task,
        })
    })
    .await?;
    Ok(Json(detail))
}

async fn list_context_checkpoints(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(task_id): Path<String>,
) -> Result<Json<Vec<ContextCheckpoint>>, ApiError> {
    let checkpoints = with_store(state, context.workspace_id, move |store| {
        store.list_context_checkpoints(&task_id)
    })
    .await?;
    Ok(Json(checkpoints))
}

async fn create_context_checkpoint(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(task_id): Path<String>,
    Json(input): Json<CreateContextCheckpointRequest>,
) -> Result<(StatusCode, Json<MutationResult<ContextCheckpoint>>), ApiError> {
    let actor = context
        .principal
        .as_ref()
        .map(|principal| format!("human:{}", principal.user_id))
        .unwrap_or_else(|| "human:cesar".into());
    let content = CheckpointContent {
        schema: CONTEXT_CHECKPOINT_SCHEMA.into(),
        version: CONTEXT_CHECKPOINT_VERSION.into(),
        checkpoint_id: input.checkpoint_id,
        task_id: task_id.clone(),
        source: input.source,
        source_completeness: "user_curated_partial".into(),
        goal: input.goal,
        decisions: input.decisions,
        completed: input.completed,
        blocked: input.blocked,
        next_actions: input.next_actions,
        constraints: input.constraints,
        artifacts: input.artifacts,
        context_capacity: "unknown; no ChatGPT web context telemetry is available".into(),
    };
    let checkpoint = with_store(state, context.workspace_id, move |store| {
        store.create_context_checkpoint(content, &actor)
    })
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(MutationResult { data: checkpoint }),
    ))
}

async fn acknowledge_context_checkpoint(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path((task_id, checkpoint_id)): Path<(String, String)>,
) -> Result<Json<MutationResult<ContextCheckpoint>>, ApiError> {
    let actor = context
        .principal
        .as_ref()
        .map(|principal| format!("human:{}", principal.user_id))
        .unwrap_or_else(|| "human:cesar".into());
    let checkpoint = with_store(state, context.workspace_id, move |store| {
        store.acknowledge_context_checkpoint(&task_id, &checkpoint_id, &actor)
    })
    .await?;
    Ok(Json(MutationResult { data: checkpoint }))
}

async fn create_task(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<MutationResult<Task>>), ApiError> {
    let principal = context.principal;
    let task = with_store(state, context.workspace_id, move |store| {
        if let Some(principal) = principal {
            store.create_task_verified_human(
                &input.title,
                &input.description,
                &principal.user_id,
                &principal.session_id,
            )
        } else {
            store.create_task(&input.title, &input.description, "human:cesar")
        }
    })
    .await?;
    Ok((StatusCode::CREATED, Json(MutationResult { data: task })))
}

async fn create_message(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(task_id): Path<String>,
    Json(input): Json<CreateMessageRequest>,
) -> Result<(StatusCode, Json<MutationResult<Message>>), ApiError> {
    let principal = context.principal;
    let sender = principal
        .as_ref()
        .map(|principal| format!("human:{}", principal.user_id))
        .unwrap_or(input.sender);
    let message = Message {
        schema_version: MESSAGE_SCHEMA_VERSION,
        message_id: Uuid::new_v4().to_string(),
        task_id,
        sender,
        recipient: input.recipient,
        correlation_id: Uuid::new_v4().to_string(),
        parent_id: None,
        status: MessageStatus::Recorded,
        body: input.body,
        evidence_refs: input.evidence_refs,
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    };
    let result = message.clone();
    with_store(state, context.workspace_id, move |store| {
        if let Some(principal) = principal {
            store.insert_message_verified_human(&message, &principal.user_id, &principal.session_id)
        } else {
            store.insert_message(&message)
        }
    })
    .await?;
    Ok((StatusCode::CREATED, Json(MutationResult { data: result })))
}

async fn transition_task(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(task_id): Path<String>,
    Json(input): Json<TransitionRequest>,
) -> Result<Json<MutationResult<Task>>, ApiError> {
    if input.next == TaskStatus::Approved
        && context
            .role
            .is_some_and(|role| role != WorkspaceRole::Owner)
    {
        return Err(ApiError(AppError::HumanApprovalRequired));
    }
    let principal = context.principal;
    let actor = principal
        .as_ref()
        .map(|principal| format!("human:{}", principal.user_id))
        .unwrap_or(input.actor);
    let correlation_id = Uuid::new_v4().to_string();
    let task = with_store(state, context.workspace_id, move |store| {
        if let Some(principal) = principal {
            let expected_version = input.expected_version.ok_or_else(|| {
                AppError::InvalidInput(
                    "expected_version is required for authenticated changes".into(),
                )
            })?;
            store.transition_task_verified_human(
                &task_id,
                input.expected,
                input.next,
                &principal.user_id,
                &principal.session_id,
                expected_version,
                &correlation_id,
            )
        } else {
            store.transition_task(
                &task_id,
                input.expected,
                input.next,
                &actor,
                &correlation_id,
            )
        }
    })
    .await?;
    Ok(Json(MutationResult { data: task }))
}

async fn request_decision(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(task_id): Path<String>,
    Json(input): Json<RequestDecisionRequest>,
) -> Result<(StatusCode, Json<MutationResult<HumanDecision>>), ApiError> {
    let decision_id = Uuid::new_v4().to_string();
    let principal = context.principal;
    let actor = principal
        .as_ref()
        .map(|principal| format!("human:{}", principal.user_id))
        .unwrap_or(input.actor);
    let correlation_id = Uuid::new_v4().to_string();
    let decision = with_store(state, context.workspace_id, move |store| {
        if let Some(principal) = principal {
            store.request_human_decision_verified_human(
                &task_id,
                &decision_id,
                &input.question,
                &principal.user_id,
                &principal.session_id,
                &correlation_id,
            )?;
        } else {
            store.request_human_decision(
                &task_id,
                &decision_id,
                &input.question,
                &actor,
                &correlation_id,
            )?;
        }
        store
            .get_pending_decision(&task_id)?
            .ok_or_else(|| AppError::DecisionNotPending(decision_id.clone()))
    })
    .await?;
    Ok((StatusCode::CREATED, Json(MutationResult { data: decision })))
}

async fn answer_decision(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path((task_id, decision_id)): Path<(String, String)>,
    Json(input): Json<AnswerDecisionRequest>,
) -> Result<Json<MutationResult<serde_json::Value>>, ApiError> {
    if context.principal.is_some()
        && !matches!(
            context.role,
            Some(WorkspaceRole::Owner | WorkspaceRole::Editor)
        )
    {
        return Err(ApiError(AppError::HumanApprovalRequired));
    }
    let principal = context.principal;
    let actor = principal
        .as_ref()
        .map(|principal| format!("human:{}", principal.user_id))
        .unwrap_or_else(|| "human:cesar".into());
    let correlation_id = Uuid::new_v4().to_string();
    let response_decision_id = decision_id.clone();
    with_store(state, context.workspace_id, move |store| {
        if let Some(principal) = principal {
            store.answer_human_decision_verified_human(
                &task_id,
                &decision_id,
                &input.answer,
                &principal.user_id,
                &principal.session_id,
                &correlation_id,
            )
        } else {
            store.answer_human_decision(
                &task_id,
                &decision_id,
                &input.answer,
                &actor,
                &correlation_id,
            )
        }
    })
    .await?;
    Ok(Json(MutationResult {
        data: serde_json::json!({"decision_id": response_decision_id, "status": "answered"}),
    }))
}

async fn frontend_index() -> Response {
    embedded_asset("index.html")
}

async fn frontend_favicon() -> Response {
    embedded_asset("codexpt.svg")
}

async fn frontend_favicon_simple() -> Response {
    embedded_asset("favicon.svg")
}

async fn frontend_asset(Path(path): Path<String>) -> Response {
    embedded_asset(&format!("assets/{path}"))
}

async fn frontend_public_asset(Path(asset): Path<String>) -> Response {
    match asset.as_str() {
        "not-found.webp" | "panda-face.webp" => embedded_asset(&asset),
        _ => not_found().await,
    }
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn embedded_asset(asset_path: &str) -> Response {
    let Some(asset) = FrontendAssets::get(asset_path) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let content_type = mime_guess::from_path(asset_path).first_or_octet_stream();
    let mut response = Response::new(asset.data.into());
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type.as_ref())
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    apply_security_headers(response.headers_mut());
    response
}

async fn with_store<T, F>(
    state: AppState,
    workspace_id: Option<String>,
    operation: F,
) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&mut Store) -> Result<T, AppError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut store = Store::open(state.database_path)?;
        store.set_workspace_context(workspace_id.as_deref());
        operation(&mut store)
    })
    .await
    .map_err(|error| ApiError(AppError::Internal(error.to_string())))?
    .map_err(ApiError)
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::Request};
    use tempfile::NamedTempFile;
    use tower::ServiceExt;

    use super::*;

    const PORT: u16 = 4173;
    const HOST: &str = "127.0.0.1:4173";
    const ORIGIN: &str = "http://127.0.0.1:4173";

    fn oidc_config_for_test() -> OidcConfig {
        OidcConfig {
            issuer: "https://issuer.example".into(),
            client_id: "client-test".into(),
            redirect_uri: "http://127.0.0.1:4173/api/auth/callback".into(),
            success_url: "http://127.0.0.1:4173/".into(),
        }
    }

    async fn authenticated_app() -> (
        Router,
        PathBuf,
        String,
        String,
        String,
        String,
        String,
        String,
    ) {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        Store::open(&path).unwrap();
        let identity = IdentityStore::connect_sqlite(&path, 5).await.unwrap();
        let owner = identity
            .provision_identity(
                "https://issuer.example",
                "owner",
                Some("owner@example.test"),
                true,
            )
            .await
            .unwrap();
        let editor = identity
            .provision_identity(
                "https://issuer.example",
                "editor",
                Some("editor@example.test"),
                true,
            )
            .await
            .unwrap();
        let viewer = identity
            .provision_identity(
                "https://issuer.example",
                "viewer",
                Some("viewer@example.test"),
                true,
            )
            .await
            .unwrap();
        let workspace = identity
            .create_workspace_for_owner(&owner, "Secure workspace")
            .await
            .unwrap();
        let invite = identity
            .create_invite(
                &owner,
                None,
                &workspace.workspace_id,
                "editor@example.test",
                crate::identity::InviteRole::Editor,
            )
            .await
            .unwrap();
        identity
            .accept_invite(&invite.raw_token, &editor, None)
            .await
            .unwrap();
        let viewer_invite = identity
            .create_invite(
                &owner,
                None,
                &workspace.workspace_id,
                "viewer@example.test",
                crate::identity::InviteRole::Viewer,
            )
            .await
            .unwrap();
        identity
            .accept_invite(&viewer_invite.raw_token, &viewer, None)
            .await
            .unwrap();
        let owner_session = identity.create_session(&owner).await.unwrap();
        let editor_session = identity.create_session(&editor).await.unwrap();
        let viewer_session = identity.create_session(&viewer).await.unwrap();
        let app = router_with_identity(
            path.clone(),
            PORT,
            Some(identity),
            Some(oidc_config_for_test()),
        );
        (
            app,
            path,
            workspace.workspace_id,
            owner_session.raw_token,
            owner_session.csrf_token,
            owner_session.session_id,
            editor_session.raw_token,
            viewer_session.raw_token,
        )
    }

    fn authenticated_request(
        method: Method,
        uri: &str,
        body: &str,
        workspace_id: &str,
        session: &str,
        csrf: &str,
    ) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, HOST)
            .header(header::ORIGIN, ORIGIN)
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::COOKIE,
                format!("codexpt_session={session}; codexpt_csrf={csrf}"),
            )
            .header("x-csrf-token", csrf)
            .header("x-codexpt-workspace-id", workspace_id)
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn only_owner_with_csrf_can_issue_workspace_invites() {
        let (app, path, workspace_id, owner_token, owner_csrf, _, editor_token, _) =
            authenticated_app().await;
        let editor_csrf = crate::identity::derive_csrf_token(&editor_token);
        let editor_attempt = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/workspaces/{workspace_id}/invites"),
                r#"{"email":"new@example.test","role":"VIEWER"}"#,
                &workspace_id,
                &editor_token,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(editor_attempt.status(), StatusCode::FORBIDDEN);

        let missing_csrf = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/workspaces/{workspace_id}/invites"),
                r#"{"email":"new@example.test","role":"VIEWER"}"#,
                &workspace_id,
                &owner_token,
                "wrong-token",
            ))
            .await
            .unwrap();
        assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

        let owner_invite = app
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/workspaces/{workspace_id}/invites"),
                r#"{"email":"new@example.test","role":"VIEWER"}"#,
                &workspace_id,
                &owner_token,
                &owner_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(owner_invite.status(), StatusCode::CREATED);
        let body = to_bytes(owner_invite.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let invite: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(!invite["raw_token"].as_str().unwrap().is_empty());
        std::fs::remove_file(path).ok();
    }

    #[tokio::test]
    async fn editor_cannot_change_membership_or_transfer_workspace_ownership() {
        let (app, _path, workspace_id, _owner, _owner_csrf, _, editor, _) =
            authenticated_app().await;
        let editor_csrf = crate::identity::derive_csrf_token(&editor);
        let routes = [
            (
                Method::PATCH,
                format!("/api/workspaces/{workspace_id}/members/target"),
                r#"{"role":"VIEWER"}"#,
            ),
            (
                Method::DELETE,
                format!("/api/workspaces/{workspace_id}/members/target"),
                "",
            ),
            (
                Method::POST,
                format!("/api/workspaces/{workspace_id}/transfer-ownership"),
                r#"{"new_owner_user_id":"target"}"#,
            ),
        ];
        for (method, uri, body) in routes {
            let response = app
                .clone()
                .oneshot(authenticated_request(
                    method,
                    &uri,
                    body,
                    &workspace_id,
                    &editor,
                    &editor_csrf,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
        }
    }

    #[tokio::test]
    async fn anonymous_cannot_read_or_mutate_hosted_workspace_routes() {
        let (app, _path, workspace_id, _, _, _, _, _) = authenticated_app().await;
        for (method, uri, body) in [
            (Method::GET, "/api/tasks", ""),
            (Method::GET, "/api/workspaces", ""),
            (
                Method::GET,
                &format!("/api/workspaces/{workspace_id}/members"),
                "",
            ),
            (
                Method::POST,
                "/api/tasks",
                r#"{"title":"Anonymous","description":"must fail"}"#,
            ),
        ] {
            let mut builder = Request::builder()
                .method(method.clone())
                .uri(uri)
                .header(header::HOST, HOST)
                .header(header::ORIGIN, ORIGIN)
                .header(header::CONTENT_TYPE, "application/json");
            if uri.starts_with("/api/tasks") {
                builder = builder.header("x-codexpt-workspace-id", &workspace_id);
            }
            let response = app
                .clone()
                .oneshot(builder.body(Body::from(body)).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "{method} {uri}"
            );
        }
    }

    #[tokio::test]
    async fn workspace_resource_ids_cannot_cross_tenant_boundaries() {
        let (app, path, workspace_a, owner_a_token, owner_a_csrf, _, _, _) =
            authenticated_app().await;
        let identity = IdentityStore::connect_sqlite(&path, 3).await.unwrap();
        let owner_b = identity
            .provision_identity(
                "https://issuer.example",
                "foreign-owner",
                Some("foreign-owner@example.test"),
                true,
            )
            .await
            .unwrap();
        let workspace_b = identity
            .create_workspace_for_owner(&owner_b, "Foreign workspace")
            .await
            .unwrap();
        let owner_b_session = identity.create_session(&owner_b).await.unwrap();

        let task_a = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                "/api/tasks",
                r#"{"title":"Workspace A task","description":"private to A"}"#,
                &workspace_a,
                &owner_a_token,
                &owner_a_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(task_a.status(), StatusCode::CREATED);
        let body = to_bytes(task_a.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let task_a: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_a_id = task_a["data"]["task_id"].as_str().unwrap().to_owned();

        let mut store = Store::open(&path).unwrap();
        store.set_workspace_context(Some(&workspace_b.workspace_id));
        let task_b = store
            .create_task("Workspace B task", "private to B", "human:foreign-owner")
            .unwrap();
        drop(store);

        for (workspace_id, token, csrf, task_id) in [
            (
                workspace_a.as_str(),
                owner_a_token.as_str(),
                owner_a_csrf.as_str(),
                task_b.task_id.as_str(),
            ),
            (
                workspace_b.workspace_id.as_str(),
                owner_b_session.raw_token.as_str(),
                owner_b_session.csrf_token.as_str(),
                task_a_id.as_str(),
            ),
        ] {
            let read = app
                .clone()
                .oneshot(authenticated_request(
                    Method::GET,
                    &format!("/api/tasks/{task_id}"),
                    "",
                    workspace_id,
                    token,
                    csrf,
                ))
                .await
                .unwrap();
            assert_eq!(read.status(), StatusCode::NOT_FOUND);

            let write = app
                .clone()
                .oneshot(authenticated_request(
                    Method::POST,
                    &format!("/api/tasks/{task_id}/messages"),
                    r#"{"sender":"human:attacker","recipient":"agent:codex","body":"must not cross"}"#,
                    workspace_id,
                    token,
                    csrf,
                ))
                .await
                .unwrap();
            assert_eq!(write.status(), StatusCode::NOT_FOUND);
        }

        let mut store = Store::open(&path).unwrap();
        store.set_workspace_context(Some(&workspace_b.workspace_id));
        assert!(store.list_messages(&task_b.task_id).unwrap().is_empty());
        store.set_workspace_context(Some(&workspace_a));
        assert!(store.list_messages(&task_a_id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn removing_membership_revokes_the_old_http_session_immediately() {
        let (app, path, workspace_id, owner_token, _, _, editor_token, _) =
            authenticated_app().await;
        let identity = IdentityStore::connect_sqlite(&path, 3).await.unwrap();
        let owner = identity
            .resolve_session(&owner_token)
            .await
            .unwrap()
            .unwrap();
        let editor = identity
            .resolve_session(&editor_token)
            .await
            .unwrap()
            .unwrap();
        identity
            .revoke_membership(
                &owner.user_id,
                Some(&owner.session_id),
                &workspace_id,
                &editor.user_id,
            )
            .await
            .unwrap();

        let response = app
            .oneshot(authenticated_request(
                Method::GET,
                "/api/tasks",
                "",
                &workspace_id,
                &editor_token,
                "irrelevant-for-GET",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn owner_http_membership_change_transfer_and_revoke_follow_current_roles() {
        let (app, path, workspace_id, owner_token, owner_csrf, _, editor_token, _) =
            authenticated_app().await;
        let identity = IdentityStore::connect_sqlite(&path, 3).await.unwrap();
        let owner = identity
            .resolve_session(&owner_token)
            .await
            .unwrap()
            .unwrap();
        let editor = identity
            .resolve_session(&editor_token)
            .await
            .unwrap()
            .unwrap();

        let role_changed = app
            .clone()
            .oneshot(authenticated_request(
                Method::PATCH,
                &format!("/api/workspaces/{workspace_id}/members/{}", editor.user_id),
                r#"{"role":"VIEWER"}"#,
                &workspace_id,
                &owner_token,
                &owner_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(role_changed.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            identity
                .membership(&editor.user_id, &workspace_id)
                .await
                .unwrap()
                .unwrap()
                .role,
            WorkspaceRole::Viewer
        );
        let stale_editor = app
            .clone()
            .oneshot(authenticated_request(
                Method::GET,
                "/api/tasks",
                "",
                &workspace_id,
                &editor_token,
                "irrelevant-for-GET",
            ))
            .await
            .unwrap();
        assert_eq!(stale_editor.status(), StatusCode::UNAUTHORIZED);

        let transferred = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/workspaces/{workspace_id}/transfer-ownership"),
                &format!(r#"{{"new_owner_user_id":"{}"}}"#, editor.user_id),
                &workspace_id,
                &owner_token,
                &owner_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(transferred.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            identity
                .membership(&editor.user_id, &workspace_id)
                .await
                .unwrap()
                .unwrap()
                .role,
            WorkspaceRole::Owner
        );
        assert_eq!(
            identity
                .membership(&owner.user_id, &workspace_id)
                .await
                .unwrap()
                .unwrap()
                .role,
            WorkspaceRole::Editor
        );

        let editor_owner_session = identity.create_session(&editor.user_id).await.unwrap();
        let revoked = app
            .clone()
            .oneshot(authenticated_request(
                Method::DELETE,
                &format!("/api/workspaces/{workspace_id}/members/{}", owner.user_id),
                "",
                &workspace_id,
                &editor_owner_session.raw_token,
                &editor_owner_session.csrf_token,
            ))
            .await
            .unwrap();
        assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
        assert!(
            identity
                .membership(&owner.user_id, &workspace_id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn invite_acceptance_requires_csrf_and_is_single_use_over_http() {
        let (app, path, workspace_id, owner_token, owner_csrf, _, _, _) = authenticated_app().await;
        let identity = IdentityStore::connect_sqlite(&path, 3).await.unwrap();
        let invitee_id = identity
            .provision_identity(
                "https://issuer.example",
                "http-invitee",
                Some("http-invitee@example.test"),
                true,
            )
            .await
            .unwrap();
        let invitee_session = identity.create_session(&invitee_id).await.unwrap();

        let created = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/workspaces/{workspace_id}/invites"),
                r#"{"email":"http-invitee@example.test","role":"EDITOR"}"#,
                &workspace_id,
                &owner_token,
                &owner_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let body = to_bytes(created.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let invite_token = created["raw_token"].as_str().unwrap().to_owned();

        let invalid_csrf = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                "/api/invites/accept",
                &format!(r#"{{"token":"{invite_token}"}}"#),
                &workspace_id,
                &invitee_session.raw_token,
                "invalid-csrf",
            ))
            .await
            .unwrap();
        assert_eq!(invalid_csrf.status(), StatusCode::FORBIDDEN);
        assert!(
            identity
                .membership(&invitee_id, &workspace_id)
                .await
                .unwrap()
                .is_none()
        );

        let accepted = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                "/api/invites/accept",
                &format!(r#"{{"token":"{invite_token}"}}"#),
                &workspace_id,
                &invitee_session.raw_token,
                &invitee_session.csrf_token,
            ))
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::CREATED);
        let membership = identity
            .membership(&invitee_id, &workspace_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(membership.role, WorkspaceRole::Editor);

        let replay = app
            .oneshot(authenticated_request(
                Method::POST,
                "/api/invites/accept",
                &format!(r#"{{"token":"{invite_token}"}}"#),
                &workspace_id,
                &invitee_session.raw_token,
                &invitee_session.csrf_token,
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            identity
                .membership(&invitee_id, &workspace_id)
                .await
                .unwrap()
                .unwrap()
                .role,
            WorkspaceRole::Editor
        );
    }

    #[tokio::test]
    async fn logout_revokes_session_and_emits_host_only_cookie_deletions() {
        let (app, _path, workspace_id, owner_token, owner_csrf, _, _, _) =
            authenticated_app().await;
        let logged_out = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                "/api/auth/logout",
                "",
                &workspace_id,
                &owner_token,
                &owner_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);
        let cookie_headers = logged_out
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect::<Vec<_>>();
        let session_cookie = cookie_headers
            .iter()
            .find(|value| value.starts_with("codexpt_session=;"))
            .unwrap();
        assert!(session_cookie.contains("HttpOnly"));
        assert!(session_cookie.contains("SameSite=Lax"));
        assert!(session_cookie.contains("Path=/"));
        assert!(session_cookie.contains("Max-Age=0"));
        assert!(!session_cookie.contains("Domain="));
        assert!(
            cookie_headers
                .iter()
                .any(|value| value.starts_with("codexpt_csrf=;"))
        );

        let replay = app
            .oneshot(authenticated_request(
                Method::GET,
                "/api/tasks",
                "",
                &workspace_id,
                &owner_token,
                &owner_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn viewer_is_read_only_at_the_http_boundary() {
        let (app, _path, workspace_id, _owner, _owner_csrf, _owner_session, _editor, viewer) =
            authenticated_app().await;
        let viewer_csrf = crate::identity::derive_csrf_token(&viewer);

        let read = app
            .clone()
            .oneshot(authenticated_request(
                Method::GET,
                "/api/tasks",
                "",
                &workspace_id,
                &viewer,
                &viewer_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(read.status(), StatusCode::OK);

        let task_write = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                "/api/tasks",
                r#"{"title":"Viewer write","description":"must be denied"}"#,
                &workspace_id,
                &viewer,
                &viewer_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(task_write.status(), StatusCode::FORBIDDEN);

        let membership_write = app
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/workspaces/{workspace_id}/invites"),
                r#"{"email":"new@example.test","role":"EDITOR"}"#,
                &workspace_id,
                &viewer,
                &viewer_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(membership_write.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn editor_can_answer_a_human_decision_with_verified_actor() {
        let (app, path, workspace_id, _owner, _owner_csrf, _, editor, _) =
            authenticated_app().await;
        let editor_csrf = crate::identity::derive_csrf_token(&editor);
        let created = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                "/api/tasks",
                r#"{"title":"Editor decision flow","description":"HTTP permission check"}"#,
                &workspace_id,
                &editor,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let body = to_bytes(created.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["data"]["task_id"].as_str().unwrap().to_owned();

        let proposed = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/tasks/{task_id}/transition"),
                r#"{"expected":"open","next":"proposed","actor":"human:owner-spoof","expected_version":1}"#,
                &workspace_id,
                &editor,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(proposed.status(), StatusCode::OK);

        let requested = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/tasks/{task_id}/decisions"),
                r#"{"question":"Choose a test option","actor":"human:owner-spoof"}"#,
                &workspace_id,
                &editor,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(requested.status(), StatusCode::CREATED);
        let body = to_bytes(requested.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let requested: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let decision_id = requested["data"]["decision_id"]
            .as_str()
            .unwrap()
            .to_owned();

        let answered = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/tasks/{task_id}/decisions/{decision_id}/answer"),
                r#"{"answer":"Use option A"}"#,
                &workspace_id,
                &editor,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(answered.status(), StatusCode::OK);

        let detail = app
            .clone()
            .oneshot(authenticated_request(
                Method::GET,
                &format!("/api/tasks/{task_id}"),
                "",
                &workspace_id,
                &editor,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
        let body = to_bytes(detail.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let detail: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(detail["pending_decision"].is_null());

        let event = Store::open(path)
            .unwrap()
            .list_events(&task_id)
            .unwrap()
            .into_iter()
            .find(|event| event.event_type == "human_decision_answered")
            .unwrap();
        assert!(event.actor.starts_with("human:"));
        assert_ne!(event.actor, "human:owner-spoof");
        assert_eq!(event.event_type, "human_decision_answered");
    }

    #[tokio::test]
    async fn every_viewer_write_route_is_denied_without_side_effects() {
        let (app, path, workspace_id, owner, owner_csrf, _, _editor, viewer) =
            authenticated_app().await;
        let viewer_csrf = crate::identity::derive_csrf_token(&viewer);
        let created = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                "/api/tasks",
                r#"{"title":"Seed task","description":"Keep unchanged"}"#,
                &workspace_id,
                &owner,
                &owner_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let body = to_bytes(created.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["data"]["task_id"].as_str().unwrap();

        let routes = [
            (
                Method::POST,
                "/api/tasks",
                r#"{"title":"Forbidden","description":""}"#,
            ),
            (
                Method::POST,
                &format!("/api/tasks/{task_id}/messages"),
                r#"{"sender":"human:viewer","recipient":"agent:codex","body":"Forbidden"}"#,
            ),
            (
                Method::POST,
                &format!("/api/tasks/{task_id}/transition"),
                r#"{"expected":"open","next":"proposed","actor":"human:viewer","expected_version":2}"#,
            ),
            (
                Method::POST,
                &format!("/api/tasks/{task_id}/decisions"),
                r#"{"question":"Forbidden?","actor":"human:viewer"}"#,
            ),
            (
                Method::POST,
                &format!("/api/tasks/{task_id}/decisions/missing/answer"),
                r#"{"answer":"Forbidden"}"#,
            ),
            (
                Method::POST,
                &format!("/api/tasks/{task_id}/checkpoints"),
                r#"{"checkpoint_id":"6fb8907e-7f4a-427c-835b-d4d39c833ffe","source":"unknown","goal":"Forbidden"}"#,
            ),
            (
                Method::POST,
                &format!(
                    "/api/tasks/{task_id}/checkpoints/6fb8907e-7f4a-427c-835b-d4d39c833ffe/resume"
                ),
                "{}",
            ),
            (
                Method::POST,
                &format!("/api/workspaces/{workspace_id}/invites"),
                r#"{"email":"blocked@example.test","role":"EDITOR"}"#,
            ),
            (
                Method::PATCH,
                &format!("/api/workspaces/{workspace_id}/members/target"),
                r#"{"role":"VIEWER"}"#,
            ),
            (
                Method::DELETE,
                &format!("/api/workspaces/{workspace_id}/members/target"),
                "",
            ),
            (
                Method::POST,
                &format!("/api/workspaces/{workspace_id}/transfer-ownership"),
                r#"{"new_owner_user_id":"target"}"#,
            ),
        ];

        for (method, uri, body) in routes {
            let response = app
                .clone()
                .oneshot(authenticated_request(
                    method.clone(),
                    uri,
                    body,
                    &workspace_id,
                    &viewer,
                    &viewer_csrf,
                ))
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::FORBIDDEN,
                "viewer unexpectedly accessed {method} {uri}"
            );
        }

        let store = Store::open(path).unwrap();
        let mut scoped = store;
        scoped.set_workspace_context(Some(&workspace_id));
        let tasks = scoped.list_tasks().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, task_id);
        assert_eq!(tasks[0].status, TaskStatus::Open);
        assert!(scoped.list_messages(task_id).unwrap().is_empty());
        assert!(scoped.get_pending_decision(task_id).unwrap().is_none());
        assert!(scoped.list_context_checkpoints(task_id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn csrf_origin_and_host_failures_reject_writes_before_storage_changes() {
        let (app, path, workspace_id, owner, owner_csrf, _, _editor, _) = authenticated_app().await;
        let valid_cookie = format!("codexpt_session={owner}; codexpt_csrf={owner_csrf}");
        let missing_csrf_cookie = format!("codexpt_session={owner}");
        let wrong_csrf_cookie = format!("codexpt_session={owner}; codexpt_csrf=wrong");
        let other_session_csrf = crate::identity::derive_csrf_token("another-session-secret");
        let cases = [
            (
                "missing session",
                None,
                Some(ORIGIN),
                Some(HOST),
                Some(owner_csrf.as_str()),
                Some("application/json"),
            ),
            (
                "missing csrf cookie",
                Some(missing_csrf_cookie.as_str()),
                Some(ORIGIN),
                Some(HOST),
                Some(owner_csrf.as_str()),
                Some("application/json"),
            ),
            (
                "missing csrf header",
                Some(valid_cookie.as_str()),
                Some(ORIGIN),
                Some(HOST),
                None,
                Some("application/json"),
            ),
            (
                "wrong csrf",
                Some(valid_cookie.as_str()),
                Some(ORIGIN),
                Some(HOST),
                Some("wrong"),
                Some("application/json"),
            ),
            (
                "wrong csrf cookie",
                Some(wrong_csrf_cookie.as_str()),
                Some(ORIGIN),
                Some(HOST),
                Some(owner_csrf.as_str()),
                Some("application/json"),
            ),
            (
                "csrf from another session",
                Some(valid_cookie.as_str()),
                Some(ORIGIN),
                Some(HOST),
                Some(other_session_csrf.as_str()),
                Some("application/json"),
            ),
            (
                "missing origin",
                Some(valid_cookie.as_str()),
                None,
                Some(HOST),
                Some(owner_csrf.as_str()),
                Some("application/json"),
            ),
            (
                "wrong origin",
                Some(valid_cookie.as_str()),
                Some("https://attacker.example"),
                Some(HOST),
                Some(owner_csrf.as_str()),
                Some("application/json"),
            ),
            (
                "wrong origin scheme",
                Some(valid_cookie.as_str()),
                Some("https://127.0.0.1:4173"),
                Some(HOST),
                Some(owner_csrf.as_str()),
                Some("application/json"),
            ),
            (
                "wrong origin port",
                Some(valid_cookie.as_str()),
                Some("http://127.0.0.1:4174"),
                Some(HOST),
                Some(owner_csrf.as_str()),
                Some("application/json"),
            ),
            (
                "wrong host",
                Some(valid_cookie.as_str()),
                Some(ORIGIN),
                Some("attacker.example"),
                Some(owner_csrf.as_str()),
                Some("application/json"),
            ),
            (
                "wrong content type",
                Some(valid_cookie.as_str()),
                Some(ORIGIN),
                Some(HOST),
                Some(owner_csrf.as_str()),
                Some("text/plain"),
            ),
        ];

        for (name, cookie, origin, host, csrf_header, content_type) in cases {
            let mut builder = Request::builder()
                .method(Method::POST)
                .uri("/api/tasks")
                .header("x-codexpt-workspace-id", &workspace_id);
            if let Some(value) = host {
                builder = builder.header(header::HOST, value);
            }
            if let Some(value) = origin {
                builder = builder.header(header::ORIGIN, value);
            }
            if let Some(value) = content_type {
                builder = builder.header(header::CONTENT_TYPE, value);
            }
            if let Some(value) = cookie {
                builder = builder.header(header::COOKIE, value);
            }
            if let Some(value) = csrf_header {
                builder = builder.header("x-csrf-token", value);
            }
            let response = app
                .clone()
                .oneshot(
                    builder
                        .body(Body::from(
                            r#"{"title":"Must not persist","description":""}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(
                matches!(
                    (name, response.status()),
                    ("missing session", StatusCode::UNAUTHORIZED)
                        | ("wrong content type", StatusCode::UNSUPPORTED_MEDIA_TYPE)
                        | (_, StatusCode::FORBIDDEN)
                ),
                "unexpected status for {name}: {}",
                response.status()
            );
        }

        let mut scoped = Store::open(path).unwrap();
        scoped.set_workspace_context(Some(&workspace_id));
        assert!(scoped.list_tasks().unwrap().is_empty());
    }

    fn app() -> (Router, PathBuf) {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        let app = router(path.clone(), PORT);
        (app, path)
    }

    fn request(method: Method, uri: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, HOST)
            .header(header::ORIGIN, ORIGIN)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn rejects_foreign_origin_before_mutating_tasks() {
        let (app, path) = app();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/tasks")
            .header(header::HOST, HOST)
            .header(header::ORIGIN, "https://attacker.example")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"title":"x","description":"y"}"#))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(Store::open(path).unwrap().list_tasks().unwrap().is_empty());
    }

    #[tokio::test]
    async fn host_and_origin_are_required_for_browser_writes() {
        let (app, path) = app();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/api/tasks")
            .header(header::HOST, "attacker.example")
            .header(header::ORIGIN, "http://attacker.example")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"title":"x","description":"y"}"#))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(Store::open(path).unwrap().list_tasks().unwrap().is_empty());
    }

    #[tokio::test]
    async fn task_and_message_api_persist_and_return_audit_detail() {
        let (app, path) = app();
        let response = app
            .clone()
            .oneshot(request(
                Method::POST,
                "/api/tasks",
                r#"{"title":"Review API","description":"Synthetic"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = to_bytes(response.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let task_id = created["data"]["task_id"].as_str().unwrap().to_string();

        let response = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("/api/tasks/{task_id}/messages"),
                r#"{"sender":"agent:codex","recipient":"agent:chatgpt","body":"Proposed change","evidence_refs":[{"label":"Test output","locator":"docs/test.md#result","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = app
            .oneshot(request(Method::GET, &format!("/api/tasks/{task_id}"), ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let detail: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(detail["messages"].as_array().unwrap().len(), 1);
        assert_eq!(
            detail["messages"][0]["evidence_refs"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(detail["events"].as_array().unwrap().len() >= 2);

        let persisted = Store::open(path).unwrap();
        assert_eq!(
            persisted.get_task(&task_id).unwrap().unwrap().title,
            "Review API"
        );
    }

    #[tokio::test]
    async fn context_checkpoint_api_freezes_exports_and_acknowledges_explicit_handoff() {
        let (app, path) = app();
        let created = app
            .clone()
            .oneshot(request(
                Method::POST,
                "/api/tasks",
                r#"{"title":"Continuity","description":"Synthetic context handoff"}"#,
            ))
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(
            &to_bytes(created.into_body(), MAX_REQUEST_BYTES)
                .await
                .unwrap(),
        )
        .unwrap();
        let task_id = created["data"]["task_id"].as_str().unwrap();
        let checkpoint_id = "6fb8907e-7f4a-427c-835b-d4d39c833ffe";
        let payload = serde_json::json!({
            "checkpoint_id": checkpoint_id,
            "source": "chatgpt_web",
            "goal": "Continue this synthetic review",
            "decisions": "No secrets in handoff",
            "completed": "The local API is implemented",
            "blocked": "Hosted deployment is not configured",
            "next_actions": "Verify the integration",
            "constraints": "Keep context capacity unknown",
            "artifacts": [{"label":"current commit","locator":"95aec3c"}]
        });
        let uri = format!("/api/tasks/{task_id}/checkpoints");
        let response = app
            .clone()
            .oneshot(request(Method::POST, &uri, &payload.to_string()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let created: serde_json::Value = serde_json::from_slice(
            &to_bytes(response.into_body(), MAX_REQUEST_BYTES)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(created["data"]["status"], "ready");
        assert_eq!(
            created["data"]["content"]["context_capacity"],
            "unknown; no ChatGPT web context telemetry is available"
        );
        assert_eq!(
            created["data"]["content"]["source_completeness"],
            "user_curated_partial"
        );

        let response = app
            .clone()
            .oneshot(request(Method::GET, &uri, ""))
            .await
            .unwrap();
        let checkpoints: serde_json::Value = serde_json::from_slice(
            &to_bytes(response.into_body(), MAX_REQUEST_BYTES)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(checkpoints.as_array().unwrap().len(), 1);

        let resume_uri = format!("/api/tasks/{task_id}/checkpoints/{checkpoint_id}/resume");
        let response = app
            .clone()
            .oneshot(request(Method::POST, &resume_uri, "{}"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let resumed: serde_json::Value = serde_json::from_slice(
            &to_bytes(response.into_body(), MAX_REQUEST_BYTES)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(resumed["data"]["status"], "resumed");
        let response = app
            .oneshot(request(Method::POST, &resume_uri, "{}"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let store = Store::open(path).unwrap();
        assert_eq!(store.list_context_checkpoints(task_id).unwrap().len(), 1);
        let events = store.list_events(task_id).unwrap();
        assert!(
            events
                .iter()
                .any(|event| event.event_type == "context_checkpoint_ready")
        );
        assert!(
            events
                .iter()
                .any(|event| event.event_type == "context_checkpoint_resumed")
        );
    }

    #[tokio::test]
    async fn rejects_oversized_json_payload() {
        let (app, path) = app();
        let body = format!(
            r#"{{"title":"{}","description":""}}"#,
            "a".repeat(MAX_REQUEST_BYTES)
        );
        let response = app
            .oneshot(request(Method::POST, "/api/tasks", &body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert!(Store::open(path).unwrap().list_tasks().unwrap().is_empty());
    }

    #[tokio::test]
    async fn human_decision_pauses_and_resumes_the_review_flow() {
        let (app, path) = app();
        let create = app
            .clone()
            .oneshot(request(
                Method::POST,
                "/api/tasks",
                r#"{"title":"Review","description":"Complete lifecycle"}"#,
            ))
            .await
            .unwrap();
        let body = to_bytes(create.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["data"]["task_id"].as_str().unwrap().to_owned();

        let proposed = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("/api/tasks/{task_id}/transition"),
                r#"{"expected":"open","next":"proposed","actor":"agent:codex"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(proposed.status(), StatusCode::OK);

        let message = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("/api/tasks/{task_id}/messages"),
                r#"{"sender":"agent:codex","recipient":"agent:chatgpt","body":"Proposal"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(message.status(), StatusCode::CREATED);

        let requested = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("/api/tasks/{task_id}/decisions"),
                r#"{"question":"Which option?","actor":"agent:codex"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(requested.status(), StatusCode::CREATED);
        let body = to_bytes(requested.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let decision: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let decision_id = decision["data"]["decision_id"].as_str().unwrap().to_owned();

        let blocked = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("/api/tasks/{task_id}/transition"),
                r#"{"expected":"proposed","next":"under_review","actor":"agent:chatgpt"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(blocked.status(), StatusCode::CONFLICT);

        let answered = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("/api/tasks/{task_id}/decisions/{decision_id}/answer"),
                r#"{"answer":"Option A"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(answered.status(), StatusCode::OK);

        let resumed = app
            .clone()
            .oneshot(request(
                Method::POST,
                &format!("/api/tasks/{task_id}/transition"),
                r#"{"expected":"proposed","next":"under_review","actor":"agent:chatgpt"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(resumed.status(), StatusCode::OK);

        let detail = app
            .oneshot(request(Method::GET, &format!("/api/tasks/{task_id}"), ""))
            .await
            .unwrap();
        let body = to_bytes(detail.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let detail: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(detail["task"]["status"], "under_review");
        assert!(detail["pending_decision"].is_null());
        assert_eq!(detail["messages"].as_array().unwrap().len(), 1);
        assert!(detail["events"].as_array().unwrap().len() >= 5);

        let store = Store::open(path).unwrap();
        assert_eq!(
            store.get_task(&task_id).unwrap().unwrap().status,
            TaskStatus::UnderReview
        );
    }

    #[tokio::test]
    async fn sets_security_headers_on_successful_responses() {
        let (app, _) = app();
        let response = app
            .oneshot(request(Method::GET, "/api/health", ""))
            .await
            .unwrap();
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert!(
            response.headers()[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'")
        );
    }

    #[tokio::test]
    async fn readiness_is_not_successful_when_hosted_identity_is_unconfigured() {
        let (app, _) = app();
        let response = app
            .oneshot(request(Method::GET, "/api/health/ready", ""))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn readiness_succeeds_when_identity_and_oidc_are_configured() {
        let (app, ..) = authenticated_app().await;
        let response = app
            .oneshot(request(Method::GET, "/api/health/ready", ""))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn oidc_loopback_test_mode_requires_explicit_and_matching_ipv4_loopback_settings() {
        assert!(!parse_oidc_loopback_test_mode(None).unwrap());
        assert!(!parse_oidc_loopback_test_mode(Some("false")).unwrap());
        assert!(parse_oidc_loopback_test_mode(Some("true")).unwrap());
        assert!(parse_oidc_loopback_test_mode(Some("1")).is_err());
        assert!(parse_oidc_loopback_test_mode(Some("TRUE")).is_err());

        let config = OidcConfig {
            issuer: "https://cognito-idp.us-east-1.amazonaws.com/us-east-1_test".into(),
            client_id: "test-public-client".into(),
            redirect_uri: "http://127.0.0.1:4173/api/auth/callback".into(),
            success_url: "http://127.0.0.1:4173/".into(),
        };
        let bind: SocketAddr = "127.0.0.1:4173".parse().unwrap();
        assert!(
            validate_loopback_test_origin(&config, bind, "127.0.0.1:4173", "http://127.0.0.1:4173")
                .is_ok()
        );

        for (address, host, origin) in [
            ("0.0.0.0:4173", "0.0.0.0:4173", "http://0.0.0.0:4173"),
            (
                "192.168.1.20:4173",
                "192.168.1.20:4173",
                "http://192.168.1.20:4173",
            ),
            ("[::1]:4173", "[::1]:4173", "http://[::1]:4173"),
            ("127.0.0.1:0", "127.0.0.1:0", "http://127.0.0.1:0"),
        ] {
            assert!(
                validate_loopback_test_origin(&config, address.parse().unwrap(), host, origin)
                    .is_err()
            );
        }
        assert!(
            validate_loopback_test_origin(&config, bind, "127.0.0.1:4174", "http://127.0.0.1:4174")
                .is_err()
        );
        assert!(
            validate_loopback_test_origin(
                &config,
                bind,
                "127.0.0.1:4173",
                "https://127.0.0.1:4173"
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn readiness_fails_when_the_hosted_identity_database_is_unavailable() {
        let file = NamedTempFile::new().unwrap();
        let path = file.into_temp_path().keep().unwrap();
        let identity = IdentityStore::connect_sqlite(&path, 1).await.unwrap();
        let state = AppState {
            database_path: path.clone(),
            port: PORT,
            identity: Some(identity.clone()),
            oidc: Some(OidcConfig {
                issuer: "https://issuer.example".into(),
                client_id: "client-test".into(),
                redirect_uri: "https://codexpt.example/api/auth/callback".into(),
                success_url: "https://codexpt.example/".into(),
            }),
            api_host: Some("codexpt.example".into()),
            web_origin: Some("https://codexpt.example".into()),
            proxy_secret: None,
        };
        identity.close_for_test().await;

        let response = readiness(State(state)).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        std::fs::remove_file(path).ok();
    }

    #[tokio::test]
    async fn readiness_fails_when_task_storage_is_unavailable() {
        let identity_file = NamedTempFile::new().unwrap();
        let identity_path = identity_file.into_temp_path().keep().unwrap();
        let identity = IdentityStore::connect_sqlite(&identity_path, 1)
            .await
            .unwrap();
        let task_db_directory = tempfile::tempdir().unwrap();
        let unavailable_database = task_db_directory.path().join("tasks.sqlite3");
        std::fs::create_dir(&unavailable_database).unwrap();
        let state = AppState {
            database_path: unavailable_database,
            port: PORT,
            identity: Some(identity),
            oidc: Some(OidcConfig {
                issuer: "https://issuer.example".into(),
                client_id: "client-test".into(),
                redirect_uri: "https://codexpt.example/api/auth/callback".into(),
                success_url: "https://codexpt.example/".into(),
            }),
            api_host: Some("codexpt.example".into()),
            web_origin: Some("https://codexpt.example".into()),
            proxy_secret: None,
        };

        let response = readiness(State(state)).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["reason"], "task storage is unavailable");
        std::fs::remove_file(identity_path).ok();
    }

    #[test]
    fn hosted_oidc_urls_must_match_the_same_public_origin() {
        let config = OidcConfig {
            issuer: "https://issuer.example".into(),
            client_id: "client-test".into(),
            redirect_uri: "https://codexpt.example/api/auth/callback".into(),
            success_url: "https://codexpt.example/".into(),
        };
        assert!(
            validate_hosted_origin(&config, "codexpt.example", "https://codexpt.example", false)
                .is_ok()
        );
        assert!(
            validate_hosted_origin(
                &config,
                "codexpt.example",
                "https://frontend.example",
                false,
            )
            .is_err()
        );

        let mut wrong_callback = config;
        wrong_callback.redirect_uri = "https://other.example/api/auth/callback".into();
        assert!(
            validate_hosted_origin(
                &wrong_callback,
                "codexpt.example",
                "https://codexpt.example",
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn hosted_oidc_allows_a_separate_frontend_origin_only_behind_the_vercel_proxy() {
        let config = OidcConfig {
            issuer: "https://issuer.example".into(),
            client_id: "client-test".into(),
            redirect_uri: "https://codexpt.jappi.net.pe/api/auth/callback".into(),
            success_url: "https://codexpt.jappi.net.pe/".into(),
        };
        assert!(
            validate_hosted_origin(
                &config,
                "codexpt-backend.up.railway.app",
                "https://codexpt.jappi.net.pe",
                true,
            )
            .is_ok()
        );
        assert!(
            validate_hosted_origin(
                &config,
                "codexpt-backend.up.railway.app",
                "https://codexpt.jappi.net.pe",
                false,
            )
            .is_err()
        );

        let mut wrong_callback = config;
        wrong_callback.redirect_uri =
            "https://codexpt-backend.up.railway.app/api/auth/callback".into();
        assert!(
            validate_hosted_origin(
                &wrong_callback,
                "codexpt-backend.up.railway.app",
                "https://codexpt.jappi.net.pe",
                true,
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn hosted_proxy_secret_blocks_direct_api_requests_but_allows_only_generic_liveness() {
        let config = OidcConfig {
            issuer: "https://issuer.example".into(),
            client_id: "client-test".into(),
            redirect_uri: "https://codexpt.jappi.net.pe/api/auth/callback".into(),
            success_url: "https://codexpt.jappi.net.pe/".into(),
        };
        let app = router_with_identity_and_origins(
            PathBuf::from("unused.sqlite3"),
            4173,
            None,
            Some(config.clone()),
            Some("codexpt-backend.up.railway.app".into()),
            Some("https://codexpt.jappi.net.pe".into()),
            Some("a-long-test-proxy-secret-that-is-not-production".into()),
        );

        let direct = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/auth/status")
                    .header(header::HOST, "codexpt-backend.up.railway.app")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(direct.status(), StatusCode::FORBIDDEN);

        let duplicate_credentials = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/auth/status")
                    .header(header::HOST, "codexpt-backend.up.railway.app")
                    .header(
                        PROXY_SECRET_HEADER,
                        "a-long-test-proxy-secret-that-is-not-production",
                    )
                    .header(PROXY_SECRET_HEADER, "attacker-controlled")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate_credentials.status(), StatusCode::FORBIDDEN);

        let proxied = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/auth/status")
                    .header(header::HOST, "codexpt-backend.up.railway.app")
                    .header(
                        PROXY_SECRET_HEADER,
                        "a-long-test-proxy-secret-that-is-not-production",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(proxied.status(), StatusCode::OK);

        let readiness = app
            .oneshot(
                Request::builder()
                    .uri("/api/health/ready")
                    .header(header::HOST, "codexpt-backend.up.railway.app")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(readiness.status(), StatusCode::FORBIDDEN);

        let liveness = router_with_identity_and_origins(
            PathBuf::from("/tmp/codexpt-test.sqlite3"),
            4173,
            None,
            Some(config),
            Some("codexpt-backend.up.railway.app".into()),
            Some("https://codexpt.jappi.net.pe".into()),
            Some("a-long-test-proxy-secret-that-is-not-production".into()),
        )
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .header(header::HOST, "codexpt-backend.up.railway.app")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(liveness.status(), StatusCode::OK);
        let body = to_bytes(liveness.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body, r#"{"status":"ok"}"#);
    }

    #[test]
    fn hosted_proxy_secret_requires_entropy_and_compares_without_early_exit() {
        assert!(validate_proxy_secret("short").is_err());
        assert!(validate_proxy_secret(" 01234567890123456789012345678901").is_err());
        assert!(validate_proxy_secret("01234567890123456789012345678901").is_ok());
        assert!(constant_time_eq(b"same-value", b"same-value"));
        assert!(!constant_time_eq(b"same-value", b"same-valuf"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[tokio::test]
    async fn authenticated_workspace_scope_blocks_idor_and_editor_approval_and_binds_owner_session()
    {
        let (app, path, workspace_id, owner_token, owner_csrf, owner_session_id, editor_token, _) =
            authenticated_app().await;
        let editor_csrf = crate::identity::derive_csrf_token(&editor_token);
        let created = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                "/api/tasks",
                r#"{"title":"Scoped decision","description":"test"}"#,
                &workspace_id,
                &editor_token,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let body = to_bytes(created.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["data"]["task_id"].as_str().unwrap().to_owned();

        let evidence = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/tasks/{task_id}/messages"),
                &format!(r#"{{"sender":"agent:codex","recipient":"agent:chatgpt","body":"Evidence","evidence_refs":[{{"label":"Test","locator":"docs/testing.md#result","sha256":"{}"}}]}}"#, "a".repeat(64)),
                &workspace_id,
                &editor_token,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(evidence.status(), StatusCode::CREATED);

        let denied_workspace = app
            .clone()
            .oneshot(authenticated_request(
                Method::GET,
                "/api/tasks",
                "",
                "foreign-workspace",
                &editor_token,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(denied_workspace.status(), StatusCode::NOT_FOUND);

        for (expected, next, expected_version) in [
            ("open", "proposed", 2),
            ("proposed", "under_review", 3),
            ("under_review", "resolved", 4),
        ] {
            let response = app
                .clone()
                .oneshot(authenticated_request(
                    Method::POST,
                    &format!("/api/tasks/{task_id}/transition"),
                    &format!(r#"{{"expected":"{expected}","next":"{next}","actor":"human:cesar","expected_version":{expected_version}}}"#),
                    &workspace_id,
                    &editor_token,
                    &editor_csrf,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
        let editor_approval = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/tasks/{task_id}/transition"),
                r#"{"expected":"resolved","next":"approved","actor":"human:cesar","expected_version":5}"#,
                &workspace_id,
                &editor_token,
                &editor_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(editor_approval.status(), StatusCode::FORBIDDEN);

        let owner_approval = app
            .clone()
            .oneshot(authenticated_request(
                Method::POST,
                &format!("/api/tasks/{task_id}/transition"),
                r#"{"expected":"resolved","next":"approved","actor":"agent:codex","expected_version":5}"#,
                &workspace_id,
                &owner_token,
                &owner_csrf,
            ))
            .await
            .unwrap();
        assert_eq!(owner_approval.status(), StatusCode::OK);
        let persisted = Store::open(path).unwrap();
        let approval = persisted
            .list_events(&task_id)
            .unwrap()
            .into_iter()
            .find(|event| {
                event.event_type == "task_transitioned" && event.details["to"] == "approved"
            })
            .unwrap();
        assert!(approval.actor.starts_with("human:"));
        assert_eq!(
            approval.details["verified_human"]["session_id"],
            owner_session_id
        );
        assert_eq!(approval.details["expected_version"], 5);
        assert_eq!(approval.details["resulting_version"], 6);
        assert_eq!(approval.details["evidence_refs"][0]["label"], "Test");
        let workspace_events = persisted
            .list_events(&task_id)
            .unwrap()
            .into_iter()
            .filter(|event| {
                matches!(
                    event.event_type.as_str(),
                    "task_created" | "message_recorded" | "task_transitioned"
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(workspace_events.len(), 6);
        for event in workspace_events {
            assert_eq!(
                event.details["verified_human"]["user_id"],
                event.actor.trim_start_matches("human:")
            );
            assert!(event.details["verified_human"]["session_id"].is_string());
        }
    }

    #[tokio::test]
    async fn hosted_origin_csrf_and_session_provenance_are_bound_to_exact_domains() {
        let database_dir = tempfile::tempdir().unwrap();
        let path = database_dir.path().join("hosted-identity.sqlite3");
        let identity = IdentityStore::connect_sqlite(&path, 2).await.unwrap();
        let (owner_id, workspace) = identity
            .bootstrap_first_owner(
                "https://issuer.example.test",
                "domain-owner",
                "owner@example.test",
                "Domain test",
            )
            .await
            .unwrap();
        let session = identity.create_session(&owner_id).await.unwrap();
        let csrf = crate::identity::derive_csrf_token(&session.raw_token);
        let config = OidcConfig {
            issuer: "https://issuer.example.test".into(),
            client_id: "codexpt-domain-test".into(),
            redirect_uri: "https://api.codexpt.example.test/api/auth/callback".into(),
            success_url: "https://codexpt.example.test/".into(),
        };
        let app = router_with_identity_and_origins(
            path.to_path_buf(),
            4173,
            Some(identity),
            Some(config),
            Some("api.codexpt.example.test".into()),
            Some("https://codexpt.example.test".into()),
            None,
        );
        let preflight = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/tasks")
                    .header(header::HOST, "api.codexpt.example.test")
                    .header(header::ORIGIN, "https://codexpt.example.test")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(
                        header::ACCESS_CONTROL_REQUEST_HEADERS,
                        "content-type,x-csrf-token,x-codexpt-workspace-id",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(preflight.status(), StatusCode::OK);
        assert_eq!(
            preflight.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "https://codexpt.example.test"
        );
        assert_eq!(
            preflight.headers()[header::ACCESS_CONTROL_ALLOW_CREDENTIALS],
            "true"
        );

        let preview = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/tasks")
                    .header(header::HOST, "api.codexpt.example.test")
                    .header(header::ORIGIN, "https://codexpt-git-pr.vercel.app")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            preview
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|value| value.to_str().ok()),
            Some("https://codexpt.example.test"),
            "CORS must return only the configured origin, never echo a preview origin"
        );

        let hostile_write = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/tasks")
                    .header(header::HOST, "api.codexpt.example.test")
                    .header(header::ORIGIN, "https://codexpt-git-pr.vercel.app")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"title":"Preview origin","description":""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(hostile_write.status(), StatusCode::FORBIDDEN);

        let missing_csrf = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/tasks")
                    .header(header::HOST, "api.codexpt.example.test")
                    .header(header::ORIGIN, "https://codexpt.example.test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::COOKIE,
                        format!("__Host-codexpt_session={}", session.raw_token),
                    )
                    .header("x-codexpt-workspace-id", &workspace.workspace_id)
                    .body(Body::from(r#"{"title":"Missing CSRF","description":""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);

        let created = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/tasks")
                    .header(header::HOST, "api.codexpt.example.test")
                    .header(header::ORIGIN, "https://codexpt.example.test")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        header::COOKIE,
                        format!(
                            "__Host-codexpt_session={}; __Host-codexpt_csrf={csrf}",
                            session.raw_token
                        ),
                    )
                    .header("x-codexpt-workspace-id", &workspace.workspace_id)
                    .header("x-csrf-token", csrf)
                    .body(Body::from(r#"{"title":"Correct domain","description":""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let body = to_bytes(created.into_body(), MAX_REQUEST_BYTES)
            .await
            .unwrap();
        let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = result["data"]["task_id"].as_str().unwrap();
        let store = Store::open(path).unwrap();
        let created_event = store
            .list_events(task_id)
            .unwrap()
            .into_iter()
            .find(|event| event.event_type == "task_created")
            .unwrap();
        assert_eq!(
            created_event.details["verified_human"]["session_id"],
            session.session_id
        );
    }
}
