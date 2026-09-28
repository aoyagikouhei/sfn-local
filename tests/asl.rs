//! ASL の実行（Lambda を呼ぶ Task、Pass・Succeed・Fail、パス、Retry・Catch、タイムアウト）。

mod common;

use std::time::Duration;

use common::{FakeLambda, Harness, LambdaReply};
use serde_json::{Value, json};

fn invoke_state(function: &str) -> Value {
    json!({
        "Type": "Task",
        "Resource": "arn:aws:states:::lambda:invoke",
        "Parameters": {"FunctionName": function, "Payload.$": "$"},
        "End": true
    })
}

fn single(state: Value) -> Value {
    json!({"StartAt": "S", "States": {"S": state}})
}

fn output(described: &Value) -> Value {
    serde_json::from_str(described["output"].as_str().expect("output がある")).unwrap()
}

#[tokio::test]
async fn lambda_invoke_は入力を_payload_で渡し_結果を包んで返す() {
    let lambda = FakeLambda::start(|_, _| LambdaReply::ok(json!({"rows": 3}))).await;
    let harness = Harness::builder().lambda(&lambda).start().await;

    let described = harness
        .run(
            "example",
            single(invoke_state(
                "arn:aws:lambda:us-east-1:123456789012:function:worker",
            )),
            json!({"sql": "SELECT 1"}),
        )
        .await;

    assert_eq!(described["status"], "SUCCEEDED");
    assert_eq!(
        output(&described),
        json!({"ExecutedVersion": "$LATEST", "Payload": {"rows": 3}, "StatusCode": 200})
    );
    let calls = lambda.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function, "worker");
    assert_eq!(calls[0].query, None);
    assert_eq!(
        serde_json::from_str::<Value>(&calls[0].body).unwrap(),
        json!({"sql": "SELECT 1"})
    );
}

#[tokio::test]
async fn output_path_で_payload_だけを出力にできる() {
    let lambda = FakeLambda::start(|_, _| LambdaReply::ok(json!({"rows": 3}))).await;
    let harness = Harness::builder().lambda(&lambda).start().await;
    let mut state = invoke_state("worker");
    state["OutputPath"] = json!("$.Payload");

    let described = harness.run("example", single(state), json!({})).await;
    assert_eq!(output(&described), json!({"rows": 3}));
}

#[tokio::test]
async fn 関数の_arn_を直に書くと戻り値がそのまま結果になる() {
    let lambda = FakeLambda::echo().await;
    let harness = Harness::builder().lambda(&lambda).start().await;
    let definition = single(json!({
        "Type": "Task",
        "Resource": "arn:aws:lambda:us-east-1:123456789012:function:worker:live",
        "InputPath": "$.args",
        "ResultPath": "$.result",
        "End": true
    }));

    let described = harness
        .run(
            "example",
            definition,
            json!({"args": {"n": 1}, "keep": true}),
        )
        .await;
    assert_eq!(
        output(&described),
        json!({"args": {"n": 1}, "keep": true, "result": {"n": 1}})
    );
    assert_eq!(lambda.calls()[0].query.as_deref(), Some("Qualifier=live"));
}

#[tokio::test]
async fn 関数のエラーは_error_type_と本文で_failed_になる() {
    let body = r#"{"errorType":"QueryError","errorMessage":"bad sql"}"#;
    let lambda = FakeLambda::start(move |_, _| {
        LambdaReply::raw(200, body.into()).header("x-amz-function-error", "Unhandled")
    })
    .await;
    let harness = Harness::builder().lambda(&lambda).start().await;

    let described = harness
        .run("example", single(invoke_state("worker")), json!({}))
        .await;
    assert_eq!(described["status"], "FAILED");
    assert_eq!(described["error"], "QueryError");
    assert_eq!(described["cause"], body);
    assert!(described.get("output").is_none());
}

