//! 実行エンジン。ステートを順にたどり、Task で Lambda を呼ぶ。実行ごとに 1 つの tokio タスクで動く。

pub mod lambda;

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::asl::definition::{Catcher, Definition, Resource, Retrier, State, StateKind, Task};
use crate::asl::path::{Path, render_template};
use crate::store::{Execution, StateMachine, Status, Store};
use crate::time::{iso8601, now_millis};

use self::lambda::Lambda;

/// ステートの失敗（`Error` と `Cause`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub error: String,
    pub cause: Option<String>,
}

impl Failure {
    pub fn new(error: &str, cause: &str) -> Self {
        Self::with_cause(error, cause.to_string())
    }

    pub fn with_cause(error: &str, cause: String) -> Self {
        Self {
            error: error.to_string(),
            cause: Some(cause),
        }
    }

    /// Retry・Catch では拾えず、実行ごと失敗させるエラー（ASL の仕様）。
    fn runtime(cause: String) -> Self {
        Self::with_cause(RUNTIME, cause)
    }

    /// Catch が ResultPath に渡すエラーの object。
    fn to_output(&self) -> Value {
        let mut output = json!({ "Error": self.error });
        if let Some(cause) = &self.cause {
            output["Cause"] = Value::String(cause.clone());
        }
        output
    }
}

const RUNTIME: &str = "States.Runtime";
const TIMEOUT: &str = "States.Timeout";

/// 実行を始める。StartExecution の応答は待たずに返す。
pub fn spawn(
    store: Arc<Store>,
    lambda: Arc<Lambda>,
    state_machine: StateMachine,
    execution: Execution,
) {
    tokio::spawn(async move {
        let status = run(&lambda, &state_machine, &execution).await;
        store.finish(&execution.arn, status);
    });
}

async fn run(lambda: &Lambda, state_machine: &StateMachine, execution: &Execution) -> Status {
    let definition = &state_machine.definition;
    let input: Value = serde_json::from_str(&execution.input).unwrap_or(Value::Null);
    let context = Context {
        state_machine,
        execution,
    };
    let walk = walk(lambda, definition, &context, input);

    let result = match definition.timeout_seconds {
        Some(seconds) => match tokio::time::timeout(Duration::from_secs(seconds), walk).await {
            Ok(result) => result,
            // 本物の stopDate は startDate + TimeoutSeconds ちょうど。error / cause は返らない（2026-09-08 実測）。
            Err(_) => {
                return Status::TimedOut {
                    stop_ms: execution.start_ms + seconds as i64 * 1000,
                };
            }
        },
        None => walk.await,
    };

    let stop_ms = now_millis();
    match result {
        Ok(output) => Status::Succeeded {
            stop_ms,
            output: output.to_string(),
        },
        Err(Terminal::Failed { error, cause }) => Status::Failed {
            stop_ms,
            error,
            cause,
        },
    }
}

/// 実行が失敗で終わる理由。Fail ステートは Error / Cause を省略できる。
enum Terminal {
    Failed {
        error: Option<String>,
        cause: Option<String>,
    },
}

impl From<Failure> for Terminal {
    fn from(failure: Failure) -> Self {
        Terminal::Failed {
            error: Some(failure.error),
            cause: failure.cause,
        }
    }
}

struct Context<'a> {
    state_machine: &'a StateMachine,
    execution: &'a Execution,
}

impl Context<'_> {
    /// コンテキストオブジェクト（`$$`）。
    fn object(&self, state_name: &str, entered_ms: i64, retry_count: u64) -> Value {
        json!({
            "Execution": {
                "Id": self.execution.arn,
                "Input": serde_json::from_str::<Value>(&self.execution.input).unwrap_or(Value::Null),
                "Name": self.execution.name,
                "RoleArn": self.state_machine.role_arn,
                "StartTime": iso8601(self.execution.start_ms),
            },
            "StateMachine": {
                "Id": self.state_machine.arn,
                "Name": self.state_machine.name,
            },
            "State": {
                "Name": state_name,
                "EnteredTime": iso8601(entered_ms),
                "RetryCount": retry_count,
            },
        })
    }
}

