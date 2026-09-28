//! CreateStateMachine・DescribeStateMachine・ListStateMachines と、起動時の読み込み。

mod common;

use common::{ACCOUNT_ID, Harness, state_machine_arn, temp_dir};
use serde_json::{Value, json};

const NS: &str = "com.amazonaws.swf.service.v2.model";

fn succeed() -> Value {
    json!({"StartAt": "Done", "States": {"Done": {"Type": "Succeed"}}})
}

fn create_body(name: &str, definition: &str) -> Value {
    json!({
        "name": name,
        "definition": definition,
        "roleArn": format!("arn:aws:iam::{ACCOUNT_ID}:role/example"),
    })
}

#[tokio::test]
async fn 作ったステートマシンを_describe_と_list_で見られる() {
    let harness = Harness::start().await;
    let definition = succeed().to_string();

    let created = harness
        .ok("CreateStateMachine", create_body("example", &definition))
        .await;
    assert_eq!(created["stateMachineArn"], state_machine_arn("example"));
    assert!(created["creationDate"].is_f64());

    let described = harness
        .ok(
            "DescribeStateMachine",
            json!({"stateMachineArn": state_machine_arn("example")}),
        )
        .await;
    assert_eq!(described["name"], "example");
    assert_eq!(described["status"], "ACTIVE");
    assert_eq!(described["definition"], definition);
    assert_eq!(described["type"], "STANDARD");
    assert_eq!(described["creationDate"], created["creationDate"]);

    harness.create("another", succeed()).await;
    let listed = harness.ok("ListStateMachines", json!({})).await;
    let names: Vec<&str> = listed["stateMachines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["another", "example"]);
}

#[tokio::test]
async fn 同じ中身なら冪等に成功し_違えば_already_exists() {
    let harness = Harness::start().await;
    let first = harness
        .ok(
            "CreateStateMachine",
            create_body("example", &succeed().to_string()),
        )
        .await;
    let again = harness
        .ok(
            "CreateStateMachine",
            create_body("example", &succeed().to_string()),
        )
        .await;
    assert_eq!(again, first);

    let other = json!({"StartAt": "P", "States": {"P": {"Type": "Pass", "End": true}}});
    let reply = harness
        .call(
            "CreateStateMachine",
            create_body("example", &other.to_string()),
        )
        .await;
    assert_eq!(reply.status, 400);
    assert_eq!(
        reply.body,
        json!({
            "__type": format!("{NS}#StateMachineAlreadyExists"),
            "message": format!("State Machine Already Exists: '{}'", state_machine_arn("example")),
        })
    );
}

#[tokio::test]
async fn 実行できない定義は_invalid_definition() {
    let harness = Harness::start().await;
    let choice = json!({"StartAt": "C", "States": {"C": {"Type": "Choice", "Choices": [], "Default": "D"}, "D": {"Type": "Succeed"}}});

    let reply = harness
        .call(
            "CreateStateMachine",
            create_body("example", &choice.to_string()),
        )
        .await;
    assert_eq!(reply.status, 400);
    assert_eq!(reply.body["__type"], format!("{NS}#InvalidDefinition"));
    assert_eq!(
        reply.body["message"],
        "Invalid State Machine Definition: 'UNSUPPORTED_BY_SFN_LOCAL: the state type 'Choice' is not supported by sfn-local yet at /States/C'"
    );

    let reply = harness
        .call("CreateStateMachine", create_body("example", "{"))
        .await;
    assert_eq!(reply.body["__type"], format!("{NS}#InvalidDefinition"));
}

#[tokio::test]
async fn 名前と種類の制約() {
    let harness = Harness::start().await;
    let definition = succeed().to_string();

    let reply = harness
        .call("CreateStateMachine", create_body("a:b", &definition))
        .await;
    assert_eq!(
        reply.body,
        json!({"__type": format!("{NS}#InvalidName"), "message": "Invalid Name: 'a:b'"})
    );

    let long = "a".repeat(81);
    let reply = harness
        .call("CreateStateMachine", create_body(&long, &definition))
        .await;
    assert_eq!(
        reply.body["__type"],
        "com.amazon.coral.validate#ValidationException"
    );

    let mut body = create_body("example", &definition);
    body["type"] = json!("BATCH");
    let reply = harness.call("CreateStateMachine", body).await;
    assert_eq!(
        reply.body["message"],
        "1 validation error detected: Value 'BATCH' at 'type' failed to satisfy constraint: Member must satisfy enum value set: [STANDARD, EXPRESS]"
    );
}

#[tokio::test]
async fn 無いステートマシンの_describe_は_does_not_exist() {
    let harness = Harness::start().await;
    let arn = state_machine_arn("missing");
    let reply = harness
        .call("DescribeStateMachine", json!({"stateMachineArn": arn}))
        .await;
    assert_eq!(
        reply.body,
        json!({"__type": format!("{NS}#StateMachineDoesNotExist"), "message": format!("State Machine Does Not Exist: '{arn}'")})
    );

    let reply = harness
        .call(
            "DescribeStateMachine",
            json!({"stateMachineArn": "not-an-arn"}),
        )
        .await;
    assert_eq!(
        reply.body,
        json!({"__type": format!("{NS}#InvalidArn"), "message": "Invalid Arn: 'Invalid ARN prefix: not-an-arn'"})
    );
}

#[tokio::test]
async fn 起動時にディレクトリの定義を読み込む() {
    let dir = temp_dir("load-state-machines");
    std::fs::write(dir.join("first.asl.json"), succeed().to_string()).unwrap();
    std::fs::write(dir.join("second.asl.json"), succeed().to_string()).unwrap();
    std::fs::write(dir.join("ignored.json"), "not asl").unwrap();
    let harness = Harness::builder().state_machines_dir(dir).start().await;

    let listed = harness.ok("ListStateMachines", json!({})).await;
    let arns: Vec<&str> = listed["stateMachines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["stateMachineArn"].as_str().unwrap())
        .collect();
    assert_eq!(
        arns,
        [state_machine_arn("first"), state_machine_arn("second")]
    );

    let execution_arn = harness
        .start_execution(&state_machine_arn("first"), json!({"k": 1}))
        .await;
    let described = harness.wait(&execution_arn).await;
    assert_eq!(described["status"], "SUCCEEDED");
    assert_eq!(described["output"], r#"{"k":1}"#);
}
