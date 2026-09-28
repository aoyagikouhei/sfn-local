use std::env;
use std::path::PathBuf;

/// 待ち受けアドレスの既定。
pub const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:8083";
/// ARN に入れるリージョンの既定。
pub const DEFAULT_REGION: &str = "us-east-1";
/// ARN に入れるアカウント ID の既定。
pub const DEFAULT_ACCOUNT_ID: &str = "123456789012";

/// sfn-local の設定。すべて環境変数で与える。
pub struct Config {
    /// HTTP の待ち受けアドレス。
    pub bind_address: String,
    /// ステートマシンの ARN に入れるリージョン。
    pub region: String,
    /// ステートマシンの ARN に入れるアカウント ID。
    pub account_id: String,
    /// 起動時に読み込む ASL の置き場（`<名前>.asl.json`）。
    pub state_machines_dir: Option<PathBuf>,
    /// Lambda の Invoke API の接続先（例: `http://localhost:9001`）。
    pub lambda_endpoint: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// 環境変数を読むクロージャを差し替えられる形。テストは本物の環境変数を触らない（並列に走るため）。
    fn from_lookup(env: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let non_empty = |key: &str| match env(key) {
            Some(text) if text.trim().is_empty() => Err(format!("{key}: 空です")),
            other => Ok(other),
        };

        let region = non_empty("SFN_LOCAL_REGION")?.unwrap_or_else(|| DEFAULT_REGION.into());
        let account_id =
            non_empty("SFN_LOCAL_ACCOUNT_ID")?.unwrap_or_else(|| DEFAULT_ACCOUNT_ID.into());
        if !account_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(format!(
                "SFN_LOCAL_ACCOUNT_ID: 数字だけで書いてください: {account_id:?}"
            ));
        }

        let lambda_endpoint = non_empty("SFN_LOCAL_LAMBDA_ENDPOINT")?;
        if let Some(endpoint) = &lambda_endpoint
            && !endpoint.starts_with("http://")
        {
            return Err(format!(
                "SFN_LOCAL_LAMBDA_ENDPOINT: http:// で始まる URL を指定してください: {endpoint:?}"
            ));
        }

        Ok(Self {
            bind_address: non_empty("SFN_LOCAL_BIND")?
                .unwrap_or_else(|| DEFAULT_BIND_ADDRESS.into()),
            region,
            account_id,
            state_machines_dir: non_empty("SFN_LOCAL_STATE_MACHINES_DIR")?.map(PathBuf::from),
            lambda_endpoint,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[test]
    fn 未設定なら既定を使う() {
        let config = Config::from_lookup(|_| None).unwrap();
        assert_eq!(config.bind_address, DEFAULT_BIND_ADDRESS);
        assert_eq!(config.region, DEFAULT_REGION);
        assert_eq!(config.account_id, DEFAULT_ACCOUNT_ID);
        assert_eq!(config.state_machines_dir, None);
        assert_eq!(config.lambda_endpoint, None);
    }

    #[test]
    fn 環境変数で変えられる() {
        let config = Config::from_lookup(lookup(&[
            ("SFN_LOCAL_BIND", "127.0.0.1:9000"),
            ("SFN_LOCAL_REGION", "ap-northeast-1"),
            ("SFN_LOCAL_ACCOUNT_ID", "111122223333"),
            ("SFN_LOCAL_STATE_MACHINES_DIR", "/state-machines"),
            ("SFN_LOCAL_LAMBDA_ENDPOINT", "http://lambda:9001"),
        ]))
        .unwrap();
        assert_eq!(config.bind_address, "127.0.0.1:9000");
        assert_eq!(config.region, "ap-northeast-1");
        assert_eq!(config.account_id, "111122223333");
        assert_eq!(
            config.state_machines_dir,
            Some(PathBuf::from("/state-machines"))
        );
        assert_eq!(
            config.lambda_endpoint.as_deref(),
            Some("http://lambda:9001")
        );
    }

    #[test]
    fn 形の崩れた値は起動時に止める() {
        let cases = [
            (("SFN_LOCAL_BIND", " "), "SFN_LOCAL_BIND: 空です"),
            (
                ("SFN_LOCAL_ACCOUNT_ID", "abc"),
                "SFN_LOCAL_ACCOUNT_ID: 数字だけで書いてください: \"abc\"",
            ),
            (
                ("SFN_LOCAL_LAMBDA_ENDPOINT", "lambda:9001"),
                "SFN_LOCAL_LAMBDA_ENDPOINT: http:// で始まる URL を指定してください: \"lambda:9001\"",
            ),
        ];
        for (pair, expected) in cases {
            let error = Config::from_lookup(lookup(&[pair])).err().unwrap();
            assert_eq!(error, expected);
        }
    }
}
