//! `X-Amz-Target` によるオペレーションの振り分け。

mod common;

use common::Harness;
use serde_json::json;

const NOT_IMPLEMENTED: &[&str] = &[
    "UpdateStateMachine",
    "DeleteStateMachine",
    "StartSyncExecution",
    "StopExecution",
    "ListExecutions",
    "GetExecutionHistory",
];

#[tokio::test]
async fn まだ無いオペレーションは未実装のエラーを返す() {
    let harness = Harness::start().await;

    for operation in NOT_IMPLEMENTED {
        let reply = harness
            .post(Some(&format!("AWSStepFunctions.{operation}")), "{}")
            .await;

        assert_eq!(reply.status, 400, "{operation}");
        assert_eq!(
            reply.content_type.as_deref(),
            Some("application/x-amz-json-1.0"),
            "{operation}"
        );
        assert_eq!(
            reply.body,
            json!({
                "__type": "NotImplemented",
                "message": format!("sfn-local: {operation} is not implemented yet"),
            }),
        );
    }
}

#[tokio::test]
async fn 知らないオペレーションは_unknown_operation_exception() {
    let harness = Harness::start().await;

    for target in [
        None,
        Some("CreateStateMachine"),
        Some("AmazonAthena.StartQueryExecution"),
        Some("AWSStepFunctions.createStateMachine"),
        Some("AWSStepFunctions.NoSuchOperation"),
    ] {
        let reply = harness.post(target, "{}").await;

        assert_eq!(reply.status, 400, "{target:?}");
        assert_eq!(
            reply.body,
            json!({ "__type": "UnknownOperationException" }),
            "{target:?}"
        );
    }
}

#[tokio::test]
async fn 本文が読めなければ_serialization_exception() {
    let harness = Harness::start().await;

    for body in ["{", "[]", r#"{"stateMachineArn": 1}"#] {
        let reply = harness
            .post(Some("AWSStepFunctions.StartExecution"), body)
            .await;
        assert_eq!(reply.status, 400, "{body}");
        assert_eq!(
            reply.body,
            json!({ "__type": "SerializationException" }),
            "{body}"
        );
    }
}
