# marketfeed-rs 文档

## 文档索引

| 文档 | 说明 |
|------|------|
| [providers/README.md](./providers/README.md) | 数据代理库总览 |
| [providers/architecture.md](./providers/architecture.md) | 架构与错误模型 |
| [providers/stooq.md](./providers/stooq.md) | Stooq |
| [providers/eastmoney.md](./providers/eastmoney.md) | 东方财富 |
| [providers/alpha_vantage.md](./providers/alpha_vantage.md) | Alpha Vantage |
| [providers/configuration.md](./providers/configuration.md) | 配置说明 |
| [providers/integration.md](./providers/integration.md) | 调用流程 |

## 本地开发

```bash
cargo build --release
cargo run -- run-daily
```

## 远程部署（Oracle Linux 等）

**1. 开发机打静态包（推荐）**

```bash
./scripts/build-deploy-package.sh
```

生成根目录 `marketfeed-deploy.tar.gz`（musl 静态链接，不依赖远程 glibc）。

**2. 上传到远程**

```bash
scp marketfeed-deploy.tar.gz opc@你的主机:~/
```

**3. 远程解压运行**

```bash
tar -xzf marketfeed-deploy.tar.gz
cd marketfeed-deploy
chmod +x marketfeed run-daily.sh
ldd ./marketfeed   # 应显示 not a dynamic executable

./marketfeed init
./marketfeed bootstrap   # 首次拉历史，较久
./run-daily.sh
```

邮件：在 `config.toml` 的 `[report.email]` 设 `enabled = true`，密码用环境变量 `MARKETFEED_SMTP_USER` / `MARKETFEED_SMTP_PASS`。正文为 HTML（适合 Gmail），本地仍保存 `reports/*.md`；QQ 等常用 `smtp_port = 465`。

## 部署包内容

```
marketfeed-deploy/
├── marketfeed      # musl 静态二进制
├── run-daily.sh
├── config.toml
├── README.txt
└── reports/
```
