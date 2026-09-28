//! CreateStateMachine・DescribeStateMachine・ListStateMachines。

use std::sync::Arc;

use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::arn::{
    NAME_MAX_CHARS, STATE_MACHINE_SEGMENT, has_forbidden_name_char, is_arn_like, state_machine_arn,
};
use crate::asl;
use crate::error::{ApiError, constraint};
use crate::handler::App;
use crate::request::parse;
use crate::response::ok;
use crate::store::{Created, StateMachine};
use crate::time::{self, now_millis};

/// 定義の最大長（API の制約。文字数）。
const DEFINITION_MAX_CHARS: usize = 1_048_576;
const TYPES: [&str; 2] = ["STANDARD", "EXPRESS"];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateStateMachineRequest {
    name: String,
    definition: String,
    role_arn: String,
    r#type: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateStateMachineResponse {
    state_machine_arn: String,
    #[serde(serialize_with = "time::epoch_seconds")]
    creation_date: i64,
}

/// 同じ名前・定義・ロールなら冪等に成功し、中身が違えば StateMachineAlreadyExists（AWS のドキュメントの挙動。未実測）。
/// 検証の順序は未実測。
pub(crate) fn create_state_machine(app: &App, body: &[u8]) -> Response {
    let request: CreateStateMachineRequest = match parse(body) {
        Ok(request) => request,
        Err(e) => return e.into_response(),
    };

    let mut violations = Vec::new();
    let name_chars = request.name.chars().count();
    if name_chars == 0 {
        violations.push(constraint(
            Some(&request.name),
            "name",
            "Member must have length greater than or equal to 1",
        ));
    }
    if name_chars > NAME_MAX_CHARS {
        violations.push(constraint(
            Some(&request.name),
            "name",
            "Member must have length less than or equal to 80",
        ));
    }
    if request.definition.chars().count() > DEFINITION_MAX_CHARS {
        violations.push(constraint(
            None,
            "definition",
            "Member must have length less than or equal to 1048576",
        ));
    }
    if let Some(kind) = &request.r#type
        && !TYPES.contains(&kind.as_str())
    {
        violations.push(constraint(
            Some(kind),
            "type",
            "Member must satisfy enum value set: [STANDARD, EXPRESS]",
        ));
    }
    if !violations.is_empty() {
        return ApiError::validation(&violations).into_response();
    }

    if has_forbidden_name_char(&request.name) {
        return ApiError::invalid_name(&request.name).into_response();
    }
    if !request.role_arn.starts_with("arn:") {
        return ApiError::invalid_arn(&request.role_arn).into_response();
    }
    let definition = match asl::parse(&request.definition) {
        Ok(definition) => definition,
        Err(e) => return ApiError::invalid_definition(&e).into_response(),
    };

    let arn = state_machine_arn(&app.config.region, &app.config.account_id, &request.name);
    let state_machine = StateMachine {
        arn: arn.clone(),
        name: request.name,
        definition_text: request.definition,
        definition: Arc::new(definition),
        role_arn: request.role_arn,
        kind: request.r#type.unwrap_or_else(|| "STANDARD".into()),
        creation_ms: now_millis(),
    };
    match app.store.create_state_machine(state_machine) {
        Created::New(created) | Created::Existing(created) => ok(&CreateStateMachineResponse {
            state_machine_arn: created.arn,
            creation_date: created.creation_ms,
        }),
        Created::Conflict => ApiError::state_machine_already_exists(&arn).into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DescribeStateMachineRequest {
    state_machine_arn: String,
}

/// 本物はほかに loggingConfiguration・tracingConfiguration などを返す（sfn-local は返さない）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DescribeStateMachineResponse {
    state_machine_arn: String,
    name: String,
    status: &'static str,
    definition: String,
    role_arn: String,
    r#type: String,
    #[serde(serialize_with = "time::epoch_seconds")]
    creation_date: i64,
}

pub(crate) fn describe_state_machine(app: &App, body: &[u8]) -> Response {
    let request: DescribeStateMachineRequest = match parse(body) {
        Ok(request) => request,
        Err(e) => return e.into_response(),
    };
    let arn = request.state_machine_arn;
    if !is_arn_like(&arn, STATE_MACHINE_SEGMENT) {
        return ApiError::invalid_arn(&arn).into_response();
    }
    match app.store.state_machine(&arn) {
        Some(state_machine) => ok(&DescribeStateMachineResponse {
            state_machine_arn: state_machine.arn,
            name: state_machine.name,
            status: "ACTIVE",
            definition: state_machine.definition_text,
            role_arn: state_machine.role_arn,
            r#type: state_machine.kind,
            creation_date: state_machine.creation_ms,
        }),
        None => ApiError::state_machine_does_not_exist(&arn).into_response(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListStateMachinesResponse {
    state_machines: Vec<StateMachineListItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StateMachineListItem {
    state_machine_arn: String,
    name: String,
    r#type: String,
    #[serde(serialize_with = "time::epoch_seconds")]
    creation_date: i64,
}

/// ページングはしない（`maxResults`・`nextToken` は読まずに全件を返す）。順序は名前順で、本物の順序は未実測。
pub(crate) fn list_state_machines(app: &App, _body: &[u8]) -> Response {
    let state_machines = app
        .store
        .state_machines()
        .into_iter()
        .map(|state_machine| StateMachineListItem {
            state_machine_arn: state_machine.arn,
            name: state_machine.name,
            r#type: state_machine.kind,
            creation_date: state_machine.creation_ms,
        })
        .collect();
    ok(&ListStateMachinesResponse { state_machines })
}
