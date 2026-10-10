use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use openidconnect::{
    AuthorizationCode, ClientId, CsrfToken, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    Scope, TokenResponse,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
};
use serde::{Deserialize, Serialize};

use super::store::{IdentityStore, SessionCookie};

const SESSION_COOKIE: &str = "__Host-codexpt_session";
const DEV_SESSION_COOKIE: &str = "codexpt_session";
const CSRF_COOKIE: &str = "__Host-codexpt_csrf";
const DEV_CSRF_COOKIE: &str = "codexpt_csrf";
const OIDC_COOKIE: &str = "__Host-codexpt_oidc";
const DEV_OIDC_COOKIE: &str = "codexpt_oidc";

#[derive(Clone, Debug)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub success_url: String,
}

impl OidcConfig {
    pub fn from_env() -> Option<Self> {
        let config = Self {
            issuer: std::env::var("CODEXPT_OIDC_ISSUER").ok()?,
            client_id: std::env::var("CODEXPT_OIDC_CLIENT_ID").ok()?,
            redirect_uri: std::env::var("CODEXPT_OIDC_REDIRECT_URI").ok()?,
            success_url: std::env::var("CODEXPT_LOGIN_SUCCESS_URL").ok()?,
        };
        config.is_valid().then_some(config)
    }

    pub fn is_valid(&self) -> bool {
        let redirect_is_safe = self.redirect_uri.starts_with("https://")
            || self.redirect_uri.starts_with("http://127.0.0.1:")
            || self.redirect_uri.starts_with("http://localhost:");
        let success_is_safe = self.success_url.starts_with("https://")
            || self.success_url.starts_with("http://127.0.0.1:")
            || self.success_url.starts_with("http://localhost:");
        !self.issuer.is_empty()
            && self.issuer.starts_with("https://")
            && !self.client_id.is_empty()
            && redirect_is_safe
            && success_is_safe
    }

    pub fn secure_cookies(&self) -> bool {
        self.redirect_uri.starts_with("https://")
    }
}

#[derive(Clone)]
pub struct OidcState {
    pub store: IdentityStore,
    pub config: Option<OidcConfig>,
}

#[derive(Serialize)]
pub struct AuthStatus {
    pub configured: bool,
    pub provider: &'static str,
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

pub async fn status(State(state): State<OidcState>) -> Json<AuthStatus> {
    Json(AuthStatus {
        configured: state.config.is_some(),
        provider: "OpenID Connect",
    })
}

pub async fn start(State(state): State<OidcState>) -> Response {
    let Some(config) = state.config else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error":"La conexión de identidad todavía no está configurada."})),
        )
            .into_response();
    };
    match begin_login(&state.store, &config).await {
        Ok((url, state_token)) => {
            let mut response = Redirect::temporary(&url).into_response();
            let secure = config.secure_cookies();
            let cookie_name = if secure { OIDC_COOKIE } else { DEV_OIDC_COOKIE };
            let secure_flag = if secure { "; Secure" } else { "" };
            if let Ok(value) = HeaderValue::from_str(&format!(
                "{cookie_name}={state_token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=600{secure_flag}"
            )) {
                response.headers_mut().append(header::SET_COOKIE, value);
            }
            response
        }
        Err(error) => {
            tracing_error("OIDC login initialization failed", &error);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "identity provider unavailable",
            )
                .into_response()
        }
    }
}

