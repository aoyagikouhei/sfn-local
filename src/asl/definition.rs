//! ASL（Amazon States Language）の定義の読み込み。sfn-local が実行できない定義は、読み込みの時点で
//! `UNSUPPORTED_BY_SFN_LOCAL` として弾く（実行してから途中で止まるより、原因が近い）。

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::path::{Path, validate_template};

/// Task の既定のタイムアウト（ASL の仕様の既定値）。
const DEFAULT_TASK_TIMEOUT_SECONDS: u64 = 99_999_999;

#[derive(Debug, Clone)]
pub struct Definition {
    pub start_at: String,
    pub states: HashMap<String, State>,
    /// トップレベルの TimeoutSeconds。超えた実行は TIMED_OUT になる。
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct State {
    pub kind: StateKind,
    /// `None` は終端（`End: true`、Succeed、Fail）。
    pub next: Option<String>,
    pub input_path: Option<Path>,
    pub output_path: Option<Path>,
}

#[derive(Debug, Clone)]
pub enum StateKind {
    Pass {
        result: Option<Value>,
        parameters: Option<Value>,
        result_path: Option<Path>,
    },
    Task(Box<Task>),
    Succeed,
    Fail {
        error: Option<String>,
        cause: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Task {
    pub resource: Resource,
    pub parameters: Option<Value>,
    pub result_selector: Option<Value>,
    /// `None` は `ResultPath: null`（結果を捨てて入力をそのまま渡す）。
    pub result_path: Option<Path>,
    pub timeout_seconds: u64,
    pub retry: Vec<Retrier>,
    pub catch: Vec<Catcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resource {
    /// `arn:aws:states:::lambda:invoke`（最適化された統合）。結果は `Payload` などで包まれる。
    LambdaInvoke,
    /// Lambda の関数の ARN を直に書いたもの。結果は関数の戻り値そのもの。
    LambdaFunction(String),
}

#[derive(Debug, Clone)]
pub struct Retrier {
    pub error_equals: Vec<String>,
    pub interval_seconds: u64,
    pub max_attempts: u64,
    pub backoff_rate: f64,
    pub max_delay_seconds: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Catcher {
    pub error_equals: Vec<String>,
    pub next: String,
    pub result_path: Option<Path>,
}

/// 読み込みの失敗。文言は `Invalid State Machine Definition: '<ここ>'` の括弧の中に入る。
pub fn parse(text: &str) -> Result<Definition, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|e| format!("INVALID_JSON_DESCRIPTION: {e}"))?;
    let Value::Object(root) = value else {
        return Err(schema("the definition must be a JSON object", ""));
    };

    if root.get("QueryLanguage").and_then(Value::as_str) == Some("JSONata") {
        return Err(unsupported("JSONata", "/QueryLanguage"));
    }
    let start_at = required_str(&root, "StartAt", "")?;
    let timeout_seconds = optional_u64(&root, "TimeoutSeconds", "")?;
    let Some(Value::Object(raw_states)) = root.get("States") else {
        return Err(schema(
            "the field 'States' is required and must be an object",
            "",
        ));
    };
    if raw_states.is_empty() {
        return Err(schema("the field 'States' must not be empty", "/States"));
    }

    let mut states = HashMap::new();
    for (name, raw) in raw_states {
        let pointer = format!("/States/{name}");
        let Value::Object(raw) = raw else {
            return Err(schema("a state must be a JSON object", &pointer));
        };
        states.insert(name.clone(), parse_state(raw, &pointer)?);
    }

    if !states.contains_key(&start_at) {
        return Err(schema(
            &format!("missing 'Next' target: {start_at}"),
            "/StartAt",
        ));
    }
    for (name, state) in &states {
        let mut targets: Vec<&String> = state.next.iter().collect();
        if let StateKind::Task(task) = &state.kind {
            targets.extend(task.catch.iter().map(|catcher| &catcher.next));
        }
        for target in targets {
            if !states.contains_key(target) {
                return Err(schema(
                    &format!("missing 'Next' target: {target}"),
                    &format!("/States/{name}"),
                ));
            }
        }
    }

    Ok(Definition {
        start_at,
        states,
        timeout_seconds,
    })
}

fn parse_state(raw: &Map<String, Value>, pointer: &str) -> Result<State, String> {
    for field in ["Assign", "Arguments", "Output", "Items", "ItemSelector"] {
        if raw.contains_key(field) {
            return Err(unsupported(&format!("the field '{field}'"), pointer));
        }
    }
    let kind_name = required_str(raw, "Type", pointer)?;
    let kind = match kind_name.as_str() {
        "Pass" => StateKind::Pass {
            result: raw.get("Result").cloned(),
            parameters: template(raw, "Parameters", pointer)?,
            result_path: result_path(raw, pointer)?,
        },
        "Task" => StateKind::Task(Box::new(parse_task(raw, pointer)?)),
        "Succeed" => StateKind::Succeed,
        "Fail" => {
            for field in ["ErrorPath", "CausePath"] {
                if raw.contains_key(field) {
                    return Err(unsupported(&format!("the field '{field}'"), pointer));
                }
            }
            StateKind::Fail {
                error: optional_str(raw, "Error", pointer)?,
                cause: optional_str(raw, "Cause", pointer)?,
            }
        }
        "Choice" | "Wait" | "Parallel" | "Map" => {
            return Err(unsupported(
                &format!("the state type '{kind_name}'"),
                pointer,
            ));
        }
        other => return Err(schema(&format!("unknown state type '{other}'"), pointer)),
    };

    let terminal = matches!(kind, StateKind::Succeed | StateKind::Fail { .. });
    let next = optional_str(raw, "Next", pointer)?;
    let end = raw.get("End").and_then(Value::as_bool) == Some(true);
    if !terminal {
        match (&next, end) {
            (Some(_), false) | (None, true) => {}
            (Some(_), true) => {
                return Err(schema(
                    "'Next' and 'End' must not be used together",
                    pointer,
                ));
            }
            (None, false) => return Err(schema("either 'Next' or 'End' is required", pointer)),
        }
    }

    let (input_path, output_path) = if matches!(kind, StateKind::Fail { .. }) {
        (None, None)
    } else {
        (
            path(raw, "InputPath", pointer)?,
            path(raw, "OutputPath", pointer)?,
        )
    };

    Ok(State {
        kind,
        next: if terminal { None } else { next },
        input_path,
        output_path,
    })
}

fn parse_task(raw: &Map<String, Value>, pointer: &str) -> Result<Task, String> {
    let resource_text = required_str(raw, "Resource", pointer)?;
    let resource = resource(&resource_text)
        .ok_or_else(|| unsupported(&format!("the resource '{resource_text}'"), pointer))?;
    let parameters = template(raw, "Parameters", pointer)?;
    if resource == Resource::LambdaInvoke {
        let has_function = parameters
            .as_ref()
            .and_then(Value::as_object)
            .is_some_and(|map| {
                map.contains_key("FunctionName") || map.contains_key("FunctionName.$")
            });
        if !has_function {
            return Err(schema(
                "the field 'FunctionName' is required in 'Parameters'",
                pointer,
            ));
        }
    }

    let retry = match raw.get("Retry") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, item)| parse_retrier(item, &format!("{pointer}/Retry/{i}")))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(schema("'Retry' must be an array", pointer)),
    };
    let catch = match raw.get("Catch") {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, item)| parse_catcher(item, &format!("{pointer}/Catch/{i}")))
            .collect::<Result<_, _>>()?,
        Some(_) => return Err(schema("'Catch' must be an array", pointer)),
    };

    Ok(Task {
        resource,
        parameters,
        result_selector: template(raw, "ResultSelector", pointer)?,
        result_path: result_path(raw, pointer)?,
        timeout_seconds: optional_u64(raw, "TimeoutSeconds", pointer)?
            .unwrap_or(DEFAULT_TASK_TIMEOUT_SECONDS),
        retry,
        catch,
    })
}

