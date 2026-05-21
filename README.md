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

## 远程部署（Oracle Linux 等旧 glibc）

```bash
./scripts/build-deploy-package.sh
```

生成 `marketfeed-deploy.tar.gz`（musl 静态二进制）。上传解压后：

```bash
chmod +x marketfeed run-daily.sh
./marketfeed init && ./marketfeed bootstrap && ./run-daily.sh
```

详见 [doc/README.md](doc/README.md)。

## 配置说明

| 文件 | 说明 |
|------|------|
| `config.example.toml` | 配置模板（可提交仓库） |
| `config.toml` | 本地配置（**勿提交**，已在 .gitignore） |

敏感项建议用环境变量：`ALPHA_VANTAGE_API_KEY`、`STOOQ_API_KEY`、`MARKETFEED_SMTP_USER`、`MARKETFEED_SMTP_PASS`。

## 文档

- [doc/README.md](doc/README.md) — 文档索引与部署
- [doc/providers/](doc/providers/) — 数据源说明

## License

MIT（如未另行说明）
