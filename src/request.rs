//! awsJson1.0 のリクエスト本文の解釈。
//!
//! 読めない JSON・object でない本文・型違いは `SerializationException`、必須の項目の欠落は
//! モデル制約の違反（`ValidationException`）にする。Step Functions では未実測で、Athena（同じ枠組み）で
//! 実測した形に倣った。`null` の項目は無いものとして扱う（Athena で実測）。未知の項目は無視する
//! （SDK が `traceHeader` などを足してくる）。

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::ApiError;

pub fn parse<T: DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(body) else {
        return Err(ApiError::serialization());
    };
    let value = Value::Object(map.into_iter().filter(|(_, v)| !v.is_null()).collect());

    serde_json::from_value(value).map_err(|e| {
        let text = e.to_string();
        match missing_field(&text) {
            Some(field) => ApiError::validation(&[format!(
                "Value null at '{field}' failed to satisfy constraint: Member must not be null"
            )]),
            None => ApiError::serialization(),
        }
    })
}

/// serde の `missing field `name`` から項目名を取り出す。
fn missing_field(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("missing field `")?;
    rest.split('`').next()
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Request {
        state_machine_arn: String,
        #[allow(dead_code)]
        name: Option<String>,
    }

    #[test]
    fn 必須の項目が無ければ_validation_exception() {
        let error = parse::<Request>(br#"{"name":"a"}"#).unwrap_err();
        assert_eq!(
            error,
            ApiError::validation(&["Value null at 'stateMachineArn' failed to satisfy constraint: Member must not be null".into()])
        );
    }

    #[test]
    fn null_の項目は無いものとして扱う() {
        let request = parse::<Request>(br#"{"stateMachineArn":"x","name":null}"#).unwrap();
        assert_eq!(request.state_machine_arn, "x");
        assert!(parse::<Request>(br#"{"stateMachineArn":null}"#).is_err());
    }

    #[test]
    fn 読めない本文と型違いは_serialization_exception() {
        for body in [&b"{"[..], b"[]", b"1", br#"{"stateMachineArn":1}"#] {
            assert_eq!(
                parse::<Request>(body).unwrap_err(),
                ApiError::serialization()
            );
        }
    }
}