/// 対応する Task の Resource。コールバック（`.waitForTaskToken`）や `.sync` は未対応。
fn resource(text: &str) -> Option<Resource> {
    let parts: Vec<&str> = text.split(':').collect();
    match parts.as_slice() {
        ["arn", _, "states", "", "", "lambda", "invoke"] => Some(Resource::LambdaInvoke),
        ["arn", _, "lambda", _, _, "function", _, ..] if parts.len() <= 8 => {
            Some(Resource::LambdaFunction(text.to_string()))
        }
        _ => None,
    }
}

fn parse_retrier(raw: &Value, pointer: &str) -> Result<Retrier, String> {
    let Value::Object(raw) = raw else {
        return Err(schema("a retrier must be a JSON object", pointer));
    };
    Ok(Retrier {
        error_equals: error_equals(raw, pointer)?,
        interval_seconds: optional_u64(raw, "IntervalSeconds", pointer)?.unwrap_or(1),
        max_attempts: optional_u64(raw, "MaxAttempts", pointer)?.unwrap_or(3),
        backoff_rate: match raw.get("BackoffRate") {
            None => 2.0,
            Some(value) => value
                .as_f64()
                .filter(|rate| *rate >= 1.0)
                .ok_or_else(|| schema("'BackoffRate' must be a number >= 1.0", pointer))?,
        },
        max_delay_seconds: optional_u64(raw, "MaxDelaySeconds", pointer)?,
    })
}