#[tokio::test]
async fn 本文に_error_がある_200_は成功のまま() {
    let lambda = FakeLambda::start(|_, _| LambdaReply::ok(json!({"error": "x"}))).await;
    let harness = Harness::builder().lambda(&lambda).start().await;

    let described = harness
        .run("example", single(invoke_state("worker")), json!({}))
        .await;
    assert_eq!(described["status"], "SUCCEEDED");
}

#[tokio::test]
async fn cargo_lambda_の_500_と_error_type_は関数のエラーとして読む() {
    let body = r#"{"errorType":"QueryError","errorMessage":"bad sql"}"#;
    let lambda = FakeLambda::start(move |_, _| LambdaReply::raw(500, body.into())).await;
    let harness = Harness::builder().lambda(&lambda).start().await;

    let described = harness
        .run("example", single(invoke_state("worker")), json!({}))
        .await;
    assert_eq!(described["error"], "QueryError");
    assert_eq!(described["cause"], body);
}

#[tokio::test]
async fn lambda_のサービスエラーは_lambda_の前置きの名前になる() {
    let lambda = FakeLambda::start(|call, _| match call.function.as_str() {
        "missing" => LambdaReply::raw(
            404,
            r#"{"Type":"User","Message":"Function not found"}"#.into(),
        )
        .header(
            "x-amzn-errortype",
            "ResourceNotFoundException:http://internal.amazon.com/",
        ),
        _ => LambdaReply::raw(502, String::new()),
    })
    .await;
    let harness = Harness::builder().lambda(&lambda).start().await;

    let described = harness
        .run("missing", single(invoke_state("missing")), json!({}))
        .await;
    assert_eq!(described["error"], "Lambda.ResourceNotFoundException");

    let described = harness
        .run("broken", single(invoke_state("broken")), json!({}))
        .await;
    assert_eq!(described["error"], "Lambda.ServiceException");
}

#[tokio::test]
async fn lambda_につながらなければ_sdk_client_exception() {
    // 何も待ち受けていないポート。
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let harness = Harness::builder().lambda_endpoint(&endpoint).start().await;

    let described = harness
        .run("example", single(invoke_state("worker")), json!({}))
        .await;
    assert_eq!(described["status"], "FAILED");
    assert_eq!(described["error"], "Lambda.SdkClientException");
    assert!(described["cause"].as_str().unwrap().contains(&endpoint));
}