async fn walk(
    lambda: &Lambda,
    definition: &Definition,
    context: &Context<'_>,
    mut input: Value,
) -> Result<Value, Terminal> {
    let mut name = definition.start_at.clone();
    loop {
        let state = &definition.states[&name];
        let entered_ms = now_millis();
        let (output, next) = match &state.kind {
            StateKind::Fail { error, cause } => {
                return Err(Terminal::Failed {
                    error: error.clone(),
                    cause: cause.clone(),
                });
            }
            StateKind::Succeed => (output_of(state, input_of(state, &input)?)?, None),
            StateKind::Pass {
                result,
                parameters,
                result_path,
            } => {
                let object = context.object(&name, entered_ms, 0);
                let effective = input_of(state, &input)?;
                let result = match (result, parameters) {
                    (Some(result), _) => result.clone(),
                    (None, Some(parameters)) => render_template(parameters, &effective, &object)
                        .map_err(Failure::runtime)?,
                    (None, None) => effective,
                };
                let merged = apply_result_path(result_path, input.clone(), result)?;
                (output_of(state, merged)?, state.next.clone())
            }
            StateKind::Task(task) => {
                match run_task(lambda, task, state, &name, context, entered_ms, &input).await? {
                    TaskOutcome::Done(merged) => (output_of(state, merged)?, state.next.clone()),
                    // Catch の遷移先には OutputPath を掛けない（ASL の仕様の読み。本物では未実測）。
                    TaskOutcome::Caught(merged, next) => (merged, Some(next)),
                }
            }
        };
        match next {
            Some(next) => {
                name = next;
                input = output;
            }
            None => return Ok(output),
        }
    }
}

enum TaskOutcome {
    Done(Value),
    Caught(Value, String),
}

async fn run_task(
    lambda: &Lambda,
    task: &Task,
    state: &State,
    name: &str,
    context: &Context<'_>,
    entered_ms: i64,
    input: &Value,
) -> Result<TaskOutcome, Terminal> {
    let effective = input_of(state, input)?;
    let mut attempts = vec![0u64; task.retry.len()];
    let mut retry_count = 0;

    let failure = loop {
        let object = context.object(name, entered_ms, retry_count);
        let parameters = match &task.parameters {
            Some(template) => {
                render_template(template, &effective, &object).map_err(Failure::runtime)?
            }
            None => effective.clone(),
        };
        let attempt = tokio::time::timeout(
            Duration::from_secs(task.timeout_seconds),
            invoke(lambda, &task.resource, &parameters),
        )
        .await
        .unwrap_or_else(|_| Err(Failure::new(TIMEOUT, "Task timed out.")));

        let failure = match attempt {
            Ok(result) => {
                let selected = match &task.result_selector {
                    Some(template) => {
                        render_template(template, &result, &object).map_err(Failure::runtime)?
                    }
                    None => result,
                };
                return Ok(TaskOutcome::Done(apply_result_path(
                    &task.result_path,
                    input.clone(),
                    selected,
                )?));
            }
            Err(failure) if failure.error == RUNTIME => return Err(failure.into()),
            Err(failure) => failure,
        };

        match retry_delay(&task.retry, &mut attempts, &failure.error) {
            Some(delay) => {
                tokio::time::sleep(delay).await;
                retry_count += 1;
            }
            None => break failure,
        }
    };

    match find_catcher(&task.catch, &failure.error) {
        Some(catcher) => {
            let merged =
                apply_result_path(&catcher.result_path, input.clone(), failure.to_output())?;
            Ok(TaskOutcome::Caught(merged, catcher.next.clone()))
        }
        None => Err(failure.into()),
    }
}

async fn invoke(
    lambda: &Lambda,
    resource: &Resource,
    parameters: &Value,
) -> Result<Value, Failure> {
    match resource {
        Resource::LambdaInvoke => {
            let function = parameters
                .get("FunctionName")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    Failure::runtime("'FunctionName' in 'Parameters' must be a string".into())
                })?;
            let invocation_type = parameters.get("InvocationType").and_then(Value::as_str);
            let invocation = lambda
                .invoke(function, parameters.get("Payload"), invocation_type)
                .await?;
            // 本物はほかに SdkHttpMetadata と SdkResponseMetadata を付ける（sfn-local は付けない）。
            Ok(json!({
                "ExecutedVersion": invocation.executed_version,
                "Payload": invocation.payload,
                "StatusCode": invocation.status,
            }))
        }
        Resource::LambdaFunction(function) => Ok(lambda
            .invoke(function, Some(parameters), None)
            .await?
            .payload),
    }
}

