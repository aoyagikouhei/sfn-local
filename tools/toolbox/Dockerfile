# 開発の足場（cargo など）を動かす実行環境。tools/dev.sh が compose.yml の dev サービスとして使う。
# tools/dev.sh がタグを Dockerfile の sha256 から決めるので、このファイルを変えると次の実行で作り直される。
# COPY するファイルを足すときは、tools/dev.sh のタグの計算にもそのファイルを入れる。

# ルートの Dockerfile の builder と同じタグにする。変えるときは両方。
FROM rust:1.98-bookworm

# 公式イメージは --profile minimal で clippy と rustfmt を持たない（CI も ci.yml で足している）。
RUN rustup component add clippy rustfmt

RUN apt-get update \
 && apt-get install -y --no-install-recommends jq curl ca-certificates \
 && rm -rf /var/lib/apt/lists/*
