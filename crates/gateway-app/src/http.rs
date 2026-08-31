//! Embedded Web UI and JSON API. Observe-mode safe: apply still requires confirm.

use super::service::AppService;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use gateway_core::{
    Paths, SystemCommandRunner, check_applied_with_artifacts, default_or_load_config,
    init_database, load_secrets, probe_uplinks, recent_events, sample_host, save_secrets,
    validate_proxy_uri,
};
use gateway_model::{
    AppConfig, OperatingMode, SecretsPatch, is_loopback_bind, parse_wireguard_blob, wan_direct_cidr,
};
use rust_embed::Embed;
use serde::Deserialize;

#[derive(Clone)]
struct AppState {
    paths: Paths,
    service: AppService,
    require_auth: bool,
}

#[derive(Debug, Deserialize)]
struct ConfirmBody {
    #[serde(default)]
    confirm: bool,
}

#[derive(Debug, Deserialize)]
struct WgPaste {
    blob: String,
}

pub async fn serve(
    paths: Paths,
    listen_override: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = default_or_load_config(&paths.config)?;
    let bind = listen_override.unwrap_or_else(|| format!("{}:{}", config.ui.bind, config.ui.port));
    let bind_host = bind
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or(bind.as_str());
    let state = AppState {
        service: AppService::new(paths.clone()),
        require_auth: !is_loopback_bind(bind_host),
        paths,
    };
    let app = Router::new()
        .route("/", get(index))
        .route("/assets/{*path}", get(dist_asset))
        .route("/api/v1/health", get(health))
        .route("/api/v1/status", get(status))
        .route("/api/v1/monitor", get(monitor))
        .route("/api/v1/events", get(events))
        .route("/api/v1/discover", get(discover))
        .route("/api/v1/config", get(get_config).put(put_config))
        .route("/api/v1/secrets", get(get_secrets).put(put_secrets))
        .route("/api/v1/wireguard/parse", post(parse_wg))
        .route("/api/v1/plan", get(plan))
        .route("/api/v1/apply", post(apply))
        .route("/api/v1/rollback", post(do_rollback))
        .route("/api/v1/disable", post(do_disable))
        .layer(middleware::from_fn_with_state(state.clone(), auth))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("gateway-kit ui http://{bind} (observe-safe until apply --confirm)");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn auth(State(state): State<AppState>, req: Request<Body>, next: Next) -> Response {
    if !state.require_auth {
        return next.run(req).await;
    }
    let token = load_secrets(&state.paths.secrets)
        .ok()
        .and_then(|s| s.ui_lan_token)
        .unwrap_or_default();
    if token.is_empty() {
        return (StatusCode::FORBIDDEN, "LAN token missing").into_response();
    }
    if header_token(req.headers()).as_deref() == Some(token.as_str()) {
        next.run(req).await
    } else {
        (StatusCode::UNAUTHORIZED, "token required").into_response()
    }
}

fn header_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get("x-gateway-token") {
        return value.to_str().ok().map(str::to_string);
    }
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
}

#[derive(Embed)]
#[folder = "ui/dist"]
struct Dist;

async fn index() -> Response {
    dist_response("index.html")
}

async fn dist_asset(Path(path): Path<String>) -> Response {
    dist_response(&format!("assets/{path}"))
}

