//! 起動時の設定エラーの表示。バイナリを実際に起動して標準エラーを見る。

mod common;

use std::process::Command;

#[test]
fn 不正な環境変数で止まるときは理由を平文で標準エラーに出す() {
    let output = Command::new(env!("CARGO_BIN_EXE_sfn-local"))
        .env("SFN_LOCAL_BIND", "")
        .output()
        .expect("バイナリを起動できる");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "SFN_LOCAL_BIND: 空です\n"
    );
}

#[test]
fn 読めない定義があれば起動を止める() {
    let dir = common::temp_dir("broken-definition");
    let file = dir.join("broken.asl.json");
    std::fs::write(
        &file,
        r#"{"StartAt": "A", "States": {"A": {"Type": "Wait", "Seconds": 1, "End": true}}}"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_sfn-local"))
        .env("SFN_LOCAL_BIND", "127.0.0.1:0")
        .env("SFN_LOCAL_STATE_MACHINES_DIR", &dir)
        .output()
        .expect("バイナリを起動できる");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "SFN_LOCAL_STATE_MACHINES_DIR: {}: {}: UNSUPPORTED_BY_SFN_LOCAL: the state type 'Wait' is not supported by sfn-local yet at /States/A\n",
            dir.display(),
            file.display()
        )
    );
}
