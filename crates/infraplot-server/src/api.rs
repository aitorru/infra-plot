//! `/api` routes.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use infraplot_model::{Diagram, Issue};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::store::{Store, valid_id};

/// Header a client must send when the server has a token configured.
const TOKEN_HEADER: &str = "x-infraplot-token";

pub fn router(store: Store, token: Option<String>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/schema", get(schema))
        .route("/validate", post(validate))
        .route("/diagrams", get(list))
        .route("/diagrams/{id}", get(fetch).put(save).delete(remove))
        .with_state(store)
        .layer(middleware::from_fn_with_state(Auth { token }, guard))
}

/// State of the auth middleware: `None` means the API runs unauthenticated,
/// which `main.rs` only allows on a loopback bind.
#[derive(Clone)]
struct Auth {
    token: Option<String>,
}

/// Rejects requests that do not come from the server's own origin, and (when a
/// token is configured) those without a matching `X-Infraplot-Token`.
///
/// The origin check is what stops a random website from writing to the API:
/// a cross-origin `PUT` with `Content-Type: text/plain` is a simple request
/// (no preflight), but it always carries `Origin`, and this rejects it before
/// it reaches a handler. `/health` stays open for probes.
async fn guard(State(auth): State<Auth>, req: Request, next: Next) -> Response {
    if req.uri().path() != "/health" {
        if !origin_allowed(req.headers()) {
            tracing::warn!("rejected request from a different origin");
            return (
                StatusCode::FORBIDDEN,
                Json(json!({ "error": "cross-origin request rejected" })),
            )
                .into_response();
        }
        if let Some(expected) = auth.token.as_deref() {
            let sent = req
                .headers()
                .get(TOKEN_HEADER)
                .and_then(|value| value.to_str().ok());
            let ok = sent.is_some_and(|sent| {
                constant_time_eq(sent.as_bytes(), expected.as_bytes())
            });
            if !ok {
                tracing::warn!("rejected request without a valid token");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "missing or invalid X-Infraplot-Token" })),
                )
                    .into_response();
            }
        }
    }
    next.run(req).await
}

/// True when the request may proceed as far as the origin check is concerned:
/// no `Origin` (curl, the desktop app, tests), an `Origin` whose authority is
/// the `Host` the request came to, or a request between loopback addresses
/// (the Vite dev server proxies `/api` from `localhost:31173` to `127.0.0.1:31080`).
fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };
    let Some(authority) = origin_authority(origin) else {
        return false;
    };
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    authority.eq_ignore_ascii_case(host)
        || (is_loopback(authority.as_str()) && is_loopback(host))
}

/// `http://host:port` → `host:port`. `null` and other opaque origins → `None`.
fn origin_authority(origin: &str) -> Option<String> {
    let lower = origin.trim().to_ascii_lowercase();
    let rest = lower
        .strip_prefix("http://")
        .or_else(|| lower.strip_prefix("https://"))?;
    let authority = rest.trim_end_matches('/');
    if authority.is_empty() {
        None
    } else {
        Some(authority.to_string())
    }
}

/// The host part of an authority, IPv6 brackets included (`[::1]:80` → `::1`).
fn is_loopback(authority: &str) -> bool {
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    host.eq_ignore_ascii_case("localhost")
        || host == "::1"
        || host.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
}

/// Compares a secret without leaking its length through timing.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Invalid(Vec<Issue>),
    NotFound,
    Internal(std::io::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
            }
            Self::Invalid(issues) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "diagram is not valid", "issues": issues })),
            )
                .into_response(),
            Self::NotFound => {
                (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response()
            }
            Self::Internal(err) => {
                tracing::error!(%err, "storage error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "internal error" })),
                )
                    .into_response()
            }
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(err)
    }
}

type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Format {
    #[default]
    Json,
    Toml,
}

#[derive(Debug, Deserialize)]
struct FormatQuery {
    #[serde(default)]
    format: Format,
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn schema() -> impl IntoResponse {
    Json(infraplot_model::json_schema())
}

/// Parses a request body as TOML or JSON depending on its `Content-Type`.
fn parse_body(headers: &HeaderMap, body: &Bytes) -> ApiResult<Diagram> {
    let text =
        std::str::from_utf8(body).map_err(|_| ApiError::BadRequest("body must be UTF-8".into()))?;
    let is_toml = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.contains("toml"));
    let parsed = if is_toml {
        Diagram::from_toml(text)
    } else {
        Diagram::from_json(text)
    };
    parsed.map_err(|e| ApiError::BadRequest(e.to_string()))
}