#[tokio::test]
async fn retry_で再試行し_catch_で遷移する() {
    let lambda = FakeLambda::start(|_, index| match index {
        0 => LambdaReply::raw(502, String::new()),
        _ => LambdaReply::ok(json!({"attempt": index})),
    })
    .await;
    let harness = Harness::builder().lambda(&lambda).start().await;
    let mut state = invoke_state("worker");
    state["Retry"] = json!([{"ErrorEquals": ["Lambda.ServiceException"], "IntervalSeconds": 1, "MaxAttempts": 1}]);
    state["OutputPath"] = json!("$.Payload");

    let described = harness.run("retry", single(state), json!({})).await;
    assert_eq!(output(&described), json!({"attempt": 1}));
    assert_eq!(lambda.calls().len(), 2);

    let failing = FakeLambda::start(|_, _| {
        LambdaReply::raw(
            200,
            r#"{"errorType":"QueryError","errorMessage":"x"}"#.into(),
        )
        .header("x-amz-function-error", "Handled")
    })
    .await;
    let harness = Harness::builder().lambda(&failing).start().await;
    let definition = json!({
        "StartAt": "Invoke",
        "States": {
            "Invoke": {
                "Type": "Task",
                "Resource": "arn:aws:states:::lambda:invoke",
                "Parameters": {"FunctionName": "worker"},
                "Retry": [{"ErrorEquals": ["Lambda.ServiceException"], "MaxAttempts": 5}],
                "OutputPath": "$.Payload",
                "Catch": [{"ErrorEquals": ["States.ALL"], "ResultPath": "$.error", "Next": "Recovered"}],
                "End": true
            },
            "Recovered": {"Type": "Pass", "End": true}
        }
    });
    let described = harness.run("catch", definition, json!({"k": 1})).await;
    assert_eq!(described["status"], "SUCCEEDED");
    assert_eq!(
        output(&described),
        json!({"k": 1, "error": {"Error": "QueryError", "Cause": r#"{"errorType":"QueryError","errorMessage":"x"}"#}})
    );
    // 一致しない Retrier では再試行しない。
    assert_eq!(failing.calls().len(), 1);
}

#[tokio::test]
async fn トップレベルの_timeout_seconds_を超えると_timed_out() {
    let lambda =
        FakeLambda::start(|_, _| LambdaReply::ok(json!({})).delay(Duration::from_secs(5))).await;
    let harness = Harness::builder().lambda(&lambda).start().await;
    let mut definition = single(invoke_state("slow"));
    definition["TimeoutSeconds"] = json!(1);

    let described = harness.run("example", definition, json!({})).await;
    assert_eq!(described["status"], "TIMED_OUT");
    for key in ["output", "error", "cause"] {
        assert!(described.get(key).is_none(), "{key}");
    }
    let elapsed =
        described["stopDate"].as_f64().unwrap() - described["startDate"].as_f64().unwrap();
    assert!((elapsed - 1.0).abs() < 0.001, "{elapsed}");
}

#[tokio::test]
async fn task_の_timeout_seconds_は_states_timeout_で拾える() {
    let lambda =
        FakeLambda::start(|_, _| LambdaReply::ok(json!({})).delay(Duration::from_secs(5))).await;
    let harness = Harness::builder().lambda(&lambda).start().await;
    let mut state = invoke_state("slow");
    state["TimeoutSeconds"] = json!(1);

    let described = harness.run("example", single(state), json!({})).await;
    assert_eq!(described["status"], "FAILED");
    assert_eq!(described["error"], "States.Timeout");
}

#[tokio::test]
async fn pass_と_fail_とコンテキストオブジェクト() {
    let harness = Harness::start().await;
    let definition = json!({
        "StartAt": "Shape",
        "States": {
            "Shape": {
                "Type": "Pass",
                "Parameters": {"name.$": "$$.Execution.Name", "value.$": "$.v", "fixed": [1, 2]},
                "ResultPath": "$.shaped",
                "Next": "Fixed"
            },
            "Fixed": {"Type": "Pass", "Result": {"x": 1}, "ResultPath": "$.fixed", "OutputPath": "$", "Next": "Done"},
            "Done": {"Type": "Succeed", "OutputPath": "$.shaped"}
        }
    });
    let arn = harness.create("example", definition).await;
    let started = harness
        .ok(
            "StartExecution",
            json!({"stateMachineArn": arn, "name": "run-1", "input": r#"{"v": "hello"}"#}),
        )
        .await;
    let described = harness
        .wait(started["executionArn"].as_str().unwrap())
        .await;
    assert_eq!(
        output(&described),
        json!({"name": "run-1", "value": "hello", "fixed": [1, 2]})
    );

    let failing = json!({"StartAt": "F", "States": {"F": {"Type": "Fail", "Error": "Custom.Error", "Cause": "because"}}});
    let described = harness.run("failing", failing, json!({})).await;
    assert_eq!(described["status"], "FAILED");
    assert_eq!(described["error"], "Custom.Error");
    assert_eq!(described["cause"], "because");
}

#[tokio::test]
async fn パスが解決できなければ_states_runtime_で失敗し_catch_でも拾わない() {
    let harness = Harness::start().await;
    let definition = json!({
        "StartAt": "Read",
        "States": {"Read": {"Type": "Pass", "InputPath": "$.missing", "End": true}}
    });

    let described = harness.run("example", definition, json!({})).await;
    assert_eq!(described["status"], "FAILED");
    assert_eq!(described["error"], "States.Runtime");
}
