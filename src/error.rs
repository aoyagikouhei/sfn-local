//! API（`POST /`）が返すエラー。wire の形は HTTP 400 +
//! `{"__type": "<名前空間>#<名前>", "message": "..."}`（2026-09-07 実測）。
//!
//! 名前空間は 1 種類ではない。Step Functions のサービス例外は `com.amazonaws.swf.service.v2.model#`、
//! モデル制約の違反（`ValidationException`）だけは `com.amazon.coral.validate#` になる（2026-09-07 実測）。
//! SDK は `#` より前を捨てて例外の型を決める。`__type` にコロンを入れないこと（Rust SDK は `:` 以降を落とす）。

use axum::response::Response;

use crate::response;

/// Step Functions のサービス例外の名前空間（2026-09-07 実測）。
const SWF: &str = "com.amazonaws.swf.service.v2.model";
/// モデル制約の違反の名前空間（2026-09-07 実測）。
const CORAL_VALIDATE: &str = "com.amazon.coral.validate";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    namespace: Option<&'static str>,
    code: &'static str,
    message: Option<String>,
}

impl ApiError {
    fn service(code: &'static str, message: String) -> Self {
        Self {
            namespace: Some(SWF),
            code,
            message: Some(message),
        }
    }

    /// 実測したのは `not-an-arn` への応答だけ（2026-09-07）。`arn:` で始まるが形が違う場合の文言は未実測。
    pub fn invalid_arn(arn: &str) -> Self {
        Self::service(
            "InvalidArn",
            format!("Invalid Arn: 'Invalid ARN prefix: {arn}'"),
        )
    }

    /// モデル制約の違反。件数を先頭に置き、複数なら `; ` で並べる（1 件の形は 2026-09-07 実測。
    /// 複数件の形は Athena で実測した同じ枠組みの形に倣った）。
    pub fn validation(violations: &[String]) -> Self {
        let noun = if violations.len() == 1 {
            "error"
        } else {
            "errors"
        };
        Self {
            namespace: Some(CORAL_VALIDATE),
            code: "ValidationException",
            message: Some(format!(
                "{} validation {noun} detected: {}",
                violations.len(),
                violations.join("; ")
            )),
        }
    }

    /// 本文が JSON として読めない・型が違う。Step Functions では未実測で、Athena で実測した同じ枠組みの形
    /// （`__type` だけ）に倣った。
    pub fn serialization() -> Self {
        Self {
            namespace: None,
            code: "SerializationException",
            message: None,
        }
    }

    pub fn state_machine_does_not_exist(arn: &str) -> Self {
        Self::service(
            "StateMachineDoesNotExist",
            format!("State Machine Does Not Exist: '{arn}'"),
        )
    }

    /// 文言は未実測（AWS のドキュメントの形に倣った）。
    pub fn state_machine_already_exists(arn: &str) -> Self {
        Self::service(
            "StateMachineAlreadyExists",
            format!("State Machine Already Exists: '{arn}'"),
        )
    }

    pub fn invalid_name(name: &str) -> Self {
        Self::service("InvalidName", format!("Invalid Name: '{name}'"))
    }

    /// 括弧の中は sfn-local の JSON パーサの文言で、本物（Jackson）とは違う。前置きは 2026-09-07 実測。
    pub fn invalid_execution_input(detail: &str) -> Self {
        Self::service(
            "InvalidExecutionInput",
            format!("Invalid State Machine Execution Input: '{detail}'"),
        )
    }

    /// 前置きは AWS のドキュメントの形。括弧の中は sfn-local の文言で、本物とは違う。
    pub fn invalid_definition(detail: &str) -> Self {
        Self::service(
            "InvalidDefinition",
            format!("Invalid State Machine Definition: '{detail}'"),
        )
    }

    pub fn execution_already_exists(execution_arn: &str) -> Self {
        Self::service(
            "ExecutionAlreadyExists",
            format!("Execution Already Exists: '{execution_arn}'"),
        )
    }

    pub fn execution_does_not_exist(execution_arn: &str) -> Self {
        Self::service(
            "ExecutionDoesNotExist",
            format!("Execution Does Not Exist: '{execution_arn}'"),
        )
    }

    pub fn into_response(self) -> Response {
        let error_type = match self.namespace {
            Some(namespace) => format!("{namespace}#{}", self.code),
            None => self.code.to_string(),
        };
        response::error(&error_type, self.message)
    }
}

/// `Value 'x' at 'name' failed to satisfy constraint: ...` の 1 件分。
pub fn constraint(value: Option<&str>, field: &str, constraint: &str) -> String {
    match value {
        Some(value) => {
            format!("Value '{value}' at '{field}' failed to satisfy constraint: {constraint}")
        }
        None => format!("Value at '{field}' failed to satisfy constraint: {constraint}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 検証エラーは件数で単数と複数を使い分ける() {
        let one = ApiError::validation(&["a".into()]);
        assert_eq!(
            one.message.as_deref(),
            Some("1 validation error detected: a")
        );
        let two = ApiError::validation(&["a".into(), "b".into()]);
        assert_eq!(
            two.message.as_deref(),
            Some("2 validation errors detected: a; b")
        );
    }
}
