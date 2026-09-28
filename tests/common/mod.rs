//! 結合テストの足場。本物の `sfn_local::router` を同じプロセスに立て、素の HTTP で叩く。

use std::net::SocketAddr;

use serde_json::Value;
use sfn_local::config::Config;
use tokio::net::TcpListener;

pub struct Harness {
    addr: SocketAddr,
    client: reqwest::Client,
}

/// `post` の応答。本文は JSON として読んでおく。
pub struct Reply {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Value,
}

impl Harness {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("待ち受けできる");
        let addr = listener.local_addr().expect("アドレスを取れる");
        let config = Config {
            bind_address: addr.to_string(),
        };
        tokio::spawn(async move {
            axum::serve(listener, sfn_local::router(config))
                .await
                .expect("サーバが動き続ける");
        });

        Self {
            addr,
            client: reqwest::Client::new(),
        }
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
}