pub async fn callback(
    State(state): State<OidcState>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let Some(config) = state.config else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Some(state_token) = query.state.as_deref() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let secure = config.secure_cookies();
    let state_cookie = if secure { OIDC_COOKIE } else { DEV_OIDC_COOKIE };
    if cookie_value(&headers, state_cookie).as_deref() != Some(state_token) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let flow = match state.store.consume_auth_transaction(state_token).await {
        Ok(Some(flow)) => flow,
        Ok(None) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => {
            tracing_error("OIDC transaction lookup failed", &error);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    if query.error.is_some() {
        return redirect_with_cleared_oidc_cookie(&config.success_url, secure, None);
    }
    let Some(code) = query.code else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match finish_login(&state.store, &config, &code, flow).await {
        Ok(session) => {
            redirect_with_cleared_oidc_cookie(&config.success_url, secure, Some(session))
        }
        Err(error) => {
            tracing_error("OIDC callback validation failed", &error);
            (StatusCode::UNAUTHORIZED, "identity verification failed").into_response()
        }
    }
}

pub async fn logout(State(state): State<OidcState>, headers: HeaderMap) -> Response {
    let secure = state
        .config
        .as_ref()
        .is_some_and(OidcConfig::secure_cookies);
    let cookie_name = if secure {
        SESSION_COOKIE
    } else {
        DEV_SESSION_COOKIE
    };
    if let Some(token) = cookie_value(&headers, cookie_name)
        && let Err(error) = state.store.revoke_session(&token).await
    {
        tracing_error("session revocation failed", &error);
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    let secure_flag = if secure { "; Secure" } else { "" };
    if let Ok(cookie) = HeaderValue::from_str(&format!(
        "{cookie_name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure_flag}"
    )) {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
    let csrf_name = if secure { CSRF_COOKIE } else { DEV_CSRF_COOKIE };
    if let Ok(cookie) = HeaderValue::from_str(&format!(
        "{csrf_name}=; Path=/; SameSite=Lax; Max-Age=0{secure_flag}"
    )) {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
    response
}

pub async fn me(State(state): State<OidcState>, headers: HeaderMap) -> Response {
    let secure = state
        .config
        .as_ref()
        .is_some_and(OidcConfig::secure_cookies);
    let cookie_name = if secure {
        SESSION_COOKIE
    } else {
        DEV_SESSION_COOKIE
    };
    let Some(token) = cookie_value(&headers, cookie_name) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    match state.store.resolve_session(&token).await {
        Ok(Some(principal)) => Json(serde_json::json!({
            "user_id":principal.user_id,
            "email":principal.email,
            "csrf_token":super::store::derive_csrf_token(&token)
        }))
        .into_response(),
        Ok(None) => StatusCode::UNAUTHORIZED.into_response(),
        Err(error) => {
            tracing_error("session validation failed", &error);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn begin_login(
    store: &IdentityStore,
    config: &OidcConfig,
) -> Result<(String, String), String> {
    let (metadata, http) = oidc_provider(config).await?;
    let client =
        CoreClient::from_provider_metadata(metadata, ClientId::new(config.client_id.clone()), None)
            .set_redirect_uri(
                openidconnect::RedirectUrl::new(config.redirect_uri.clone())
                    .map_err(|error| error.to_string())?,
            );
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, csrf, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("openid".into()))
        .add_scope(Scope::new("email".into()))
        .set_pkce_challenge(challenge)
        .url();
    let state_token = csrf.secret().to_owned();
    store
        .create_auth_transaction(&state_token, nonce.secret(), verifier.secret())
        .await
        .map_err(|error| error.to_string())?;
    drop(http);
    Ok((url.to_string(), state_token))
}

async fn finish_login(
    store: &IdentityStore,
    config: &OidcConfig,
    code: &str,
    flow: super::store::AuthTransaction,
) -> Result<SessionCookie, String> {
    let (metadata, http) = oidc_provider(config).await?;
    let client =
        CoreClient::from_provider_metadata(metadata, ClientId::new(config.client_id.clone()), None)
            .set_redirect_uri(
                openidconnect::RedirectUrl::new(config.redirect_uri.clone())
                    .map_err(|error| error.to_string())?,
            );
    let token_response = client
        .exchange_code(AuthorizationCode::new(code.to_owned()))
        .map_err(|error| error.to_string())?
        .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier))
        .request_async(&http)
        .await
        .map_err(|error| error.to_string())?;
    let id_token = token_response
        .id_token()
        .ok_or_else(|| "provider did not return an ID token".to_owned())?;
    let claims = id_token
        .claims(&client.id_token_verifier(), &Nonce::new(flow.nonce))
        .map_err(|error| error.to_string())?;
    let subject = claims.subject().as_str();
    let email = claims.email().map(|value| value.as_str());
    let email_verified = claims.email_verified().unwrap_or(false);
    let user_id = store
        .authenticate_identity(config.issuer.as_str(), subject, email, email_verified)
        .await
        .map_err(|error| error.to_string())?;
    store
        .create_session(&user_id)
        .await
        .map_err(|error| error.to_string())
}

async fn oidc_provider(
    config: &OidcConfig,
) -> Result<(CoreProviderMetadata, openidconnect::reqwest::Client), String> {
    let http = openidconnect::reqwest::ClientBuilder::new()
        .redirect(openidconnect::reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|error| error.to_string())?;
    let issuer = IssuerUrl::new(config.issuer.clone()).map_err(|error| error.to_string())?;
    let metadata = CoreProviderMetadata::discover_async(issuer, &http)
        .await
        .map_err(|error| error.to_string())?;
    Ok((metadata, http))
}

fn redirect_with_cleared_oidc_cookie(
    target: &str,
    secure: bool,
    session: Option<SessionCookie>,
) -> Response {
    let mut response = Redirect::to(target).into_response();
    let oidc_cookie = if secure { OIDC_COOKIE } else { DEV_OIDC_COOKIE };
    let session_cookie = if secure {
        SESSION_COOKIE
    } else {
        DEV_SESSION_COOKIE
    };
    let csrf_cookie = if secure { CSRF_COOKIE } else { DEV_CSRF_COOKIE };
    let secure_flag = if secure { "; Secure" } else { "" };
    let mut cookies = vec![format!(
        "{oidc_cookie}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure_flag}"
    )];
    if let Some(session) = session {
        cookies.push(format!(
            "{csrf_cookie}={}; Path=/; SameSite=Lax; Max-Age=43200{secure_flag}",
            session.csrf_token
        ));
        cookies.push(format!(
            "{session_cookie}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=43200{secure_flag}",
            session.raw_token
        ));
    }
    for cookie in cookies {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

pub(crate) fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_owned())
}

fn tracing_error(context: &str, error: &impl std::fmt::Display) {
    eprintln!("{context}: {error}");
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use axum::{
        Form, Router,
        extract::{Query as AxumQuery, State as AxumState},
        routing::{get, post},
    };
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use chrono::Utc;
    use openidconnect::url::Url;
    use rsa::{
        RsaPrivateKey, RsaPublicKey,
        pkcs1v15::SigningKey,
        rand_core::OsRng,
        signature::{SignatureEncoding, Signer},
        traits::PublicKeyParts,
    };
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use tokio::net::TcpListener;

    use super::*;

    const CLIENT_ID: &str = "codexpt-test-client";
    const SUBJECT: &str = "test-owner-subject";
    const EMAIL: &str = "owner@example.test";

    #[derive(Clone)]
    struct ProviderState {
        issuer: String,
        auth: Arc<Mutex<Option<AuthorizationRequest>>>,
        private_key: Arc<RsaPrivateKey>,
        jwks: serde_json::Value,
    }

    #[derive(Clone)]
    struct AuthorizationRequest {
        nonce: String,
        code_challenge: String,
    }

    #[derive(Deserialize)]
    struct AuthorizationQuery {
        client_id: String,
        response_type: String,
        redirect_uri: String,
        state: String,
        nonce: String,
        code_challenge: String,
        code_challenge_method: String,
    }

    async fn discovery(AxumState(state): AxumState<ProviderState>) -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "issuer": state.issuer,
            "authorization_endpoint": format!("{}/authorize", state.issuer),
            "token_endpoint": format!("{}/token", state.issuer),
            "jwks_uri": format!("{}/jwks", state.issuer),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
            "grant_types_supported": ["authorization_code"],
            "scopes_supported": ["openid", "email"],
            "token_endpoint_auth_methods_supported": ["none"]
        }))
    }

    async fn jwks(AxumState(state): AxumState<ProviderState>) -> Json<serde_json::Value> {
        Json(state.jwks)
    }

    async fn authorize(
        AxumState(state): AxumState<ProviderState>,
        AxumQuery(query): AxumQuery<AuthorizationQuery>,
    ) -> StatusCode {
        assert_eq!(query.client_id, CLIENT_ID);
        assert_eq!(query.response_type, "code");
        assert!(query.redirect_uri.ends_with("/api/auth/callback"));
        assert!(!query.state.is_empty());
        assert_eq!(query.code_challenge_method, "S256");
        *state.auth.lock().unwrap() = Some(AuthorizationRequest {
            nonce: query.nonce,
            code_challenge: query.code_challenge,
        });
        StatusCode::NO_CONTENT
    }

    async fn token(
        AxumState(state): AxumState<ProviderState>,
        headers: HeaderMap,
        Form(form): Form<std::collections::HashMap<String, String>>,
    ) -> Response {
        if headers.contains_key(header::AUTHORIZATION)
            || form.get("client_id").map(String::as_str) != Some(CLIENT_ID)
            || form.get("grant_type").map(String::as_str) != Some("authorization_code")
        {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error":"invalid_client"})),
            )
                .into_response();
        }
        let authorization = state.auth.lock().unwrap().clone();
        let Some(authorization) = authorization else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"invalid_grant"})),
            )
                .into_response();
        };
        let Some(verifier) = form.get("code_verifier") else {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"invalid_grant"})),
            )
                .into_response();
        };
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        if challenge != authorization.code_challenge {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"invalid_grant"})),
            )
                .into_response();
        }
        let now = Utc::now().timestamp();
        let nonce = if form.get("code").map(String::as_str) == Some("wrong-nonce") {
            "invalid-nonce".to_owned()
        } else {
            authorization.nonce
        };
        let claims = serde_json::json!({
            "iss": state.issuer,
            "sub": SUBJECT,
            "aud": CLIENT_ID,
            "iat": now,
            "exp": now + 300,
            "nonce": nonce,
            "email": EMAIL,
            "email_verified": true
        });
        let id_token = sign_id_token(&state.private_key, claims);
        Json(serde_json::json!({
            "access_token":"fixture-access-token",
            "token_type":"Bearer",
            "expires_in":300,
            "id_token":id_token
        }))
        .into_response()
    }

    fn sign_id_token(key: &RsaPrivateKey, claims: serde_json::Value) -> String {
        let header = serde_json::json!({"alg":"RS256","kid":"codexpt-test-key","typ":"JWT"});
        let signing_input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap()),
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        );
        let signature = SigningKey::<Sha256>::new(key.clone()).sign(signing_input.as_bytes());
        format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
    }

    async fn start_provider() -> (ProviderState, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let private_key = Arc::new(RsaPrivateKey::new(&mut OsRng, 2048).unwrap());
        let public_key = RsaPublicKey::from(private_key.as_ref());
        let state = ProviderState {
            issuer: issuer.clone(),
            auth: Arc::new(Mutex::new(None)),
            private_key,
            jwks: serde_json::json!({"keys":[{
                "kty":"RSA",
                "use":"sig",
                "alg":"RS256",
                "kid":"codexpt-test-key",
                "n":URL_SAFE_NO_PAD.encode(public_key.n().to_bytes_be()),
                "e":URL_SAFE_NO_PAD.encode(public_key.e().to_bytes_be())
            }]}),
        };
        let app = Router::new()
            .route("/.well-known/openid-configuration", get(discovery))
            .route("/authorize", get(authorize))
            .route("/jwks", get(jwks))
            .route("/token", post(token))
            .with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (state, task)
    }

    async fn begin_test_login(state: OidcState) -> (String, String, String) {
        let response = start(AxumState(state)).await;
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        let url = Url::parse(location).unwrap();
        let state_token = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        let oidc_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let provider_url = format!("{}?{}", url.path(), url.query().unwrap());
        (
            state_token,
            oidc_cookie,
            format!("{}{}", url.origin().ascii_serialization(), provider_url),
        )
    }

    async fn authorize_test_login(provider: &ProviderState, location: &str) {
        let response = openidconnect::reqwest::Client::new()
            .get(location)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(provider.auth.lock().unwrap().is_some());
    }

    async fn call_callback(
        state: OidcState,
        state_token: String,
        cookie: String,
        code: &str,
    ) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
        callback(
            AxumState(state),
            headers,
            AxumQuery(CallbackQuery {
                code: Some(code.to_owned()),
                state: Some(state_token),
                error: None,
            }),
        )
        .await
    }

    #[tokio::test]
    async fn controlled_oidc_callback_validates_pkce_signature_nonce_state_and_replay() {
        let (provider, provider_task) = start_provider().await;
        let database = tempfile::NamedTempFile::new().unwrap();
        let store = IdentityStore::connect_sqlite(database.path(), 2)
            .await
            .unwrap();
        store
            .bootstrap_first_owner(&provider.issuer, SUBJECT, EMAIL, "OIDC test workspace")
            .await
            .unwrap();
        let config = OidcConfig {
            issuer: provider.issuer.clone(),
            client_id: CLIENT_ID.into(),
            redirect_uri: "https://api.example.test/api/auth/callback".into(),
            success_url: "https://codexpt.example.test/".into(),
        };
        let state = OidcState {
            store,
            config: Some(config),
        };

        let (state_token, cookie, authorization_url) = begin_test_login(state.clone()).await;
        authorize_test_login(&provider, &authorization_url).await;
        let successful = call_callback(
            state.clone(),
            state_token.clone(),
            cookie.clone(),
            "valid-code",
        )
        .await;
        assert_eq!(successful.status(), StatusCode::SEE_OTHER);
        let callback_cookies: Vec<_> = successful
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect();
        let session_cookie = callback_cookies
            .iter()
            .find(|value| value.starts_with("__Host-codexpt_session="))
            .unwrap();
        assert!(
            callback_cookies
                .iter()
                .any(|value| value.starts_with("__Host-codexpt_csrf="))
        );
        assert!(
            callback_cookies
                .iter()
                .any(|value| value.starts_with("__Host-codexpt_oidc=;"))
        );
        let session_cookie = session_cookie.split(';').next().unwrap().to_owned();
        let mut me_headers = HeaderMap::new();
        me_headers.insert(
            header::COOKIE,
            HeaderValue::from_str(&session_cookie).unwrap(),
        );
        let me_response = me(AxumState(state.clone()), me_headers).await;
        assert_eq!(me_response.status(), StatusCode::OK);

        let replay = call_callback(state.clone(), state_token, cookie, "valid-code").await;
        assert_eq!(replay.status(), StatusCode::FORBIDDEN);

        let (bad_nonce_state, bad_nonce_cookie, bad_nonce_url) =
            begin_test_login(state.clone()).await;
        authorize_test_login(&provider, &bad_nonce_url).await;
        let bad_nonce = call_callback(
            state.clone(),
            bad_nonce_state,
            bad_nonce_cookie,
            "wrong-nonce",
        )
        .await;
        assert_eq!(bad_nonce.status(), StatusCode::UNAUTHORIZED);

        let (bad_pkce_state, bad_pkce_cookie, bad_pkce_url) = begin_test_login(state.clone()).await;
        authorize_test_login(&provider, &bad_pkce_url).await;
        provider
            .auth
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .code_challenge = "wrong-challenge".into();
        let bad_pkce = call_callback(state, bad_pkce_state, bad_pkce_cookie, "valid-code").await;
        assert_eq!(bad_pkce.status(), StatusCode::UNAUTHORIZED);

        provider_task.abort();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
