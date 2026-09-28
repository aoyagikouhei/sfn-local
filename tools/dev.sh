#!/usr/bin/env bash
# 開発の足場（cargo など）を toolbox コンテナ（compose.yml の dev サービス）の中で動かす。
#
# 使い方（リポジトリの中のどこからでも。cwd はそのままコンテナに引き継ぐ）:
#   tools/dev.sh cargo test
#   tools/dev.sh RUST_LOG=debug cargo run               # 環境変数は VAR=VALUE をコマンドの前に並べる（env に渡す）
#   tools/dev.sh bash -c 'cargo test 2>&1 | tail -n 5'  # パイプやリダイレクトをコンテナの中でするときは bash -c
#
# コンテナの中の HOME・CARGO_HOME・CARGO_TARGET_DIR はリポジトリの .toolbox/ の下（home / cargo / target）。
# ホストの ~/.cargo と target/ とは混ぜない。消すときは rm -rf .toolbox（次の実行で作り直される）。
# イメージのタグは tools/toolbox/Dockerfile の sha256 から決めるので、Dockerfile を変えると次の実行で作り直される。
set -euo pipefail

die() {
  echo "dev.sh: $1" >&2
  exit 2
}

main() {
  if [ $# -eq 0 ]; then
    die "使い方: tools/dev.sh [VAR=VALUE ...] <コマンド> [引数...]"
  fi

  local repo cwd
  repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
  cwd=$(pwd -P)
  # リポジトリの外はコンテナにマウントしていないので、そこを cwd にはできない。
  case "$cwd/" in
    "$repo/"*) ;;
    *) die "リポジトリの外（$cwd）では動かせない。$repo の中で実行する" ;;
  esac

  # 代入と export を分ける（export VAR=$(...) だと置換の失敗を set -e が拾わない）。
  TOOLBOX_TAG=$(sha256sum "$repo/tools/toolbox/Dockerfile" | cut -c1-12)
  DEV_REPO=$repo
  DEV_CWD=$cwd
  DEV_UID=$(id -u)
  DEV_GID=$(id -g)
  DEV_USER=$(id -un)
  export DEV_REPO DEV_CWD DEV_UID DEV_GID DEV_USER TOOLBOX_TAG

  # HOME だけ先に作る（cargo と target は cargo が自分で作る）。
  mkdir -p "$repo/.toolbox/home"

  exec docker compose -f "$repo/compose.yml" run --rm dev env -- "$@"
}

main "$@"