fn render(diagram: &Diagram, format: Format) -> ApiResult<Response> {
    Ok(match format {
        Format::Json => Json(diagram).into_response(),
        Format::Toml => {
            let body = diagram
                .to_toml()
                .map_err(|e| ApiError::Internal(std::io::Error::other(e)))?;
            (
                [(header::CONTENT_TYPE, "application/toml; charset=utf-8")],
                body,
            )
                .into_response()
        }
    })
}

#[derive(Serialize)]
struct ValidateResponse {
    valid: bool,
    issues: Vec<Issue>,
}

async fn validate(headers: HeaderMap, body: Bytes) -> ApiResult<Json<ValidateResponse>> {
    let issues = parse_body(&headers, &body)?.validate();
    Ok(Json(ValidateResponse {
        valid: issues.is_empty(),
        issues,
    }))
}

async fn list(State(store): State<Store>) -> ApiResult<impl IntoResponse> {
    Ok(Json(store.list().await?))
}

fn checked_id(id: &str) -> ApiResult<()> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "ids must be 1-64 chars of [a-z0-9_-] starting with a letter or digit".into(),
        ))
    }
}

async fn fetch(
    State(store): State<Store>,
    Path(id): Path<String>,
    Query(q): Query<FormatQuery>,
) -> ApiResult<Response> {
    checked_id(&id)?;
    let (diagram, _) = store.get(&id).await?.ok_or(ApiError::NotFound)?;
    render(&diagram, q.format)
}

async fn save(
    State(store): State<Store>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> ApiResult<Response> {
    checked_id(&id)?;
    let diagram = parse_body(&headers, &body)?;
    let issues = diagram.validate();
    if !issues.is_empty() {
        return Err(ApiError::Invalid(issues));
    }
    store.put(&id, &diagram).await?;
    tracing::info!(%id, "diagram saved");
    Ok(Json(json!({ "id": id, "title": diagram.title })).into_response())
}

async fn remove(State(store): State<Store>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    checked_id(&id)?;
    if store.delete(&id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    #[test]
    fn no_origin_is_allowed() {
        assert!(origin_allowed(&headers(&[("host", "127.0.0.1:31080")])));
    }

    #[test]
    fn same_authority_is_allowed() {
        assert!(origin_allowed(&headers(&[
            ("host", "127.0.0.1:31080"),
            ("origin", "http://127.0.0.1:31080"),
        ])));
        assert!(origin_allowed(&headers(&[
            ("host", "infra.example.com"),
            ("origin", "https://infra.example.com"),
        ])));
    }

    #[test]
    fn another_site_is_rejected() {
        assert!(!origin_allowed(&headers(&[
            ("host", "127.0.0.1:31080"),
            ("origin", "https://evil.example"),
        ])));
        assert!(!origin_allowed(&headers(&[
            ("host", "infra.example.com"),
            ("origin", "http://localhost:31173"),
        ])));
    }

    #[test]
    fn opaque_origin_is_rejected() {
        assert!(!origin_allowed(&headers(&[
            ("host", "127.0.0.1:31080"),
            ("origin", "null"),
        ])));
    }

    #[test]
    fn vite_dev_proxy_is_allowed() {
        assert!(origin_allowed(&headers(&[
            ("host", "127.0.0.1:31080"),
            ("origin", "http://localhost:31173"),
        ])));
        assert!(origin_allowed(&headers(&[
            ("host", "[::1]:31080"),
            ("origin", "http://[::1]:31173"),
        ])));
    }

    #[test]
    fn missing_host_with_origin_is_rejected() {
        assert!(!origin_allowed(&headers(&[("origin", "http://localhost:31173")])));
    }

    #[test]
    fn loopback_authority_parsing() {
        assert!(is_loopback("localhost:31173"));
        assert!(is_loopback("127.0.0.1:31080"));
        assert!(is_loopback("127.1.2.3"));
        assert!(is_loopback("[::1]:31080"));
        assert!(!is_loopback("infra.example.com"));
        assert!(!is_loopback("192.168.1.10:31080"));
    }

    #[test]
    fn tokens_match_only_exactly() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secrez"));
        assert!(!constant_time_eq(b"secret", b"secret-longer"));
        assert!(!constant_time_eq(b"", b"x"));
    }
}
