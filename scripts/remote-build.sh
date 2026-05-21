#!/usr/bin/env bash
# 在远程 Oracle Linux / 旧 glibc 机器上，于部署目录内从源码编译（需先上传源码或 git clone）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v cargo >/dev/null 2>&1; then
  echo "==> 安装 Rust (rustup)..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

echo "==> cargo build --release"
cargo build --release

cp -f target/release/marketfeed "$ROOT/marketfeed-deploy/marketfeed" 2>/dev/null \
  || cp -f target/release/marketfeed "$ROOT/marketfeed"
chmod +x "$ROOT/marketfeed-deploy/marketfeed" 2>/dev/null || chmod +x "$ROOT/marketfeed"

echo "==> 完成"
./marketfeed-deploy/marketfeed --help 2>/dev/null || ./marketfeed --help 2>/dev/null || true
ldd ./marketfeed-deploy/marketfeed 2>/dev/null || ldd ./marketfeed 2>/dev/null || true
