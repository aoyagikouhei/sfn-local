//! JSONPath の部分集合。対応するのは参照パス（`$`、`$.a.b`、`$['a']`、`$.a[0]`）と、
//! コンテキストオブジェクトの `$$...` だけ。ワイルドカード・フィルタ・`..`・組み込み関数は定義の読み込みで弾く。

use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Field(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    text: String,
    context: bool,
    segments: Vec<Segment>,
}

impl Path {
    pub fn parse(text: &str) -> Result<Self, String> {
        let (context, mut rest) = if let Some(rest) = text.strip_prefix("$$") {
            (true, rest)
        } else if let Some(rest) = text.strip_prefix('$') {
            (false, rest)
        } else {
            return Err(format!("path must start with '$': {text}"));
        };

        let mut segments = Vec::new();
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix("['") {
                let end = after
                    .find("']")
                    .ok_or_else(|| format!("unterminated bracket in path: {text}"))?;
                segments.push(Segment::Field(after[..end].to_string()));
                rest = &after[end + 2..];
            } else if let Some(after) = rest.strip_prefix('[') {
                let end = after
                    .find(']')
                    .ok_or_else(|| format!("unterminated bracket in path: {text}"))?;
                let index = after[..end]
                    .parse()
                    .map_err(|_| format!("unsupported path (sfn-local supports only field names and array indexes): {text}"))?;
                segments.push(Segment::Index(index));
                rest = &after[end + 1..];
            } else if let Some(after) = rest.strip_prefix('.') {
                let end = after.find(['.', '[']).unwrap_or(after.len());
                let field = &after[..end];
                if field.is_empty() || field == "*" {
                    return Err(format!(
                        "unsupported path (sfn-local supports only field names and array indexes): {text}"
                    ));
                }
                segments.push(Segment::Field(field.to_string()));
                rest = &after[end..];
            } else {
                return Err(format!("invalid path: {text}"));
            }
        }

        Ok(Self {
            text: text.to_string(),
            context,
            segments,
        })
    }

    pub fn is_context(&self) -> bool {
        self.context
    }

    /// 配列の添字を含まないか（ResultPath に使えるのはフィールド名だけ）。
    pub fn is_fields_only(&self) -> bool {
        self.segments
            .iter()
            .all(|segment| matches!(segment, Segment::Field(_)))
    }

    /// `input`（`$$` なら `context`）から値を取り出す。無ければ実行時エラーの文言。
    pub fn select(&self, input: &Value, context: &Value) -> Result<Value, String> {
        let mut current = if self.context { context } else { input };
        for segment in &self.segments {
            let next = match segment {
                Segment::Field(name) => current.get(name.as_str()),
                Segment::Index(index) => current.get(*index),
            };
            current = next.ok_or_else(|| format!("Invalid path '{}': No results", self.text))?;
        }
        Ok(current.clone())
    }

    /// ResultPath の書き込み。途中のフィールドが無ければ object を作る。
    pub fn assign(&self, root: Value, value: Value) -> Result<Value, String> {
        let fields: Vec<&str> = self
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Field(name) => name.as_str(),
                Segment::Index(_) => unreachable!("ResultPath は読み込みでフィールド名だけに絞る"),
            })
            .collect();
        assign(root, &fields, value).ok_or_else(|| {
            format!(
                "Unable to apply ResultPath '{}': the input is not a JSON object at that path",
                self.text
            )
        })
    }
}

fn assign(root: Value, fields: &[&str], value: Value) -> Option<Value> {
    let Some((first, rest)) = fields.split_first() else {
        return Some(value);
    };
    let Value::Object(mut map) = root else {
        return None;
    };
    let child = map
        .remove(*first)
        .unwrap_or_else(|| Value::Object(Map::new()));
    let child = if rest.is_empty() {
        value
    } else {
        assign(child, rest, value)?
    };
    map.insert((*first).to_string(), child);
    Some(Value::Object(map))
}

