//! StartExecution・DescribeExecution の検証と応答の形。

mod common;

use std::time::Duration;

use common::{FakeLambda, Harness, LambdaReply, state_machine_arn};
use serde_json::{Value, json};

const NS: &str = "com.amazonaws.swf.service.v2.model";
const VALIDATE: &str = "com.amazon.coral.validate#ValidationException";

fn pass() -> Value {
    json!({"StartAt": "P", "States": {"P": {"Type": "Pass", "End": true}}})
}

fn start(arn: &str, name: &str, input: &str) -> Value {
    json!({"stateMachineArn": arn, "name": name, "input": input})
}

#[tokio::test]
async fn 開始した実行が終わると_output_と_stop_date_が付く() {
    let harness = Harness::start().await;
    let arn = harness.create("example", pass()).await;

    let started = harness
        .ok("StartExecution", start(&arn, "run-1", r#"{"a": 1}"#))
        .await;
    assert_eq!(
        started["executionArn"],
        "arn:aws:states:us-east-1:123456789012:execution:example:run-1"
    );
    assert!(started["startDate"].is_f64());

    let described = harness
        .wait(started["executionArn"].as_str().unwrap())
        .await;
    assert_eq!(described["name"], "run-1");
    assert_eq!(described["stateMachineArn"], arn);
    assert_eq!(described["status"], "SUCCEEDED");
    assert_eq!(described["input"], r#"{"a": 1}"#);
    assert_eq!(described["output"], r#"{"a":1}"#);
    assert_eq!(described["startDate"], started["startDate"]);
    assert!(described["stopDate"].as_f64() >= described["startDate"].as_f64());
    assert!(described.get("error").is_none());
}

#[tokio::test]
async fn 名前と入力を省略すると_uuid_の名前と空の_object_で始まる() {
    let harness = Harness::start().await;
    let arn = harness.create("example", pass()).await;

    let started = harness
        .ok("StartExecution", json!({"stateMachineArn": arn}))
        .await;
    let described = harness
        .wait(started["executionArn"].as_str().unwrap())
        .await;
    assert_eq!(described["name"].as_str().unwrap().len(), 36);
    assert_eq!(described["input"], "{}");
}

#[tokio::test]
async fn 実行中の_describe_には_stop_date_が無い() {
    let lambda =
        FakeLambda::start(|_, _| LambdaReply::ok(json!({})).delay(Duration::from_millis(500)))
            .await;
    let harness = Harness::builder().lambda(&lambda).start().await;
    let arn = harness
        .create(
            "example",
            json!({"StartAt": "T", "States": {"T": {"Type": "Task", "Resource": "arn:aws:states:::lambda:invoke", "Parameters": {"FunctionName": "slow"}, "End": true}}}),
        )
        .await;

    let execution_arn = harness.start_execution(&arn, json!({})).await;
    let described = harness.describe(&execution_arn).await;
    assert_eq!(described["status"], "RUNNING");
    for key in ["stopDate", "output", "error", "cause"] {
        assert!(described.get(key).is_none(), "{key}");
    }
}

#[tokio::test]
async fn 実行中に同じ名前と同じ入力で始めると同じ実行を返す() {
    let lambda =
        FakeLambda::start(|_, _| LambdaReply::ok(json!({})).delay(Duration::from_millis(500)))
            .await;
    let harness = Harness::builder().lambda(&lambda).start().await;
    let arn = harness
        .create(
            "example",
            json!({"StartAt": "T", "States": {"T": {"Type": "Task", "Resource": "arn:aws:states:::lambda:invoke", "Parameters": {"FunctionName": "slow"}, "End": true}}}),
        )
        .await;

    let first = harness
        .ok("StartExecution", start(&arn, "same", "{}"))
        .await;
    let again = harness
        .ok("StartExecution", start(&arn, "same", "{}"))
        .await;
    assert_eq!(again, first);

    let reply = harness
        .call("StartExecution", start(&arn, "same", r#"{"x": 1}"#))
        .await;
    let execution_arn = first["executionArn"].as_str().unwrap();
    assert_eq!(
        reply.body,
        json!({"__type": format!("{NS}#ExecutionAlreadyExists"), "message": format!("Execution Already Exists: '{execution_arn}'")})
    );

    harness.wait(execution_arn).await;
    assert_eq!(lambda.calls().len(), 1);
    let reply = harness
        .call("StartExecution", start(&arn, "same", "{}"))
        .await;
    assert_eq!(reply.body["__type"], format!("{NS}#ExecutionAlreadyExists"));
}

#[tokio::test]
async fn start_execution_の検証() {
    let harness = Harness::start().await;
    let arn = harness.create("example", pass()).await;
    let missing = state_machine_arn("missing");
    let long = "a".repeat(81);
    let large = format!("\"{}\"", "a".repeat(262_144));

    let cases = [
        (
            start("not-an-arn", "n", "{}"),
            json!({"__type": format!("{NS}#InvalidArn"), "message": "Invalid Arn: 'Invalid ARN prefix: not-an-arn'"}),
        ),
        (
            start(&arn, &long, "{}"),
            json!({"__type": VALIDATE, "message": format!("1 validation error detected: Value '{long}' at 'name' failed to satisfy constraint: Member must have length less than or equal to 80")}),
        ),
        (
            start(&arn, "n", &large),
            json!({"__type": VALIDATE, "message": "1 validation error detected: Value at 'input' failed to satisfy constraint: Member must have size less than or equal to 262144 bytes in UTF-8 encoding"}),
        ),
        // モデル制約はステートマシンの存在より先（2026-09-07 実測）。
        (
            start(&missing, &long, "{}"),
            json!({"__type": VALIDATE, "message": format!("1 validation error detected: Value '{long}' at 'name' failed to satisfy constraint: Member must have length less than or equal to 80")}),
        ),
        (
            start(&missing, "a b", "{"),
            json!({"__type": format!("{NS}#StateMachineDoesNotExist"), "message": format!("State Machine Does Not Exist: '{missing}'")}),
        ),
        (
            start(&arn, "a b", "{}"),
            json!({"__type": format!("{NS}#InvalidName"), "message": "Invalid Name: 'a b'"}),
        ),
        (
            json!({"name": "n"}),
            json!({"__type": VALIDATE, "message": "1 validation error detected: Value null at 'stateMachineArn' failed to satisfy constraint: Member must not be null"}),
        ),
    ];
    for (request, expected) in cases {
        let reply = harness.call("StartExecution", request.clone()).await;
        assert_eq!(reply.status, 400, "{request}");
        assert_eq!(reply.body, expected, "{request}");
    }

    let reply = harness.call("StartExecution", start(&arn, "n", "{")).await;
    assert_eq!(reply.body["__type"], format!("{NS}#InvalidExecutionInput"));
    assert!(
        reply.body["message"]
            .as_str()
            .unwrap()
            .starts_with("Invalid State Machine Execution Input: '")
    );
}

#[tokio::test]
async fn describe_execution_の検証() {
    let harness = Harness::start().await;
    let arn = "arn:aws:states:us-east-1:123456789012:execution:example:missing";

    let reply = harness
        .call("DescribeExecution", json!({"executionArn": arn}))
        .await;
    assert_eq!(
        reply.body,
        json!({"__type": format!("{NS}#ExecutionDoesNotExist"), "message": format!("Execution Does Not Exist: '{arn}'")})
    );

    let reply = harness
        .call(
            "DescribeExecution",
            json!({"executionArn": state_machine_arn("example")}),
        )
        .await;
    assert_eq!(reply.body["__type"], format!("{NS}#InvalidArn"));
}
