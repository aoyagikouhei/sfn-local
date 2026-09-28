//! awsJson1.0 のレスポンス組み立て。

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;

const CONTENT_TYPE: &str = "application/x-amz-json-1.0";

/// `__type` と `message` の本文。本物の Step Functions のエラー本文の形（キーの大文字小文字、
/// `__type` の名前空間の有無）は未実測。
pub fn error(code: &str, message: impl Into<String>) -> Response {
    error_body(json!({ "__type": code, "message": message.into() }))
}

/// `__type` だけの本文。
pub fn bare_error(code: &str) -> Response {
    error_body(json!({ "__type": code }))
}

fn error_body(body: serde_json::Value) -> Response {
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, CONTENT_TYPE)],
        serde_json::to_vec(&body).unwrap_or_default(),
    )
        .into_response()
}
