//! 起動時の設定エラーの表示。バイナリを実際に起動して標準エラーを見る。

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
