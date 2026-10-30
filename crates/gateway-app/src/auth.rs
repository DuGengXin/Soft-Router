//! Password authentication and short-lived, in-memory browser sessions.

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use axum::Json;
use axum::body::Body;
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use gateway_core::{load_secrets, save_secrets};
use gateway_model::Secrets;
use getrandom::getrandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SESSION_TTL: Duration = Duration::from_secs(8 * 60 * 60);
const SESSION_COOKIE: &str = "gk_session";
const LOGIN_WINDOW: Duration = Duration::from_secs(60);
const LOGIN_FAILURE_LIMIT: usize = 8;

#[derive(Clone)]
pub(crate) struct AuthState {
    secrets_path: PathBuf,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    failures: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
    secure_cookie: bool,
}

#[derive(Clone)]
struct Session {
    username: String,
    credential_revision: String,
    must_change_password: bool,
    expires_at: Instant,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LoginBody {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CredentialsBody {
    username: String,
    current_password: String,
    new_password: String,
}

#[derive(Debug, Serialize)]
struct AuthStatus {
    authenticated: bool,
    username: Option<String>,
    password_change_required: bool,
}

#[derive(Debug, Serialize)]
struct AuthSuccess {
    authenticated: bool,
    username: String,
    password_change_required: bool,
}

impl AuthState {
    pub(crate) fn new(secrets_path: PathBuf) -> Self {
        Self {
            secrets_path,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            failures: Arc::new(Mutex::new(HashMap::new())),
            secure_cookie: std::env::var("GATEWAY_KIT_COOKIE_SECURE")
                .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE")),
        }
    }
}

pub(crate) fn bootstrap_admin(
    secrets_path: &Path,
    reset: bool,
) -> Result<Option<(String, String)>, Box<dyn std::error::Error>> {
    let mut secrets = load_secrets(secrets_path)?;
    if !reset
        && secrets
            .admin_username
            .as_deref()
            .is_some_and(|s| !s.is_empty())
        && secrets
            .admin_password_hash
            .as_deref()
            .is_some_and(|s| !s.is_empty())
    {
        save_secrets(secrets_path, &secrets)?;
        return Ok(None);
    }
    let username = secrets
        .admin_username
        .clone()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "admin".into());
    let password = random_hex(24)?;
    secrets.admin_username = Some(username.clone());
    secrets.admin_password_hash = Some(hash_password(&password)?);
    secrets.admin_password_change_required = true;
    save_secrets(secrets_path, &secrets)?;
    Ok(Some((username, password)))
}

pub(crate) async fn middleware(
    state: Arc<AuthState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if is_public_path(path) {
        return next.run(request).await;
    }
    let Some(session) = session_for(&state, request.headers()) else {
        return api_error(StatusCode::UNAUTHORIZED, "登录已失效，请重新登录");
    };
    if session.must_change_password && path != "/api/v1/auth/credentials" {
        return api_error(StatusCode::FORBIDDEN, "首次登录后请先修改管理员密码");
    }
    next.run(request).await
}

pub(crate) fn is_public_path(path: &str) -> bool {
    path == "/"
        || path.starts_with("/assets/")
        || path == "/api/v1/auth/login"
        || path == "/api/v1/auth/session"
        || path == "/api/v1/auth/logout"
}

pub(crate) async fn session(state: Arc<AuthState>, headers: HeaderMap) -> Response {
    let mut response = match session_for(&state, &headers) {
        Some(session) => Json(AuthStatus {
            authenticated: true,
            username: Some(session.username),
            password_change_required: session.must_change_password,
        })
        .into_response(),
        None => Json(AuthStatus {
            authenticated: false,
            username: None,
            password_change_required: false,
        })
        .into_response(),
    };
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

pub(crate) async fn login(state: Arc<AuthState>, address: SocketAddr, body: LoginBody) -> Response {
    if !allow_login_attempt(&state, address.ip()) {
        return api_error(StatusCode::TOO_MANY_REQUESTS, "登录尝试过多，请稍后再试");
    }
    if body.username.len() > 64 || body.password.len() > 1024 {
        record_login_failure(&state, address.ip());
        return api_error(StatusCode::UNAUTHORIZED, "用户名或密码错误");
    }
    let secrets = match load_secrets(&state.secrets_path) {
        Ok(secrets) => secrets,
        Err(_) => return api_error(StatusCode::SERVICE_UNAVAILABLE, "管理员账户不可用"),
    };
    let expected_user = secrets.admin_username.clone().unwrap_or_default();
    let password_hash = secrets.admin_password_hash.clone().unwrap_or_default();
    let username = body.username;
    let password = body.password;
    let verify = tokio::task::spawn_blocking(move || {
        verify_password(&password_hash, &password) && username == expected_user
    })
    .await
    .unwrap_or(false);
    if !verify {
        record_login_failure(&state, address.ip());
        return api_error(StatusCode::UNAUTHORIZED, "用户名或密码错误");
    }
    clear_login_failures(&state, address.ip());
    let username = secrets
        .admin_username
        .as_deref()
        .unwrap_or("admin")
        .to_string();
    issue_session_response(&state, &secrets, username)
}

pub(crate) async fn logout(state: Arc<AuthState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_token(&headers) {
        lock(&state.sessions).remove(&token_digest(&token));
    }
    let mut response = Json(serde_json::json!({ "authenticated": false })).into_response();
    let value = expired_cookie(state.secure_cookie);
    response.headers_mut().insert(SET_COOKIE, value);
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

pub(crate) async fn change_credentials(
    state: Arc<AuthState>,
    headers: HeaderMap,
    body: CredentialsBody,
) -> Response {
    if session_for(&state, &headers).is_none() {
        return api_error(StatusCode::UNAUTHORIZED, "登录已失效，请重新登录");
    }
    if !valid_username(&body.username) {
        return api_error(
            StatusCode::BAD_REQUEST,
            "用户名需为 3–32 位字母、数字、点、横线或下划线",
        );
    }
    if !(12..=128).contains(&body.new_password.len()) {
        return api_error(StatusCode::BAD_REQUEST, "新密码长度需为 12–128 个字节");
    }
    let mut secrets = match load_secrets(&state.secrets_path) {
        Ok(secrets) => secrets,
        Err(_) => return api_error(StatusCode::SERVICE_UNAVAILABLE, "管理员账户不可用"),
    };
    let old_hash = secrets.admin_password_hash.clone().unwrap_or_default();
    let current_password = body.current_password;
    let verified =
        tokio::task::spawn_blocking(move || verify_password(&old_hash, &current_password))
            .await
            .unwrap_or(false);
    if !verified {
        return api_error(StatusCode::UNAUTHORIZED, "当前密码错误");
    }
    let new_password = body.new_password;
    let new_hash = match tokio::task::spawn_blocking(move || hash_password(&new_password)).await {
        Ok(Ok(hash)) => hash,
        _ => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "密码保存失败"),
    };
    let username = body.username.trim().to_string();
    secrets.admin_username = Some(username.clone());
    secrets.admin_password_hash = Some(new_hash);
    secrets.admin_password_change_required = false;
    if save_secrets(&state.secrets_path, &secrets).is_err() {
        return api_error(StatusCode::INTERNAL_SERVER_ERROR, "密码保存失败");
    }
    lock(&state.sessions).clear();
    issue_session_response(&state, &secrets, username)
}

fn issue_session_response(state: &AuthState, secrets: &Secrets, username: String) -> Response {
    let token = match random_hex(32) {
        Ok(token) => token,
        Err(_) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法创建登录会话"),
    };
    let revision = credential_revision(secrets);
    let session = Session {
        username: username.clone(),
        credential_revision: revision,
        must_change_password: secrets.admin_password_change_required,
        expires_at: Instant::now() + SESSION_TTL,
    };
    lock(&state.sessions).insert(token_digest(&token), session);
    let mut response = Json(AuthSuccess {
        authenticated: true,
        username,
        password_change_required: secrets.admin_password_change_required,
    })
    .into_response();
    let cookie = session_cookie(&token, state.secure_cookie);
    response.headers_mut().insert(SET_COOKIE, cookie);
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}

fn session_for(state: &AuthState, headers: &HeaderMap) -> Option<Session> {
    let token = cookie_token(headers)?;
    let key = token_digest(&token);
    let session = lock(&state.sessions).get(&key).cloned()?;
    if session.expires_at <= Instant::now() {
        lock(&state.sessions).remove(&key);
        return None;
    }
    let secrets = load_secrets(&state.secrets_path).ok()?;
    if session.credential_revision != credential_revision(&secrets) {
        lock(&state.sessions).remove(&key);
        return None;
    }
    Some(session)
}

fn credential_revision(secrets: &Secrets) -> String {
    let mut digest = Sha256::new();
    digest.update(secrets.admin_username.as_deref().unwrap_or_default());
    digest.update(secrets.admin_password_hash.as_deref().unwrap_or_default());
    bytes_hex(&digest.finalize())
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|pair| pair.strip_prefix(&format!("{SESSION_COOKIE}=")))
        .filter(|value| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_owned)
}

fn session_cookie(token: &str, secure: bool) -> HeaderValue {
    let secure_attr = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}{}",
        SESSION_TTL.as_secs(),
        secure_attr
    ))
    .expect("generated cookie contains only safe characters")
}

