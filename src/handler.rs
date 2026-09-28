//! Step Functions API のディスパッチ。awsJson1.0 なので POST / の 1 本で、
//! X-Amz-Target ヘッダでオペレーションを見分ける。SigV4 署名は検証しない。

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;

use crate::config::Config;
use crate::engine::lambda::Lambda;
use crate::operation;
use crate::response::bare_error;
use crate::store::Store;

const TARGET_PREFIX: &str = "AWSStepFunctions.";

#[derive(Clone)]
pub struct App {
    pub config: Arc<Config>,
    pub store: Arc<Store>,
    pub lambda: Arc<Lambda>,
}

/// ヘッダが無い・前置き `AWSStepFunctions.` が無い・名前が違うときは `UnknownOperationException` の 400。
/// 本物の形は未実測（athena-local で測った Athena の形に倣った仮置き）。
pub(crate) async fn dispatch(State(app): State<App>, headers: HeaderMap, body: Bytes) -> Response {
    let Some(operation) = operation_name(&headers) else {
        return unknown_operation();
    };

    match operation {
        "CreateStateMachine" => operation::create_state_machine(&app, &body),
        "DescribeStateMachine" => operation::describe_state_machine(&app, &body),
        "UpdateStateMachine" => operation::update_state_machine(&app, &body),
        "DeleteStateMachine" => operation::delete_state_machine(&app, &body),
        "ListStateMachines" => operation::list_state_machines(&app, &body),
        "StartExecution" => operation::start_execution(&app, &body),
        "StartSyncExecution" => operation::start_sync_execution(&app, &body),
        "DescribeExecution" => operation::describe_execution(&app, &body),
        "StopExecution" => operation::stop_execution(&app, &body),
        "ListExecutions" => operation::list_executions(&app, &body),
        "GetExecutionHistory" => operation::get_execution_history(&app, &body),
        _ => unknown_operation(),
    }
}

fn unknown_operation() -> Response {
    bare_error("UnknownOperationException")
}

fn operation_name(headers: &HeaderMap) -> Option<&str> {
    let target = headers.get("x-amz-target")?.to_str().ok()?;
    target.strip_prefix(TARGET_PREFIX)
}
