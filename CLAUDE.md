# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 概要

AWS Step Functions API（`awsJson1.0`）のローカル代役。構成は姉妹プロジェクトの athena-local（`../athena-local`）に倣う。今はがわだけで、どのオペレーションも `NotImplemented` を返す。利用者向けの仕様は `docs/`、README.md は概要とクイックスタートだけ。

## コマンド

既定は toolbox（`tools/dev.sh` 経由。compose.yml の dev サービスの中で動く）。ホストに rust があればホスト直でも動く（`target/` は toolbox の `.toolbox/target` と別）。

```bash
tools/dev.sh cargo run                       # 既定の待ち受けは 0.0.0.0:8083（SFN_LOCAL_BIND）
tools/dev.sh cargo test                      # AWS は要らない
tools/dev.sh cargo test --test dispatch      # 結合テストを 1 ファイルだけ（tests/<名前>.rs）
tools/dev.sh cargo test --lib config::tests  # src 内のユニットテストだけ
tools/dev.sh cargo fmt --check
tools/dev.sh cargo clippy --all-targets --locked -- -D warnings
docker build -t sfn-local:dev .
```

CI（`.github/workflows/ci.yml`）は fmt・clippy・test。

## アーキテクチャ

- `handler.rs` — `POST /` の 1 本だけ。`X-Amz-Target: AWSStepFunctions.<Operation>` で振り分ける。SigV4 は検証しない。
- `operation/` — オペレーションの本体。今は `mod.rs` のスタブだけ。実装するときはオペレーションごとにファイルを分け、`mod.rs` は `mod` 宣言と再エクスポートだけにする。
- `response.rs`（awsJson1.0 の本文とエラーの形）、`config.rs`（環境変数）。

## テスト

結合テストは `tests/common/mod.rs` の `Harness`（本物の `sfn_local::router` を同じプロセスに立てる）を使う。

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

- **本物の Step Functions に合わせることが目的**。文言、エラーコード、応答の形は本番で実測した値に合わせ、コメントに「2026-09-28 実測」のように書く。実測していない振る舞いは推測で埋めず、未実測であることをコメントと docs に書く。
- 挙動を変えたら `docs/`（対応オペレーションは `api.md`）と CHANGELOGS.md の `[Unreleased]` も更新する。
- コメント、テスト名（日本語の文）、エラーメッセージ、コミットメッセージ（「〜する」で終わる一行）、PR の本文は日本語。README・`docs/*.md`・CHANGELOG は英語、`docs/dev/` は日本語。
- **ユーザーへの返答は常に日本語で書く。**
- Rust edition 2024。Docker のビルドイメージは `rust:1.98`。toolbox（`tools/toolbox/Dockerfile`）も同じタグ。変えるときは両方。