fn parse_catcher(raw: &Value, pointer: &str) -> Result<Catcher, String> {
    let Value::Object(raw) = raw else {
        return Err(schema("a catcher must be a JSON object", pointer));
    };
    Ok(Catcher {
        error_equals: error_equals(raw, pointer)?,
        next: required_str(raw, "Next", pointer)?,
        result_path: result_path(raw, pointer)?,
    })
}

fn error_equals(raw: &Map<String, Value>, pointer: &str) -> Result<Vec<String>, String> {
    let names: Option<Vec<String>> =
        raw.get("ErrorEquals")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            });
    match names {
        Some(names) if !names.is_empty() => Ok(names),
        _ => Err(schema(
            "'ErrorEquals' must be a non-empty array of strings",
            pointer,
        )),
    }
}

/// InputPath・OutputPath。無ければ `$`、`null` なら `None`（`{}` を渡す）。
fn path(raw: &Map<String, Value>, field: &str, pointer: &str) -> Result<Option<Path>, String> {
    match raw.get(field) {
        None => Ok(Some(Path::parse("$").expect("$ は読める"))),
        Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Path::parse(text)
            .map(Some)
            .map_err(|e| schema(&e, &format!("{pointer}/{field}"))),
        Some(_) => Err(schema(
            &format!("'{field}' must be a string or null"),
            pointer,
        )),
    }
}

fn result_path(raw: &Map<String, Value>, pointer: &str) -> Result<Option<Path>, String> {
    let path = path(raw, "ResultPath", pointer)?;
    if let Some(path) = &path
        && (path.is_context() || !path.is_fields_only())
    {
        return Err(schema(
            "'ResultPath' must be a path of field names under '$'",
            pointer,
        ));
    }
    Ok(path)
}

fn template(raw: &Map<String, Value>, field: &str, pointer: &str) -> Result<Option<Value>, String> {
    let Some(value) = raw.get(field) else {
        return Ok(None);
    };
    validate_template(value, &format!("{pointer}/{field}")).map_err(|e| {
        if e.starts_with("intrinsic") {
            format!("UNSUPPORTED_BY_SFN_LOCAL: {e}")
        } else {
            format!("SCHEMA_VALIDATION_FAILED: {e}")
        }
    })?;
    Ok(Some(value.clone()))
}

fn required_str(raw: &Map<String, Value>, field: &str, pointer: &str) -> Result<String, String> {
    optional_str(raw, field, pointer)?
        .ok_or_else(|| schema(&format!("the field '{field}' is required"), pointer))
}

fn optional_str(
    raw: &Map<String, Value>,
    field: &str,
    pointer: &str,
) -> Result<Option<String>, String> {
    match raw.get(field) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(schema(&format!("'{field}' must be a string"), pointer)),
    }
}

fn optional_u64(
    raw: &Map<String, Value>,
    field: &str,
    pointer: &str,
) -> Result<Option<u64>, String> {
    match raw.get(field) {
        None => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or_else(|| {
            schema(
                &format!("'{field}' must be a non-negative integer"),
                pointer,
            )
        }),
    }
}

