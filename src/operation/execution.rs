//! StartExecution・DescribeExecution。

use axum::response::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::arn::{
    EXECUTION_SEGMENT, NAME_MAX_CHARS, STATE_MACHINE_SEGMENT, execution_arn,
    has_forbidden_name_char, is_arn_like,
};
use crate::engine;
use crate::error::{ApiError, constraint};
use crate::handler::App;
use crate::request::parse;
use crate::response::ok;
use crate::store::{Execution, Started, Status};
use crate::time::{self, now_millis};

/// 入力の最大サイズ（UTF-8 のバイト数。2026-09-07 実測）。
const INPUT_MAX_BYTES: usize = 262_144;
/// 入力を省略したときに渡す値（`{}` を空の入力の正典とする API の説明に倣った。本物の保存値は未実測）。
const DEFAULT_INPUT: &str = "{}";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartExecutionRequest {
    state_machine_arn: String,
    name: Option<String>,
    input: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartExecutionResponse {
    execution_arn: String,
    #[serde(serialize_with = "time::epoch_seconds")]
    start_date: i64,
}

/// 検証の順序は実測どおり（2026-09-07）: ARN の形 → モデル制約 → ステートマシンの存在 → 名前の文字と入力の JSON
/// → 同じ名前の実行。ARN の形とモデル制約の順序だけは未実測。
pub(crate) fn start_execution(app: &App, body: &[u8]) -> Response {
    let request: StartExecutionRequest = match parse(body) {
        Ok(request) => request,
        Err(e) => return e.into_response(),
    };
    let state_machine_arn = request.state_machine_arn;

    if !is_arn_like(&state_machine_arn, STATE_MACHINE_SEGMENT) {
        return ApiError::invalid_arn(&state_machine_arn).into_response();
    }

    let mut violations = Vec::new();
    if let Some(name) = &request.name {
        let chars = name.chars().count();
        // 空の名前を送れるクライアントは見つかっておらず、文言は未実測（モデルの min 1 に倣った）。
        if chars == 0 {
            violations.push(constraint(
                Some(name),
                "name",
                "Member must have length greater than or equal to 1",
            ));
        }
        if chars > NAME_MAX_CHARS {
            violations.push(constraint(
                Some(name),
                "name",
                "Member must have length less than or equal to 80",
            ));
        }
    }
    if let Some(input) = &request.input
        && input.len() > INPUT_MAX_BYTES
    {
        violations.push(constraint(
            None,
            "input",
            "Member must have size less than or equal to 262144 bytes in UTF-8 encoding",
        ));
    }
    if !violations.is_empty() {
        return ApiError::validation(&violations).into_response();
    }

    let Some(state_machine) = app.store.state_machine(&state_machine_arn) else {
        return ApiError::state_machine_does_not_exist(&state_machine_arn).into_response();
    };

    if let Some(name) = &request.name
        && has_forbidden_name_char(name)
    {
        return ApiError::invalid_name(name).into_response();
    }
    // 空文字は `{}` にしない（`--input ''` は JSON として壊れているのが正しい）。
    let input = request.input.unwrap_or_else(|| DEFAULT_INPUT.into());
    if let Err(e) = serde_json::from_str::<Value>(&input) {
        return ApiError::invalid_execution_input(&e.to_string()).into_response();
    }

    let name = request.name.unwrap_or_else(|| Uuid::new_v4().to_string());
    let arn = execution_arn(&state_machine_arn, &name);
    let execution = Execution {
        arn: arn.clone(),
        state_machine_arn,
        name,
        input,
        start_ms: now_millis(),
        status: Status::Running,
    };

    match app.store.start_execution(execution) {
        Started::New(execution) => {
            let response = StartExecutionResponse {
                execution_arn: execution.arn.clone(),
                start_date: execution.start_ms,
            };
            engine::spawn(
                app.store.clone(),
                app.lambda.clone(),
                state_machine,
                execution,
            );
            ok(&response)
        }
        // 1 回目と同じ ARN と startDate を返す（2026-09-07 実測）。
        Started::Existing(execution) => ok(&StartExecutionResponse {
            execution_arn: execution.arn,
            start_date: execution.start_ms,
        }),
        Started::Conflict => ApiError::execution_already_exists(&arn).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DescribeExecutionRequest {
    execution_arn: String,
}

/// 状態によってキーが消える（`null` にはならない。2026-09-07 実測）。本物はほかに inputDetails・redriveCount
/// などを返す（sfn-local は返さない）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DescribeExecutionResponse {
    execution_arn: String,
    state_machine_arn: String,
    name: String,
    status: &'static str,
    #[serde(serialize_with = "time::epoch_seconds")]
    start_date: i64,
    /// RUNNING では無い。
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "time::epoch_seconds_opt"
    )]
    stop_date: Option<i64>,
    input: String,
    /// SUCCEEDED だけ。
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    /// FAILED だけ。TIMED_OUT は error も cause も返さない（2026-09-08 実測）。
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cause: Option<String>,
}

pub(crate) fn describe_execution(app: &App, body: &[u8]) -> Response {
    let request: DescribeExecutionRequest = match parse(body) {
        Ok(request) => request,
        Err(e) => return e.into_response(),
    };
    let arn = request.execution_arn;
    if !is_arn_like(&arn, EXECUTION_SEGMENT) {
        return ApiError::invalid_arn(&arn).into_response();
    }
    let Some(execution) = app.store.execution(&arn) else {
        return ApiError::execution_does_not_exist(&arn).into_response();
    };

    let mut response = DescribeExecutionResponse {
        execution_arn: execution.arn,
        state_machine_arn: execution.state_machine_arn,
        name: execution.name,
        status: "RUNNING",
        start_date: execution.start_ms,
        stop_date: None,
        input: execution.input,
        output: None,
        error: None,
        cause: None,
    };
    match execution.status {
        Status::Running => {}
        Status::Succeeded { stop_ms, output } => {
            response.status = "SUCCEEDED";
            response.stop_date = Some(stop_ms);
            response.output = Some(output);
        }
        Status::Failed {
            stop_ms,
            error,
            cause,
        } => {
            response.status = "FAILED";
            response.stop_date = Some(stop_ms);
            response.error = error;
            response.cause = cause;
        }
        Status::TimedOut { stop_ms } => {
            response.status = "TIMED_OUT";
            response.stop_date = Some(stop_ms);
        }
    }
    ok(&response)
}
