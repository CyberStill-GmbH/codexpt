use std::{net::SocketAddr, path::PathBuf};

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{SecondsFormat, Utc};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppError, EvidenceRef, HumanDecision, Message, MessageStatus, Store, Task, TaskEvent,
    TaskStatus, domain::MESSAGE_SCHEMA_VERSION,
};

const MAX_REQUEST_BYTES: usize = 32 * 1024;

#[derive(Clone)]
struct AppState {
    database_path: PathBuf,
    port: u16,
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
}

#[derive(Debug, Deserialize)]
struct RequestDecisionRequest {
    question: String,
    actor: String,
}

#[derive(Debug, Deserialize)]
struct AnswerDecisionRequest {
    answer: String,
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
            error @ (AppError::InvalidTransition { .. }
            | AppError::StaleState { .. }
            | AppError::PendingDecision
            | AppError::IdempotencyConflict(_)
            | AppError::HumanApprovalRequired) => (StatusCode::CONFLICT, error.to_string()),
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
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    let app = router(database_path, port);
    eprintln!("CodeXPT local dashboard: http://{address}");
    eprintln!("Only loopback clients are accepted. Press Ctrl+C to stop.");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn router(database_path: PathBuf, port: u16) -> Router {
    let state = AppState {
        database_path,
        port,
    };
    Router::new()
        .route("/api/health", get(health))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/{task_id}", get(get_task))
        .route("/api/tasks/{task_id}/messages", post(create_message))
        .route("/api/tasks/{task_id}/transition", post(transition_task))
        .route("/api/tasks/{task_id}/decisions", post(request_decision))
        .route(
            "/api/tasks/{task_id}/decisions/{decision_id}/answer",
            post(answer_decision),
        )
        .route("/", get(frontend_index))
        .route("/codexpt.svg", get(frontend_favicon))
        .route("/assets/{*path}", get(frontend_asset))
        .fallback(get(not_found))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(middleware::from_fn_with_state(state, local_request_guard))
}

async fn local_request_guard(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let expected_host = format!("127.0.0.1:{}", state.port);
    let alternate_host = format!("localhost:{}", state.port);
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if !matches!(host, Some(value) if value == expected_host || value == alternate_host) {
        return guarded_error(StatusCode::FORBIDDEN, "invalid Host header");
    }

    if matches!(
        request.method(),
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    ) {
        let expected_origin = format!("http://{expected_host}");
        let alternate_origin = format!("http://{alternate_host}");
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok());
        if !matches!(origin, Some(value) if value == expected_origin || value == alternate_origin) {
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

    let mut response = next.run(request).await;
    apply_security_headers(response.headers_mut());
    response
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
    Json(serde_json::json!({"status": "ok", "mode": "local"}))
}

async fn list_tasks(State(state): State<AppState>) -> Result<Json<Vec<Task>>, ApiError> {
    let tasks = with_store(state, |store| store.list_tasks()).await?;
    Ok(Json(tasks))
}

async fn get_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskDetail>, ApiError> {
    let detail = with_store(state, move |store| {
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

async fn create_task(
    State(state): State<AppState>,
    Json(input): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<MutationResult<Task>>), ApiError> {
    let task = with_store(state, move |store| {
        store.create_task(&input.title, &input.description, "human:cesar")
    })
    .await?;
    Ok((StatusCode::CREATED, Json(MutationResult { data: task })))
}

async fn create_message(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(input): Json<CreateMessageRequest>,
) -> Result<(StatusCode, Json<MutationResult<Message>>), ApiError> {
    let message = Message {
        schema_version: MESSAGE_SCHEMA_VERSION,
        message_id: Uuid::new_v4().to_string(),
        task_id,
        sender: input.sender,
        recipient: input.recipient,
        correlation_id: Uuid::new_v4().to_string(),
        parent_id: None,
        status: MessageStatus::Recorded,
        body: input.body,
        evidence_refs: input.evidence_refs,
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    };
    let result = message.clone();
    with_store(state, move |store| store.insert_message(&message)).await?;
    Ok((StatusCode::CREATED, Json(MutationResult { data: result })))
}

async fn transition_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(input): Json<TransitionRequest>,
) -> Result<Json<MutationResult<Task>>, ApiError> {
    let correlation_id = Uuid::new_v4().to_string();
    let task = with_store(state, move |store| {
        store.transition_task(
            &task_id,
            input.expected,
            input.next,
            &input.actor,
            &correlation_id,
        )
    })
    .await?;
    Ok(Json(MutationResult { data: task }))
}

async fn request_decision(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(input): Json<RequestDecisionRequest>,
) -> Result<(StatusCode, Json<MutationResult<HumanDecision>>), ApiError> {
    let decision_id = Uuid::new_v4().to_string();
    let correlation_id = Uuid::new_v4().to_string();
    let decision = with_store(state, move |store| {
        store.request_human_decision(
            &task_id,
            &decision_id,
            &input.question,
            &input.actor,
            &correlation_id,
        )?;
        store
            .get_pending_decision(&task_id)?
            .ok_or_else(|| AppError::DecisionNotPending(decision_id.clone()))
    })
    .await?;
    Ok((StatusCode::CREATED, Json(MutationResult { data: decision })))
}

async fn answer_decision(
    State(state): State<AppState>,
    Path((task_id, decision_id)): Path<(String, String)>,
    Json(input): Json<AnswerDecisionRequest>,
) -> Result<Json<MutationResult<serde_json::Value>>, ApiError> {
    let correlation_id = Uuid::new_v4().to_string();
    let response_decision_id = decision_id.clone();
    with_store(state, move |store| {
        store.answer_human_decision(
            &task_id,
            &decision_id,
            &input.answer,
            "human:cesar",
            &correlation_id,
        )
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

async fn frontend_asset(Path(path): Path<String>) -> Response {
    embedded_asset(&format!("assets/{path}"))
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

async fn with_store<T, F>(state: AppState, operation: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&mut Store) -> Result<T, AppError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut store = Store::open(state.database_path)?;
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
}
