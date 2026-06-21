# marketfeed-rs

多数据源日线行情采集、规则信号与 Markdown 日报（Rust）。

## 功能

- 数据源：Stooq、东方财富（股票 K 线 / 基金净值）、Alpha Vantage
- 命令：`init`、`bootstrap`、`update`、`run-daily`、`signal`、`report`
- 可选邮件发送日报（SMTP）

## 快速开始

```bash
cp config.example.toml config.toml
# 编辑 config.toml：标的、API Key、邮件等

cargo build --release
cargo run -- init
cargo run -- bootstrap
cargo run -- run-daily
```

## 远程部署（Oracle Linux / opc）

```bash
./scripts/build-opc.sh
scp marketfeed-deploy/marketfeed opc@你的主机:~/marketfeed-deploy/
```

**编译说明（本机 release vs opc 静态、常见 GLIBC 报错）** → [doc/build.md](doc/build.md)

部署与 cron → [doc/README.md](doc/README.md)。

## 配置说明

| 文件 | 说明 |
|------|------|
| `config.example.toml` | 配置模板（可提交仓库） |
| `config.toml` | 本地配置（**勿提交**，已在 .gitignore） |

敏感项建议用环境变量：`ALPHA_VANTAGE_API_KEY`、`STOOQ_API_KEY`、`MARKETFEED_SMTP_USER`、`MARKETFEED_SMTP_PASS`。

## 历史记录

- 2026-06-21：Stooq 对上证指数 `^shc` 返回浏览器验证页，已不适合作为上证指数来源；配置中将 `stooq_enabled` 设为 `false`，上证指数使用东方财富 `eastmoney`（`eastmoney_secid = "1.000001"`）。

## 文档

- [doc/README.md](doc/README.md) — 文档索引与部署
- [doc/build.md](doc/build.md) — 编译与 opc 上传
- [doc/providers/](doc/providers/) — 数据源说明

## License

MIT（如未另行说明）


本番部署：
Oracle Osaka2