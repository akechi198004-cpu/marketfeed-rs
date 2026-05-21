#!/usr/bin/env bash
# 生成 marketfeed-deploy.tar.gz（musl 静态链接，兼容 Oracle Linux 等旧 glibc）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DEPLOY_DIR="$ROOT/marketfeed-deploy"
ARCHIVE="$ROOT/marketfeed-deploy.tar.gz"
BINARY="$DEPLOY_DIR/marketfeed"

if ! command -v musl-gcc >/dev/null 2>&1; then
  echo "错误: 未找到 musl-gcc，请安装: sudo apt install musl-tools" >&2
  exit 1
fi

echo "==> musl 静态编译 ..."
rustup target add x86_64-unknown-linux-musl >/dev/null 2>&1 || true
cargo build --release --target x86_64-unknown-linux-musl -q

rm -rf "$DEPLOY_DIR"
mkdir -p "$DEPLOY_DIR/reports"
cp -f target/x86_64-unknown-linux-musl/release/marketfeed "$BINARY"

echo "==> 二进制:"
file "$BINARY"
ldd "$BINARY" 2>&1 || true

cp -f "$ROOT/config.example.toml" "$DEPLOY_DIR/config.toml"
cp -f "$ROOT/scripts/deploy-run-daily.sh" "$DEPLOY_DIR/run-daily.sh"
cp -f "$ROOT/scripts/deploy-README.txt" "$DEPLOY_DIR/README.txt"

chmod +x "$BINARY" "$DEPLOY_DIR/run-daily.sh"
tar -czf "$ARCHIVE" -C "$ROOT" marketfeed-deploy
ls -lh "$ARCHIVE"
echo "完成: $ARCHIVE"
echo "注意: 包内为 config.example.toml，部署到远程后请复制为 config.toml 并填写密钥"
