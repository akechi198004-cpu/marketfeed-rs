# 编译说明

## 两种编译，别混用

| 场景 | 命令 | 产物路径 | 能否拷到 opc |
|------|------|----------|--------------|
| **本机开发、调试** | `cargo build --release` | `target/release/marketfeed` | **不能** |
| **Oracle Linux / opc 部署** | `./scripts/build-opc.sh` | `marketfeed-deploy/marketfeed` | **能** |

opc 上的 glibc 较旧。本机 `cargo build --release` 会链接**你这台 Ubuntu 的新 glibc**，拷过去运行会报：

```text
./marketfeed: /lib64/libc.so.6: version `GLIBC_2.30' not found ...
```

远程要用 **musl 静态链接**，不依赖远端 glibc。

## 日常：本地编好，直接 scp 到 opc（推荐）

```bash
# 开发机（项目根目录）
./scripts/build-opc.sh

scp marketfeed-deploy/marketfeed opc@你的主机:~/marketfeed-deploy/
```

等价手敲：

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
cp target/x86_64-unknown-linux-musl/release/marketfeed marketfeed-deploy/marketfeed
chmod +x marketfeed-deploy/marketfeed
```

**只复制这一个文件**即可更新程序；不要覆盖远程已有的 `config.toml`、`marketfeed.sqlite`、`reports/`。

## 打完整部署包（含 run-daily.sh、示例配置）

```bash
./scripts/build-deploy-package.sh
# 生成根目录 marketfeed-deploy.tar.gz
```

解压时注意不要用包里的 `config.toml` 覆盖远程已配好的邮件/API 配置。

## 开发机依赖（首次）

```bash
sudo apt install musl-tools    # 提供 musl-gcc
rustup target add x86_64-unknown-linux-musl
```

项目已配置 `.cargo/config.toml`，musl 目标自动用 `musl-gcc` 链接。

## 复制前 / 复制后自检

**开发机：**

```bash
file marketfeed-deploy/marketfeed
# 期望: static-pie linked, statically linked

ldd marketfeed-deploy/marketfeed
# 期望: not a dynamic executable
```

**opc 上（scp 之后）：**

```bash
cd ~/marketfeed-deploy
file ./marketfeed
ldd ./marketfeed
chmod +x marketfeed
./marketfeed --help
```

若 `ldd` 仍列出 `libc.so.6`，说明传错了文件（常见误传 `target/release/marketfeed`）。

## 常见误区

| 误区 | 说明 |
|------|------|
| `cargo build --release` 后拷 `target/release/marketfeed` | 仅适合本机或 glibc **不低于** 编译机的主机 |
| 以为 `marketfeed-deploy/marketfeed` 一定是静态的 | 只有跑过 `build-opc.sh` 或 `build-deploy-package.sh` 才是 musl 版 |
| 整包解压覆盖远程目录 | 会盖掉远程 `config.toml` 和数据库 |
## 相关脚本

| 脚本 | 用途 |
|------|------|
| `scripts/build-opc.sh` | musl 编译 → `marketfeed-deploy/marketfeed`，打印 scp 提示 |
| `scripts/build-deploy-package.sh` | 调用 `build-opc.sh` 并打 `marketfeed-deploy.tar.gz` |
| `scripts/deploy-run-daily.sh` | 部署目录内 `run-daily.sh` 模板（远程无 Rust） |
| `run-daily.sh`（项目根） | 本机 `cargo run --release -- run-daily` |

部署与 cron 见 [README.md](./README.md)。
