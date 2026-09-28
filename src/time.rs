//! 時刻。内部は epoch ミリ秒の `i64` で持ち、wire では epoch 秒の小数（ミリ秒 3 桁）にする（2026-09-07 実測）。

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serializer;

pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// `#[serde(serialize_with = "time::epoch_seconds")]` 用。f64 の精度は足りる（epoch ミリ秒は 2^53 に収まる）。
pub fn epoch_seconds<S: Serializer>(millis: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(*millis as f64 / 1000.0)
}

pub fn epoch_seconds_opt<S: Serializer>(
    millis: &Option<i64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match millis {
        Some(millis) => epoch_seconds(millis, serializer),
        None => serializer.serialize_none(),
    }
}

/// コンテキストオブジェクトの時刻（`2026-09-28T12:34:56.789Z`）。
pub fn iso8601(millis: i64) -> String {
    let seconds = millis.div_euclid(1000);
    let ms = millis.rem_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{ms:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// epoch からの日数 → 年月日（Howard Hinnant の civil_from_days）。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_はミリ秒まで_utc_で書く() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(1_790_598_896_789), "2026-09-28T12:34:56.789Z");
        assert_eq!(iso8601(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }
}
