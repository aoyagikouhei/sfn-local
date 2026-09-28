//! Step Functions API（awsJson1.0）を受けて、ASL の定義を実行するサーバ。
//! バイナリは main.rs、組み立ては router() に置いてテストから使えるようにしている。

mod arn;
mod asl;
pub mod config;
mod engine;
mod error;
mod handler;
mod loader;
mod operation;
mod request;
mod response;
mod store;
mod time;

use std::sync::Arc;

use axum::Router;
use axum::routing::post;

use crate::config::Config;
use crate::engine::lambda::Lambda;
use crate::handler::App;
use crate::store::Store;

/// Step Functions API を受けるルータと、起動時に読み込んだステートマシンの ARN（名前順）。
/// awsJson1.0 なのでパスは / だけ。定義を 1 つでも読めなければ理由を返す。
pub fn router(config: Config) -> Result<(Router, Vec<String>), String> {
    let store = Arc::new(Store::default());
    let loaded = match &config.state_machines_dir {
        Some(dir) => loader::load(dir, &config, &store)?,
        None => Vec::new(),
    };
    let app = App {
        lambda: Arc::new(Lambda::new(config.lambda_endpoint.clone())),
        config: Arc::new(config),
        store,
    };

    let router = Router::new()
        .route("/", post(handler::dispatch))
        .with_state(app);
    Ok((router, loaded))
}