fn schema(message: &str, pointer: &str) -> String {
    let at = if pointer.is_empty() { "/" } else { pointer };
    format!("SCHEMA_VALIDATION_FAILED: {message} at {at}")
}

/// 本物は受け付けるが sfn-local がまだ実行できないもの。本物には無い前置き。
fn unsupported(what: &str, pointer: &str) -> String {
    let at = if pointer.is_empty() { "/" } else { pointer };
    format!("UNSUPPORTED_BY_SFN_LOCAL: {what} is not supported by sfn-local yet at {at}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lambda_を呼ぶ_task_を読む() {
        let definition = parse(
            r#"{
              "StartAt": "Invoke",
              "TimeoutSeconds": 840,
              "States": {
                "Invoke": {
                  "Type": "Task",
                  "Resource": "arn:aws:states:::lambda:invoke",
                  "Parameters": {"FunctionName": "arn:aws:lambda:us-east-1:123456789012:function:example", "Payload.$": "$"},
                  "OutputPath": "$.Payload",
                  "Retry": [{"ErrorEquals": ["Lambda.ServiceException"], "IntervalSeconds": 2, "MaxAttempts": 6, "BackoffRate": 2}],
                  "End": true
                }
              }
            }"#,
        )
        .unwrap();
        assert_eq!(definition.timeout_seconds, Some(840));
        let StateKind::Task(task) = &definition.states["Invoke"].kind else {
            panic!("Task のはず");
        };
        assert_eq!(task.resource, Resource::LambdaInvoke);
        assert_eq!(task.retry[0].max_attempts, 6);
        assert_eq!(task.timeout_seconds, DEFAULT_TASK_TIMEOUT_SECONDS);
    }

    #[test]
    fn 実行できない定義は読み込みで弾く() {
        let cases = [
            ("{", "INVALID_JSON_DESCRIPTION"),
            (
                r#"{"States": {"A": {"Type": "Succeed"}}}"#,
                "'StartAt' is required",
            ),
            (
                r#"{"StartAt": "B", "States": {"A": {"Type": "Succeed"}}}"#,
                "missing 'Next' target: B",
            ),
            (
                r#"{"StartAt": "A", "States": {"A": {"Type": "Pass"}}}"#,
                "either 'Next' or 'End'",
            ),
            (
                r#"{"StartAt": "A", "States": {"A": {"Type": "Pass", "Next": "Z"}}}"#,
                "missing 'Next' target: Z",
            ),
            (
                r#"{"StartAt": "A", "States": {"A": {"Type": "Choice", "Choices": []}}}"#,
                "UNSUPPORTED_BY_SFN_LOCAL: the state type 'Choice'",
            ),
            (
                r#"{"StartAt": "A", "States": {"A": {"Type": "Task", "Resource": "arn:aws:states:::sqs:sendMessage", "End": true}}}"#,
                "UNSUPPORTED_BY_SFN_LOCAL: the resource",
            ),
            (
                r#"{"StartAt": "A", "States": {"A": {"Type": "Task", "Resource": "arn:aws:states:::lambda:invoke", "End": true}}}"#,
                "'FunctionName' is required",
            ),
            (
                r#"{"StartAt": "A", "QueryLanguage": "JSONata", "States": {"A": {"Type": "Succeed"}}}"#,
                "UNSUPPORTED_BY_SFN_LOCAL: JSONata",
            ),
            (
                r#"{"StartAt": "A", "States": {"A": {"Type": "Pass", "ResultPath": "$.a[0]", "End": true}}}"#,
                "'ResultPath' must be",
            ),
        ];
        for (text, expected) in cases {
            let error = parse(text).unwrap_err();
            assert!(error.contains(expected), "{text}: {error}");
        }
    }

    #[test]
    fn lambda_の関数の_arn_を直に書いた_resource_を読む() {
        assert_eq!(
            resource("arn:aws:lambda:us-east-1:123456789012:function:example:live"),
            Some(Resource::LambdaFunction(
                "arn:aws:lambda:us-east-1:123456789012:function:example:live".into()
            ))
        );
        assert_eq!(
            resource("arn:aws:states:::lambda:invoke.waitForTaskToken"),
            None
        );
    }
}