/// 次の再試行までの待ち時間。再試行しないなら `None`。最初に一致した Retrier だけを見る（ASL の仕様）。
fn retry_delay(retriers: &[Retrier], attempts: &mut [u64], error: &str) -> Option<Duration> {
    let index = retriers
        .iter()
        .position(|retrier| matches_any(&retrier.error_equals, error))?;
    let retrier = &retriers[index];
    let count = attempts[index];
    if count >= retrier.max_attempts {
        return None;
    }
    attempts[index] += 1;
    let mut seconds = retrier.interval_seconds as f64 * retrier.backoff_rate.powi(count as i32);
    if let Some(max) = retrier.max_delay_seconds {
        seconds = seconds.min(max as f64);
    }
    Some(Duration::from_secs_f64(seconds))
}

fn find_catcher<'a>(catchers: &'a [Catcher], error: &str) -> Option<&'a Catcher> {
    catchers
        .iter()
        .find(|catcher| matches_any(&catcher.error_equals, error))
}

/// `States.ALL` は States.Runtime 以外のすべて、`States.TaskFailed` は States.Timeout 以外のすべてに一致する。
fn matches_any(names: &[String], error: &str) -> bool {
    names.iter().any(|name| match name.as_str() {
        "States.ALL" => error != RUNTIME,
        "States.TaskFailed" => error != RUNTIME && error != TIMEOUT,
        other => other == error,
    })
}

/// InputPath。`null` なら `{}`。
fn input_of(state: &State, input: &Value) -> Result<Value, Failure> {
    select(&state.input_path, input)
}

/// OutputPath。`null` なら `{}`。
fn output_of(state: &State, output: Value) -> Result<Value, Failure> {
    select(&state.output_path, &output)
}

fn select(path: &Option<Path>, value: &Value) -> Result<Value, Failure> {
    match path {
        Some(path) => path.select(value, &Value::Null).map_err(Failure::runtime),
        None => Ok(json!({})),
    }
}

/// ResultPath。`null` なら結果を捨てて入力をそのまま渡す。
fn apply_result_path(path: &Option<Path>, input: Value, result: Value) -> Result<Value, Failure> {
    match path {
        Some(path) => path.assign(input, result).map_err(Failure::runtime),
        None => Ok(input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retrier(
        errors: &[&str],
        interval: u64,
        max: u64,
        rate: f64,
        max_delay: Option<u64>,
    ) -> Retrier {
        Retrier {
            error_equals: errors.iter().map(|e| e.to_string()).collect(),
            interval_seconds: interval,
            max_attempts: max,
            backoff_rate: rate,
            max_delay_seconds: max_delay,
        }
    }

    #[test]
    fn 再試行の間隔は倍率で伸び_上限で止まる() {
        let retriers = [retrier(&["Lambda.ServiceException"], 2, 3, 2.0, Some(5))];
        let mut attempts = [0];
        let delays: Vec<_> =
            std::iter::from_fn(|| retry_delay(&retriers, &mut attempts, "Lambda.ServiceException"))
                .collect();
        assert_eq!(
            delays,
            [
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(5)
            ]
        );
    }

    #[test]
    fn 最初に一致した_retrier_だけを数える() {
        let retriers = [
            retrier(&["A"], 1, 0, 2.0, None),
            retrier(&["States.ALL"], 1, 5, 2.0, None),
        ];
        let mut attempts = [0, 0];
        assert_eq!(retry_delay(&retriers, &mut attempts, "A"), None);
        assert!(retry_delay(&retriers, &mut attempts, "B").is_some());
    }

    #[test]
    fn states_all_と_task_failed_の一致の範囲() {
        let all = ["States.ALL".to_string()];
        let task_failed = ["States.TaskFailed".to_string()];
        assert!(matches_any(&all, "Lambda.ServiceException"));
        assert!(matches_any(&all, TIMEOUT));
        assert!(!matches_any(&all, RUNTIME));
        assert!(matches_any(&task_failed, "Boom"));
        assert!(!matches_any(&task_failed, TIMEOUT));
    }
}