fn expired_cookie(secure: bool) -> HeaderValue {
    let secure_attr = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0{}",
        secure_attr
    ))
    .expect("static cookie is valid")
}

fn random_hex(bytes: usize) -> Result<String, Box<dyn std::error::Error>> {
    let mut value = vec![0u8; bytes];
    getrandom(&mut value).map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(value.iter().map(|b| format!("{b:02x}")).collect())
}

fn token_digest(token: &str) -> String {
    bytes_hex(&Sha256::digest(token.as_bytes()))
}

fn bytes_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash_password(password: &str) -> Result<String, String> {
    let mut salt_bytes = [0u8; 16];
    getrandom(&mut salt_bytes).map_err(|error| error.to_string())?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|error| error.to_string())?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| error.to_string())
}

fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash).ok().is_some_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

fn valid_username(username: &str) -> bool {
    (3..=32).contains(&username.len())
        && username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn allow_login_attempt(state: &AuthState, ip: IpAddr) -> bool {
    let now = Instant::now();
    let mut failures = lock(&state.failures);
    let attempts = failures.entry(ip).or_default();
    while attempts
        .front()
        .is_some_and(|time| now.duration_since(*time) > LOGIN_WINDOW)
    {
        attempts.pop_front();
    }
    attempts.len() < LOGIN_FAILURE_LIMIT
}

fn record_login_failure(state: &AuthState, ip: IpAddr) {
    let mut failures = lock(&state.failures);
    if failures.len() >= 2048 && !failures.contains_key(&ip) {
        failures.retain(|_, times| !times.is_empty());
        if failures.len() >= 2048 {
            failures.clear();
        }
    }
    failures.entry(ip).or_default().push_back(Instant::now());
}

fn clear_login_failures(state: &AuthState, ip: IpAddr) {
    lock(&state.failures).remove(&ip);
}

fn api_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "message": message }))).into_response()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
