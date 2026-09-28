use std::error::Error;

use sfn_local::config::Config;
use tokio::net::TcpListener;

/// ローカル用の Step Functions 代役。
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 設定エラーは `String` なので、`?` で `main` の出口に渡すと Rust の既定のハンドラが
    // `Error: "..."` と Debug 形（引用符とエスケープ付き）で表示する。理由を平文の 1 行で出して止める。
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(reason) => {
            eprintln!("{reason}");
            std::process::exit(1);
        }
    };
    let listener = TcpListener::bind(&config.bind_address).await?;

    println!("sfn-local listening on {}", config.bind_address);

    axum::serve(listener, sfn_local::router(config)).await?;

    Ok(())
}
