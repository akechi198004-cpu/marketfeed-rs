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
| [signals.md](./signals.md) | 信号与「连续 N 日」说明 |
| [build.md](./build.md) | **编译**：本机 release vs opc musl 静态、scp 路径 |

## 本地开发

```bash
cargo build --release
cargo run -- run-daily
```

## 远程部署（Oracle Linux / opc）

编译与上传步骤见 **[build.md](./build.md)**（避免误用 `target/release/marketfeed`）。

简要：

```bash
./scripts/build-opc.sh
scp marketfeed-deploy/marketfeed opc@你的主机:~/marketfeed-deploy/
```

首次部署可打 tar 包：`./scripts/build-deploy-package.sh`，远程 `init` / `bootstrap` 后日常 `./run-daily.sh`。

邮件：在 `config.toml` 的 `[report.email]` 设 `enabled = true`，密码用环境变量 `MARKETFEED_SMTP_USER` / `MARKETFEED_SMTP_PASS`。正文为 HTML（适合 Gmail），含 **连续 N 日买入/卖出**（由历史信号自动统计，见 [signals.md](./signals.md)）；QQ 等常用 `smtp_port = 465`。

## 部署包内容

```
marketfeed-deploy/
├── marketfeed      # musl 静态二进制
├── run-daily.sh
├── config.toml
├── README.txt
└── reports/
```
