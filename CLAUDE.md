# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 概要

AWS Step Functions API（`awsJson1.0`）の汎用のローカル代役。ASL の定義を自前で解釈して実行し、Task の Lambda は Invoke API 互換のエンドポイント（cargo lambda watch、LocalStack など）に投げる。構成は姉妹プロジェクトの athena-local（`../athena-local`）に倣う。利用者向けの仕様は `docs/`、README.md は概要とクイックスタートだけ。

## コマンド

既定は toolbox（`tools/dev.sh` 経由。compose.yml の dev サービスの中で動く）。ホストに rust があればホスト直でも動く（`target/` は toolbox の `.toolbox/target` と別）。

```bash
tools/dev.sh cargo run                       # 既定の待ち受けは 0.0.0.0:8083。環境変数は docs/configuration.md
tools/dev.sh cargo test                      # AWS も Lambda も要らない（テスト内で偽の Lambda を立てる）
tools/dev.sh cargo test --test dispatch      # 結合テストを 1 ファイルだけ（tests/<名前>.rs）
tools/dev.sh cargo test --lib config::tests  # src 内のユニットテストだけ
tools/dev.sh cargo fmt --check
tools/dev.sh cargo clippy --all-targets --locked -- -D warnings
docker build -t sfn-local:dev .
```

CI（`.github/workflows/ci.yml`）は fmt・clippy・test。

## アーキテクチャ

- `handler.rs` — `POST /` の 1 本だけ。`X-Amz-Target: AWSStepFunctions.<Operation>` で振り分ける。SigV4 は検証しない。
- `operation/` — オペレーションの本体（`state_machine.rs`・`execution.rs`）。まだ無いものは `mod.rs` のスタブが `NotImplemented` を返す。
- `asl/` — 定義の読み込み（`definition.rs`）と JSONPath の部分集合・テンプレート（`path.rs`）。実行できない定義は読み込みで `UNSUPPORTED_BY_SFN_LOCAL:` として弾く。
- `engine/` — 実行エンジン（`mod.rs`。実行ごとに 1 つの tokio タスクでステートをたどる）と Lambda の Invoke API の呼び出し（`lambda.rs`）。
- `store.rs`（ステートマシンと実行をメモリに持つ）、`loader.rs`（起動時の `SFN_LOCAL_STATE_MACHINES_DIR` の読み込み）、`arn.rs`（ARN と名前の規則）、`error.rs`（`__type` の名前空間と文言）、`request.rs`／`response.rs`（awsJson1.0 の本文）、`time.rs`、`config.rs`。

壊すと事故になる不変条件:

- 実行できない定義は読み込み（起動時と CreateStateMachine）で弾き、実行の途中で「未対応」にしない。
- `States.Runtime` は Retry・Catch で拾わない（ASL の仕様）。
- エンジンは Lambda の応答の本文を、2xx でヘッダが無いときは覗かない（`{"error": ...}` を返しても成功のまま）。

## テスト

結合テストは `tests/common/mod.rs` の `Harness`（本物の `sfn_local::router` を同じプロセスに立てる）と `FakeLambda`（偽の Invoke API。応答を関数で決め、呼び出しを記録する）を使う。

- `config.rs` のテストは本物の環境変数を触らない（テストが並列に走るため）。`Config::from_lookup` に環境変数を読むクロージャを渡して差し替える。
- ユニットテストはインラインの `#[cfg(test)] mod tests` に置く。

## ドキュメントの置き場

| 置き場 | 書くこと | 言語 |
|---|---|---|
| `README.md` | 概要、クイックスタート、docs へのリンク | 英語 |
| `docs/*.md` | sfn-local の現在の挙動（利用者向け） | 英語 |
| `docs/dev/` | 内部の地図、実測の結果、設計判断 | 日本語 |
| `CHANGELOGS.md` | 版の間で何が変わったか。1 項目 1〜2 行 | 英語 |

## 開発上の約束

- **汎用のローカル代役にする。** 特定の利用者の都合で結果を強制する仕組み（状態を固定するモード、テスト用の制御エンドポイント、独自のエラー名）は入れない。結果は ASL を解釈して自然に出す。
- **本物の Step Functions に合わせることが目的**。文言、エラーコード、応答の形は本番で実測した値に合わせ、コメントに「2026-09-28 実測」のように書く。実測していない振る舞いは推測で埋めず、未実測であることをコメントと docs に書く。
- 挙動を変えたら `docs/`（対応オペレーションは `api.md`）と CHANGELOGS.md の `[Unreleased]` も更新する。
- コメント、テスト名（日本語の文）、エラーメッセージ、コミットメッセージ（「〜する」で終わる一行）、PR の本文は日本語。README・`docs/*.md`・CHANGELOG は英語、`docs/dev/` は日本語。
- **ユーザーへの返答は常に日本語で書く。**
- Rust edition 2024。Docker のビルドイメージは `rust:1.98`。toolbox（`tools/toolbox/Dockerfile`）も同じタグ。変えるときは両方。
