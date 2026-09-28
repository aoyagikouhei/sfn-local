//! Lambda の Invoke API（`POST {endpoint}/2015-03-31/functions/{name}/invocations`）の呼び出し。
//! 署名はしない。宛先は `SFN_LOCAL_LAMBDA_ENDPOINT`（cargo lambda watch、LocalStack など）。
//!
//! 応答の読み方:
//! - 2xx で `X-Amz-Function-Error` がある → 関数のエラー。`error` は本文の `errorType`、`cause` は本文そのもの。
//! - 2xx でヘッダが無い → 成功（本文は覗かない。`{"error": ...}` を返しても成功のまま）。
//! - 2xx 以外で本文に `errorType` がある → 関数のエラー。本物の Lambda は関数のエラーを 200 で返すが、
//!   cargo lambda watch は 500 とヘッダ無しで返す（2026-09-08 実測）ので、同じものとして読む。
//! - それ以外の 2xx 以外 → `x-amzn-ErrorType` があれば `Lambda.<その名前>`、無ければ `Lambda.ServiceException`。
//! - つながらない → `Lambda.SdkClientException`。
//!
//! 後ろの 2 つの `error` の名前は、本物の Step Functions の Lambda 統合が使う名前（AWS のドキュメント）に
//! 合わせた。本物で同じ状況を作って測ってはいない。

use serde_json::Value;

use super::Failure;

const INVOKE_PATH: &str = "/2015-03-31/functions/";
const FUNCTION_ERROR_HEADER: &str = "x-amz-function-error";
const ERROR_TYPE_HEADER: &str = "x-amzn-errortype";
const EXECUTED_VERSION_HEADER: &str = "x-amz-executed-version";

pub struct Lambda {
    endpoint: Option<String>,
    client: reqwest::Client,
}

/// 成功した呼び出し。
#[derive(Debug)]
pub struct Invocation {
    pub status: u16,
    pub executed_version: String,
    pub payload: Value,
}

impl Lambda {
    pub fn new(endpoint: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            // ホストやコンテナに注入された HTTP_PROXY を使わない（ローカルの Lambda にだけつなぐ）。
            .no_proxy()
            .build()
            .unwrap_or_default();
        Self { endpoint, client }
    }

    /// `function` は関数名、関数の ARN、部分 ARN（`123456789012:function:name`）のどれでもよい。
    /// `payload` が `None` なら本文を空で送る。
    pub async fn invoke(
        &self,
        function: &str,
        payload: Option<&Value>,
        invocation_type: Option<&str>,
    ) -> Result<Invocation, Failure> {
        let Some(endpoint) = &self.endpoint else {
            return Err(Failure::new(
                "Lambda.SdkClientException",
                "SFN_LOCAL_LAMBDA_ENDPOINT is not set, so sfn-local cannot invoke Lambda functions",
            ));
        };
        let (name, qualifier) = function_name(function);
        let mut url = format!(
            "{}{INVOKE_PATH}{name}/invocations",
            endpoint.trim_end_matches('/')
        );
        if let Some(qualifier) = qualifier {
            url.push_str("?Qualifier=");
            url.push_str(qualifier);
        }

        let mut request = self.client.post(&url);
        if let Some(payload) = payload {
            request = request.body(payload.to_string());
        }
        if let Some(invocation_type) = invocation_type {
            request = request.header("x-amz-invocation-type", invocation_type);
        }

        let response = request.send().await.map_err(|e| {
            Failure::new(
                "Lambda.SdkClientException",
                &format!("invoking {url} failed: {}", describe(&e)),
            )
        })?;
        let status = response.status();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        let function_error = header(FUNCTION_ERROR_HEADER).is_some();
        let error_type_header = header(ERROR_TYPE_HEADER);
        let executed_version = header(EXECUTED_VERSION_HEADER).unwrap_or_else(|| "$LATEST".into());
        let body = response.text().await.map_err(|e| {
            Failure::new(
                "Lambda.SdkClientException",
                &format!("reading the response from {url} failed: {}", describe(&e)),
            )
        })?;

        if status.is_success() {
            if function_error {
                return Err(function_failure(&body)
                    .unwrap_or_else(|| Failure::with_cause("Lambda.Unknown", body)));
            }
            // 非同期呼び出し（`Event`）は 202 と空の本文。JSON でない本文は文字列として渡す。
            let payload = serde_json::from_str(&body).unwrap_or(Value::String(body));
            return Ok(Invocation {
                status: status.as_u16(),
                executed_version,
                payload,
            });
        }

        if let Some(failure) = function_failure(&body) {
            return Err(failure);
        }
        let error = match error_type_header {
            Some(error_type) => {
                let name = error_type.split(':').next().unwrap_or_default();
                format!("Lambda.{name}")
            }
            None => "Lambda.ServiceException".to_string(),
        };
        let cause = if body.is_empty() {
            format!("{url} returned HTTP {}", status.as_u16())
        } else {
            body
        };
        Err(Failure::with_cause(&error, cause))
    }
}

/// 本文が `errorType` を持つ JSON なら関数のエラー。`cause` は本文そのもの（JSON 文字列）。
fn function_failure(body: &str) -> Option<Failure> {
    let value: Value = serde_json::from_str(body).ok()?;
    let error_type = value.get("errorType")?.as_str()?;
    if error_type.is_empty() {
        return None;
    }
    Some(Failure::with_cause(error_type, body.to_string()))
}

/// 関数名と修飾子（バージョンかエイリアス）。
fn function_name(function: &str) -> (&str, Option<&str>) {
    let rest = match function.find(":function:") {
        Some(index) => &function[index + ":function:".len()..],
        None => function,
    };
    match rest.split_once(':') {
        Some((name, qualifier)) => (name, Some(qualifier)),
        None => (rest, None),
    }
}

/// `reqwest::Error` の理由を最後まで辿る（`Display` だけでは接続拒否と名前解決の失敗が区別できない）。
fn describe(error: &reqwest::Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut source = std::error::Error::source(error);
    while let Some(e) = source {
        parts.push(e.to_string());
        source = e.source();
    }
    parts.join(": ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 関数名は名前_arn_部分_arn_のどれからでも取り出す() {
        assert_eq!(function_name("example"), ("example", None));
        assert_eq!(function_name("example:live"), ("example", Some("live")));
        assert_eq!(
            function_name("arn:aws:lambda:us-east-1:123456789012:function:example"),
            ("example", None)
        );
        assert_eq!(
            function_name("arn:aws:lambda:us-east-1:123456789012:function:example:$LATEST"),
            ("example", Some("$LATEST"))
        );
        assert_eq!(
            function_name("123456789012:function:example"),
            ("example", None)
        );
    }

    #[test]
    fn error_type_が空か文字列でなければ関数のエラーとは読まない() {
        assert!(function_failure(r#"{"errorType":""}"#).is_none());
        assert!(function_failure(r#"{"errorType":1}"#).is_none());
        assert!(function_failure(r#"{"detail":"missing function"}"#).is_none());
        let failure = function_failure(r#"{"errorType":"Boom","errorMessage":"x"}"#).unwrap();
        assert_eq!(failure.error, "Boom");
        assert_eq!(
            failure.cause.as_deref(),
            Some(r#"{"errorType":"Boom","errorMessage":"x"}"#)
        );
    }
}
