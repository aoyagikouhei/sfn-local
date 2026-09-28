use std::env;

/// 待ち受けアドレスの既定。
pub const DEFAULT_BIND_ADDRESS: &str = "0.0.0.0:8083";

/// sfn-local の設定。すべて環境変数で与える。
pub struct Config {
    /// HTTP の待ち受けアドレス。
    pub bind_address: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    /// 環境変数を読むクロージャを差し替えられる形。テストは本物の環境変数を触らない（並列に走るため）。
    fn from_lookup(env: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let bind_address = match env("SFN_LOCAL_BIND") {
            Some(text) if text.trim().is_empty() => {
                return Err("SFN_LOCAL_BIND: 空です".to_string());
            }
            Some(text) => text,
            None => DEFAULT_BIND_ADDRESS.to_string(),
        };

        Ok(Self { bind_address })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 未設定なら既定の待ち受けアドレスを使う() {
        let config = Config::from_lookup(|_| None).unwrap();
        assert_eq!(config.bind_address, DEFAULT_BIND_ADDRESS);
    }

    #[test]
    fn 待ち受けアドレスを環境変数で変えられる() {
        let config =
            Config::from_lookup(|key| (key == "SFN_LOCAL_BIND").then(|| "127.0.0.1:9000".into()))
                .unwrap();
        assert_eq!(config.bind_address, "127.0.0.1:9000");
    }

    #[test]
    fn 空の待ち受けアドレスは起動時に止める() {
        let error = Config::from_lookup(|_| Some(" ".into())).err().unwrap();
        assert_eq!(error, "SFN_LOCAL_BIND: 空です");
    }
}
