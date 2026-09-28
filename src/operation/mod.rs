//! オペレーションの本体。今はがわだけで、どれも `NotImplemented` を返す。
//! 実装するときは、オペレーションごとにファイルを分けてここから再エクスポートする。

use axum::response::Response;

use crate::handler::App;
use crate::response::error;

/// 未実装のオペレーションが返すエラー。本物の Step Functions には無い、sfn-local だけの `__type`。
/// 5xx にすると SDK が再試行するので 400 で返す。
pub(crate) const NOT_IMPLEMENTED: &str = "NotImplemented";

fn not_implemented(operation: &str) -> Response {
    error(
        NOT_IMPLEMENTED,
        format!("sfn-local: {operation} is not implemented yet"),
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
    create_state_machine => "CreateStateMachine",
    describe_state_machine => "DescribeStateMachine",
    update_state_machine => "UpdateStateMachine",
    delete_state_machine => "DeleteStateMachine",
    list_state_machines => "ListStateMachines",
    start_execution => "StartExecution",
    start_sync_execution => "StartSyncExecution",
    describe_execution => "DescribeExecution",
    stop_execution => "StopExecution",
    list_executions => "ListExecutions",
    get_execution_history => "GetExecutionHistory",
}
