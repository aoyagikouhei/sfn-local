//! 結合テストの足場。本物の `sfn_local::router` と偽の Lambda（Invoke API）を同じプロセスに立て、素の HTTP で叩く。

#![allow(dead_code)] // テストのファイルごとに使うヘルパが違う。

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::{Value, json};
use sfn_local::config::Config;
use tokio::net::TcpListener;

pub const REGION: &str = "us-east-1";
pub const ACCOUNT_ID: &str = "123456789012";

pub fn state_machine_arn(name: &str) -> String {
    format!("arn:aws:states:{REGION}:{ACCOUNT_ID}:stateMachine:{name}")
}

pub struct Harness {
    addr: SocketAddr,
    client: reqwest::Client,
}

/// `call` の応答。本文は JSON として読んでおく。
pub struct Reply {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Value,
}

#[derive(Default)]
pub struct HarnessBuilder {
    lambda_endpoint: Option<String>,
    state_machines_dir: Option<PathBuf>,
}

impl HarnessBuilder {
    pub fn lambda(mut self, lambda: &FakeLambda) -> Self {
        self.lambda_endpoint = Some(lambda.endpoint());
        self
    }

    pub fn lambda_endpoint(mut self, endpoint: &str) -> Self {
        self.lambda_endpoint = Some(endpoint.to_string());
        self
    }

    pub fn state_machines_dir(mut self, dir: PathBuf) -> Self {
        self.state_machines_dir = Some(dir);
        self
    }

    pub async fn start(self) -> Harness {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("待ち受けできる");
        let addr = listener.local_addr().expect("アドレスを取れる");
        let config = Config {
            bind_address: addr.to_string(),
            region: REGION.into(),
            account_id: ACCOUNT_ID.into(),
            state_machines_dir: self.state_machines_dir,
            lambda_endpoint: self.lambda_endpoint,
        };
        let (router, _) = sfn_local::router(config).expect("組み立てられる");
        tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("サーバが動き続ける");
        });

        Harness {
            addr,
            client: reqwest::Client::new(),
        }
    }
}

impl Harness {
    pub fn builder() -> HarnessBuilder {
        HarnessBuilder::default()
    }

    pub async fn start() -> Self {
        Self::builder().start().await
    }

    /// `X-Amz-Target` を付けて `POST /` する。`None` ならヘッダを付けない。
    pub async fn post(&self, target: Option<&str>, body: &str) -> Reply {
        let mut request = self
            .client
            .post(format!("http://{}/", self.addr))
            .header("content-type", "application/x-amz-json-1.0")
            .body(body.to_string());
        if let Some(target) = target {
            request = request.header("x-amz-target", target);
        }

        let response = request.send().await.expect("応答が返る");
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = response.json().await.expect("本文が JSON");

        Reply {
            status,
            content_type,
            body,
        }
    }

    /// オペレーションを呼ぶ。
    pub async fn call(&self, operation: &str, body: Value) -> Reply {
        self.post(
            Some(&format!("AWSStepFunctions.{operation}")),
            &body.to_string(),
        )
        .await
    }

    /// 成功を前提に呼び、本文を返す。
    pub async fn ok(&self, operation: &str, body: Value) -> Value {
        let reply = self.call(operation, body).await;
        assert_eq!(reply.status, 200, "{operation}: {}", reply.body);
        reply.body
    }

    /// ステートマシンを作って ARN を返す。
    pub async fn create(&self, name: &str, definition: Value) -> String {
        let body = self
            .ok(
                "CreateStateMachine",
                json!({
                    "name": name,
                    "definition": definition.to_string(),
                    "roleArn": format!("arn:aws:iam::{ACCOUNT_ID}:role/example"),
                }),
            )
            .await;
        body["stateMachineArn"].as_str().unwrap().to_string()
    }

    /// 実行を始めて実行の ARN を返す。
    pub async fn start_execution(&self, state_machine_arn: &str, input: Value) -> String {
        let body = self
            .ok(
                "StartExecution",
                json!({ "stateMachineArn": state_machine_arn, "input": input.to_string() }),
            )
            .await;
        body["executionArn"].as_str().unwrap().to_string()
    }

