#!/usr/bin/env bash
# 生成 marketfeed-deploy.tar.gz（优先 musl 静态链接，兼容 Oracle Linux 7/8 等旧 glibc）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DEPLOY_DIR="$ROOT/marketfeed-deploy"
ARCHIVE="$ROOT/marketfeed-deploy.tar.gz"
BINARY="$DEPLOY_DIR/marketfeed"

build_musl_local() {
  if ! command -v musl-gcc >/dev/null 2>&1; then
    return 1
  fi
  echo "==> 本地 musl 编译 ..."
  rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1 || true
  cargo build --release --target x86_64-unknown-linux-musl -q
  cp -f target/x86_64-unknown-linux-musl/release/marketfeed "$BINARY"
  return 0
}

build_musl_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    return 1
  fi
  echo "==> Docker musl 编译（兼容旧 glibc 远程机）..."
  docker build -f Dockerfile.deploy --target builder -t marketfeed-musl-build .
  cid=$(docker create marketfeed-musl-build)
  docker cp "$cid:/marketfeed" "$BINARY"
  docker rm "$cid" >/dev/null
  return 0
}

build_gnu_local() {
  echo "==> 本地 gnu 编译（仅适用于较新 glibc 的 Linux）..."
  cargo build --release -q
  cp -f target/release/marketfeed "$BINARY"
}

rm -rf "$DEPLOY_DIR"
mkdir -p "$DEPLOY_DIR/reports"

if build_musl_local || build_musl_docker; then
  echo "==> 已使用 musl 静态二进制"
  file "$BINARY"
  ldd "$BINARY" 2>&1 || true
else
  echo "警告: musl 编译失败，回退 gnu 二进制（旧系统可能报 GLIBC_2.xx not found）" >&2
  echo "  请安装 musl-tools: sudo apt install musl-tools" >&2
  echo "  或安装 Docker 后重新运行本脚本" >&2
  build_gnu_local
fi

cp -f "$ROOT/config.toml" "$DEPLOY_DIR/"
cp -f "$ROOT/scripts/deploy-run-daily.sh" "$DEPLOY_DIR/run-daily.sh"
cp -f "$ROOT/scripts/deploy-README.txt" "$DEPLOY_DIR/README.txt"

chmod +x "$BINARY" "$DEPLOY_DIR/run-daily.sh"
tar -czf "$ARCHIVE" -C "$ROOT" marketfeed-deploy
ls -lh "$ARCHIVE"
echo "完成: $ARCHIVE"
