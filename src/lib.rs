//! Step Functions API（awsJson1.0）を受けるサーバ。
//! バイナリは main.rs、組み立ては router() に置いてテストから使えるようにしている。

pub mod config;
mod handler;
mod operation;
mod response;

use std::sync::Arc;

use axum::Router;
use axum::routing::post;

use crate::config::Config;
use crate::handler::App;

/// Step Functions API を受けるルータ。awsJson1.0 なのでパスは / だけ。
pub fn router(config: Config) -> Router {
    let app = App {
        config: Arc::new(config),
    };

    Router::new()
        .route("/", post(handler::dispatch))
        .with_state(app)
}
