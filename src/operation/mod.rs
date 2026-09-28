//! オペレーションの本体。まだ無いものは `NotImplemented` を返す。

mod execution;
mod state_machine;

use axum::response::Response;

pub(crate) use execution::{describe_execution, start_execution};
pub(crate) use state_machine::{create_state_machine, describe_state_machine, list_state_machines};

use crate::handler::App;
use crate::response::error;

/// 未実装のオペレーションが返すエラー。本物の Step Functions には無い、sfn-local だけの `__type`。
/// 5xx にすると SDK が再試行するので 400 で返す。
pub(crate) const NOT_IMPLEMENTED: &str = "NotImplemented";

fn not_implemented(operation: &str) -> Response {
    error(
        NOT_IMPLEMENTED,
        Some(format!("sfn-local: {operation} is not implemented yet")),
    )
}

macro_rules! stub_operations {
    ($($name:ident => $operation:literal),* $(,)?) => {
        $(
            pub(crate) fn $name(_app: &App, _body: &[u8]) -> Response {
                not_implemented($operation)
            }
        )*
    };
}

stub_operations! {
    update_state_machine => "UpdateStateMachine",
    delete_state_machine => "DeleteStateMachine",
    start_sync_execution => "StartSyncExecution",
    stop_execution => "StopExecution",
    list_executions => "ListExecutions",
    get_execution_history => "GetExecutionHistory",
}
