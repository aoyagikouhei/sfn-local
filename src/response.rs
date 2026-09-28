//! awsJson1.0 のレスポンス組み立て。

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::{Map, Value};

const CONTENT_TYPE: &str = "application/x-amz-json-1.0";

pub fn ok<T: Serialize>(body: &T) -> Response {
    match serde_json::to_vec(body) {
        Ok(payload) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, CONTENT_TYPE)],
            payload,
        )
            .into_response(),
        Err(e) => error(
            "InternalServerException",
            Some(format!("could not serialize the response: {e}")),
        ),
    }
}

/// `__type` と、あれば `message` の本文。メッセージのキーは小文字の `message`（2026-09-07 実測）。
pub fn error(error_type: &str, message: Option<String>) -> Response {
    let mut body = Map::new();
    body.insert("__type".into(), Value::String(error_type.to_string()));
    if let Some(message) = message {
        body.insert("message".into(), Value::String(message));
    }
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, CONTENT_TYPE)],
        serde_json::to_vec(&body).unwrap_or_default(),
    )
        .into_response()
}

/// `__type` だけの本文。
pub fn bare_error(code: &str) -> Response {
    error(code, None)
}
