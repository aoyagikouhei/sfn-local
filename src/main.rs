use std::error::Error;

use sfn_local::config::Config;
use tokio::net::TcpListener;

/// ローカル用の Step Functions 代役。
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 設定エラーは `String` なので、`?` で `main` の出口に渡すと Rust の既定のハンドラが
    // `Error: "..."` と Debug 形（引用符とエスケープ付き）で表示する。理由を平文の 1 行で出して止める。
    let exit = |reason: String| -> ! {
        eprintln!("{reason}");
        std::process::exit(1);
    };
    let config = Config::from_env().unwrap_or_else(|reason| exit(reason));
    let summary = format!(
        "sfn-local listening on {} (region: {}, account: {}, lambda endpoint: {})",
        config.bind_address,
        config.region,
        config.account_id,
        config.lambda_endpoint.as_deref().unwrap_or("-")
    );
    let listener = TcpListener::bind(&config.bind_address).await?;
    let (router, loaded) = sfn_local::router(config).unwrap_or_else(|reason| exit(reason));

    println!("{summary}");
    for arn in loaded {
        println!("state machine loaded: {arn}");
    }

    axum::serve(listener, router).await?;

    Ok(())
}
