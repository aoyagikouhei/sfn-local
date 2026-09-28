//! ARN と名前の規則。

pub const STATE_MACHINE_SEGMENT: &str = ":stateMachine:";
pub const EXECUTION_SEGMENT: &str = ":execution:";

/// ステートマシン名・実行名の最大長。バイト数ではなく文字数で数える（2026-09-07 実測）。
pub const NAME_MAX_CHARS: usize = 80;

/// ARN の形の粗い判定。`InvalidArn` と `*DoesNotExist` の境界は未実測で、実測したのは
/// `not-an-arn` のような明らかに壊れた入力だけ（2026-09-07）。
pub fn is_arn_like(arn: &str, segment: &str) -> bool {
    arn.starts_with("arn:") && arn.contains(segment)
}

pub fn state_machine_arn(region: &str, account_id: &str, name: &str) -> String {
    format!("arn:aws:states:{region}:{account_id}{STATE_MACHINE_SEGMENT}{name}")
}

/// 実行の ARN は、ステートマシンの ARN の `:stateMachine:` を `:execution:` にして末尾に実行名を足したもの
/// （2026-09-07 実測）。
pub fn execution_arn(state_machine_arn: &str, name: &str) -> String {
    format!(
        "{}:{name}",
        state_machine_arn.replacen(STATE_MACHINE_SEGMENT, EXECUTION_SEGMENT, 1)
    )
}

/// 名前に使えない文字（AWS のドキュメントの一覧。空白、`< > { } [ ] ? * " # % \ ^ | ~ ` $ & , ; : /`、制御文字）。
/// 単独のサロゲートは JSON の段階で弾かれるので来ない。
pub fn has_forbidden_name_char(name: &str) -> bool {
    name.chars().any(|c| {
        c.is_whitespace()
            || "<>{}[]?*\"#%\\^|~`$&,;:/".contains(c)
            || ('\u{0}'..='\u{1F}').contains(&c)
            || ('\u{7F}'..='\u{9F}').contains(&c)
            || c == '\u{FFFE}'
            || c == '\u{FFFF}'
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 実行の_arn_はステートマシンの_arn_から組み立てる() {
        assert_eq!(
            execution_arn(
                "arn:aws:states:us-east-1:123456789012:stateMachine:example",
                "run-1"
            ),
            "arn:aws:states:us-east-1:123456789012:execution:example:run-1"
        );
    }

    #[test]
    fn arn_の形は前置きとセグメントで見る() {
        let arn = "arn:aws:states:us-east-1:123456789012:stateMachine:example";
        assert!(is_arn_like(arn, STATE_MACHINE_SEGMENT));
        assert!(!is_arn_like("not-an-arn", STATE_MACHINE_SEGMENT));
        assert!(!is_arn_like(
            "arn:aws:states:us-east-1:123456789012:activity:example",
            STATE_MACHINE_SEGMENT
        ));
    }

    #[test]
    fn 名前に使えない文字を弾く() {
        for bad in [
            "a b",
            "a\tb",
            "a<b",
            "a{b",
            "a?b",
            "a\"b",
            "a\\b",
            "a`b",
            "a$b",
            "a:b",
            "a/b",
            "a\u{1}b",
            "a\u{7F}b",
            "a\u{FFFE}b",
        ] {
            assert!(has_forbidden_name_char(bad), "{bad:?}");
        }
        for ok in ["abc", "a-b_c.d", "0123", "あいう"] {
            assert!(!has_forbidden_name_char(ok), "{ok:?}");
        }
    }
}
