//! `/api` routes.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use infraplot_model::{Diagram, Issue};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::store::{Store, valid_id};

pub fn router(store: Store) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/schema", get(schema))
        .route("/validate", post(validate))
        .route("/diagrams", get(list))
        .route(
            "/diagrams/{id}",
            get(fetch).put(save).delete(remove),
        )
        .with_state(store)
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
    let text = std::str::from_utf8(body)
        .map_err(|_| ApiError::BadRequest("body must be UTF-8".into()))?;
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
            let body = diagram.to_toml().map_err(|e| {
                ApiError::Internal(std::io::Error::other(e))
            })?;
            ([(header::CONTENT_TYPE, "application/toml; charset=utf-8")], body).into_response()
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