fn dist_response(rel: &str) -> Response {
    match dist_bytes(rel) {
        Some(bytes) => {
            let mut res = Response::new(Body::from(bytes));
            if let Ok(value) = HeaderValue::from_str(mime_of(rel)) {
                res.headers_mut()
                    .insert(axum::http::header::CONTENT_TYPE, value);
            }
            #[cfg(debug_assertions)]
            {
                res.headers_mut().insert(
                    axum::http::header::CACHE_CONTROL,
                    HeaderValue::from_static("no-store"),
                );
            }
            res
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn dist_bytes(rel: &str) -> Option<Vec<u8>> {
    let rel = rel.trim_start_matches('/');
    #[cfg(debug_assertions)]
    {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("ui")
            .join("dist")
            .join(rel);
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    Dist::get(rel).map(|file| file.data.into_owned())
}

fn mime_of(rel: &str) -> &'static str {
    if rel.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if rel.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if rel.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if rel.ends_with(".svg") {
        "image/svg+xml"
    } else if rel.ends_with(".woff2") {
        "font/woff2"
    } else if rel.ends_with(".map") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

async fn monitor() -> impl IntoResponse {
    Json(sample_host())
}

async fn events(State(state): State<AppState>) -> impl IntoResponse {
    match init_database(&state.paths.state_db).and_then(|conn| recent_events(&conn, 30)) {
        Ok(items) => Json(items).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let config = default_or_load_config(&state.paths.config).unwrap_or_default();
    Json(serde_json::json!({
        "status": "ok",
        "mode": config.mode,
        "mutating": false,
        "dataplane": dataplane_json(&state.paths, &config),
    }))
}

async fn status(State(state): State<AppState>) -> impl IntoResponse {
    match state.service.load_all() {
        Ok((config, _, report)) => {
            let (wan_uplink, tunnel_uplink, link_notes) =
                probe_uplinks(&config, &SystemCommandRunner);
            Json(serde_json::json!({
                "mode": config.mode,
                "wan": config.wan,
                "lan": config.lan,
                "wireguard": { "enabled": config.wireguard.enabled, "interface": config.wireguard.interface },
                "routing": { "china_direct": config.routing.china_direct },
                "wan_direct_cidr": wan_direct_cidr(&config),
                "wan_uplink": wan_uplink,
                "tunnel_uplink": tunnel_uplink,
                "link_notes": link_notes,
                "blockers": report.has_blockers(),
                "conflicts": report.conflicts,
                "interfaces": report.interfaces,
                "dataplane": dataplane_json(&state.paths, &config),
            }))
            .into_response()
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

async fn discover(State(state): State<AppState>) -> impl IntoResponse {
    match state.service.load_all() {
        Ok((_, _, report)) => Json(report).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

async fn parse_wg(Json(body): Json<WgPaste>) -> impl IntoResponse {
    match parse_wireguard_blob(&body.blob) {
        Ok(parsed) => Json(parsed).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    match default_or_load_config(&state.paths.config) {
        Ok(config) => Json(config).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn put_config(
    State(state): State<AppState>,
    Json(config): Json<AppConfig>,
) -> impl IntoResponse {
    let secrets = load_secrets(&state.paths.secrets).unwrap_or_default();
    match config.validate(&secrets) {
        Ok(_) => match state.service.save_config(&config) {
            Ok(()) => Json(serde_json::json!({"saved": true})).into_response(),
            Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
        },
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

async fn get_secrets(State(state): State<AppState>) -> impl IntoResponse {
    match load_secrets(&state.paths.secrets) {
        Ok(secrets) => Json(secrets.status()).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

async fn put_secrets(
    State(state): State<AppState>,
    Json(patch): Json<SecretsPatch>,
) -> impl IntoResponse {
    if let Some(proxy_uri) = patch
        .proxy_uri
        .as_deref()
        .map(str::trim)
        .filter(|uri| !uri.is_empty())
        && let Err(error) = validate_proxy_uri(proxy_uri)
    {
        return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
    }
    let mut secrets = load_secrets(&state.paths.secrets).unwrap_or_default();
    secrets.apply_patch(&patch);
    match save_secrets(&state.paths.secrets, &secrets) {
        Ok(()) => Json(secrets.status()).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

async fn plan(State(state): State<AppState>) -> impl IntoResponse {
    match state.service.plan() {
        Ok(plan) => Json(plan.redacted()).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}

async fn apply(State(state): State<AppState>, Json(body): Json<ConfirmBody>) -> impl IntoResponse {
    mutate(&state.service, body.confirm, Mutate::Apply)
}

async fn do_rollback(
    State(state): State<AppState>,
    Json(body): Json<ConfirmBody>,
) -> impl IntoResponse {
    mutate(&state.service, body.confirm, Mutate::Rollback)
}

async fn do_disable(
    State(state): State<AppState>,
    Json(body): Json<ConfirmBody>,
) -> impl IntoResponse {
    mutate(&state.service, body.confirm, Mutate::Disable)
}

enum Mutate {
    Apply,
    Rollback,
    Disable,
}

fn mutate(service: &AppService, confirm: bool, kind: Mutate) -> Response {
    let result = match kind {
        Mutate::Apply => service.apply(confirm),
        Mutate::Rollback => service.rollback(confirm),
        Mutate::Disable => service.disable(confirm),
    };
    match result {
        Ok(value) => Json(serde_json::json!({
            "generation_id": value.generation_id,
            "message": value.message
        }))
        .into_response(),
        Err(error) => (StatusCode::CONFLICT, error.to_string()).into_response(),
    }
}

fn dataplane_json(paths: &Paths, config: &AppConfig) -> serde_json::Value {
    if config.mode != OperatingMode::Gateway {
        return serde_json::json!({
            "status": "observe",
            "message": "observe mode: dataplane idle",
            "failed_checks": [],
            "notes": []
        });
    }
    let geo_cache = paths.generated_dir.join("sing-box-cache.db");
    let singbox_config = paths.generated_dir.join("sing-box.json");
    serde_json::to_value(check_applied_with_artifacts(
        config,
        &SystemCommandRunner,
        paths.manages_host_units(),
        Some(&singbox_config),
        Some(&geo_cache),
    ))
    .unwrap_or_else(|_| serde_json::json!({ "status": "unhealthy", "message": "serialize failed" }))
}

#[cfg(test)]
mod tests {
    use super::ConfirmBody;

    #[test]
    fn apply_body_without_confirm_is_false() {
        let body: ConfirmBody = serde_json::from_str("{}").expect("json");
        assert!(!body.confirm);
    }
}