/// Parameters・ResultSelector の中の `.$` で終わるキーを検証する。`pointer` はエラーの位置。
pub fn validate_template(template: &Value, pointer: &str) -> Result<(), String> {
    match template {
        Value::Object(map) => {
            for (key, value) in map {
                let here = format!("{pointer}/{key}");
                if key.ends_with(".$") {
                    let Value::String(text) = value else {
                        return Err(format!(
                            "the value of a field ending with '.$' must be a string at {here}"
                        ));
                    };
                    if text.starts_with("States.") {
                        return Err(format!(
                            "intrinsic functions are not supported by sfn-local yet at {here}"
                        ));
                    }
                    Path::parse(text).map_err(|e| format!("{e} at {here}"))?;
                } else {
                    validate_template(value, &here)?;
                }
            }
            Ok(())
        }
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(i, item)| validate_template(item, &format!("{pointer}/{i}"))),
        _ => Ok(()),
    }
}

/// Parameters・ResultSelector を評価する。`.$` で終わるキーはパスの値に置き換え、キーから `.$` を落とす。
pub fn render_template(template: &Value, input: &Value, context: &Value) -> Result<Value, String> {
    match template {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, value) in map {
                match (key.strip_suffix(".$"), value) {
                    (Some(name), Value::String(text)) => {
                        let path = Path::parse(text)?;
                        out.insert(name.to_string(), path.select(input, context)?);
                    }
                    _ => {
                        out.insert(key.clone(), render_template(value, input, context)?);
                    }
                }
            }
            Ok(Value::Object(out))
        }
        Value::Array(items) => items
            .iter()
            .map(|item| render_template(item, input, context))
            .collect::<Result<_, _>>()
            .map(Value::Array),
        other => Ok(other.clone()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn 参照パスで値を取り出す() {
        let input = json!({"a": {"b": [10, {"c": "x"}]}, "d e": 1});
        let ctx = json!({"Execution": {"Name": "run"}});
        let get = |text: &str| Path::parse(text).unwrap().select(&input, &ctx);
        assert_eq!(get("$").unwrap(), input);
        assert_eq!(get("$.a.b[0]").unwrap(), json!(10));
        assert_eq!(get("$.a.b[1].c").unwrap(), json!("x"));
        assert_eq!(get("$['d e']").unwrap(), json!(1));
        assert_eq!(get("$$.Execution.Name").unwrap(), json!("run"));
        assert!(get("$.missing").is_err());
    }

    #[test]
    fn 対応しないパスは読み込みで弾く() {
        for text in ["a", "$..a", "$.a[*]", "$.a[?(@.b)]", "$.*", "$."] {
            assert!(Path::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn result_path_は途中の_object_を作って書き込む() {
        let path = Path::parse("$.a.b").unwrap();
        assert_eq!(
            path.assign(json!({"x": 1}), json!(2)).unwrap(),
            json!({"x": 1, "a": {"b": 2}})
        );
        assert_eq!(
            Path::parse("$")
                .unwrap()
                .assign(json!({"x": 1}), json!(2))
                .unwrap(),
            json!(2)
        );
        assert!(path.assign(json!([1]), json!(2)).is_err());
    }

    #[test]
    fn テンプレートは_ドル_で終わるキーをパスの値に置き換える() {
        let template = json!({"FunctionName": "f", "Payload.$": "$", "Nested": [{"Name.$": "$$.Execution.Name"}]});
        let input = json!({"k": 1});
        let ctx = json!({"Execution": {"Name": "run"}});
        assert_eq!(
            render_template(&template, &input, &ctx).unwrap(),
            json!({"FunctionName": "f", "Payload": {"k": 1}, "Nested": [{"Name": "run"}]})
        );
    }

    #[test]
    fn 組み込み関数はまだ弾く() {
        let template = json!({"a.$": "States.Format('{}', $.x)"});
        assert!(validate_template(&template, "/States/A/Parameters").is_err());
    }
}