    pub async fn describe(&self, execution_arn: &str) -> Value {
        self.ok(
            "DescribeExecution",
            json!({ "executionArn": execution_arn }),
        )
        .await
    }

    /// 実行が終わるまで待って DescribeExecution の本文を返す。
    pub async fn wait(&self, execution_arn: &str) -> Value {
        for _ in 0..500 {
            let body = self.describe(execution_arn).await;
            if body["status"] != "RUNNING" {
                return body;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("10 秒待っても終わらない: {execution_arn}");
    }

    /// 定義を作って、入力を渡して、終わるまで待つ。
    pub async fn run(&self, name: &str, definition: Value, input: Value) -> Value {
        let arn = self.create(name, definition).await;
        let execution_arn = self.start_execution(&arn, input).await;
        self.wait(&execution_arn).await
    }
}

/// 偽の Lambda の応答。
#[derive(Clone)]
pub struct LambdaReply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
    pub delay: Duration,
}

impl LambdaReply {
    pub fn ok(body: Value) -> Self {
        Self::raw(200, body.to_string())
    }

    pub fn raw(status: u16, body: String) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
            delay: Duration::ZERO,
        }
    }

    pub fn header(mut self, name: &'static str, value: &str) -> Self {
        self.headers.push((name, value.to_string()));
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

/// 偽の Lambda が受け取った呼び出し。
#[derive(Debug, Clone)]
pub struct LambdaCall {
    pub function: String,
    pub query: Option<String>,
    pub invocation_type: Option<String>,
    pub body: String,
}

type Responder = dyn Fn(&LambdaCall, usize) -> LambdaReply + Send + Sync;

/// 偽の Lambda の Invoke API（`POST /2015-03-31/functions/{name}/invocations`）。
#[derive(Clone)]
pub struct FakeLambda {
    addr: SocketAddr,
    calls: Arc<Mutex<Vec<LambdaCall>>>,
}

#[derive(Clone)]
struct FakeLambdaState {
    calls: Arc<Mutex<Vec<LambdaCall>>>,
    responder: Arc<Responder>,
}

impl FakeLambda {
    /// `responder` は呼び出しと、それが何回目か（0 始まり）を受け取る。
    pub async fn start(
        responder: impl Fn(&LambdaCall, usize) -> LambdaReply + Send + Sync + 'static,
    ) -> Self {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let state = FakeLambdaState {
            calls: calls.clone(),
            responder: Arc::new(responder),
        };
        let router = Router::new()
            .route("/2015-03-31/functions/{name}/invocations", post(invoke))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("待ち受けできる");
        let addr = listener.local_addr().expect("アドレスを取れる");
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("動き続ける");
        });
        Self { addr, calls }
    }

    /// 受け取った本文をそのまま返す Lambda。
    pub async fn echo() -> Self {
        Self::start(|call, _| LambdaReply::raw(200, call.body.clone())).await
    }

    pub fn endpoint(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn calls(&self) -> Vec<LambdaCall> {
        self.calls.lock().unwrap().clone()
    }
}

async fn invoke(
    State(state): State<FakeLambdaState>,
    Path(name): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let call = LambdaCall {
        function: name,
        query,
        invocation_type: headers
            .get("x-amz-invocation-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    let index = {
        let mut calls = state.calls.lock().unwrap();
        calls.push(call.clone());
        calls.len() - 1
    };
    let reply = (state.responder)(&call, index);
    tokio::time::sleep(reply.delay).await;

    let mut response = (StatusCode::from_u16(reply.status).unwrap(), reply.body).into_response();
    for (name, value) in reply.headers {
        response.headers_mut().insert(name, value.parse().unwrap());
    }
    response
}

/// テストごとの一時ディレクトリ（`CARGO_TARGET_TMPDIR` の下）。
pub fn temp_dir(name: &str) -> PathBuf {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("作れる");
    dir
}
