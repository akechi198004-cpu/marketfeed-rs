marketfeed-rs 远程部署包
========================

目录内容
--------
  marketfeed      主程序（musl 静态编译，兼容旧 glibc / Oracle Linux）
  run-daily.sh    每日：更新数据 + 信号 + 报告（+ 邮件若已启用）
  config.toml     配置文件
  reports/        报告输出目录

使用前
------
  chmod +x marketfeed run-daily.sh
  ldd ./marketfeed
  # 应显示 "not a dynamic executable" 或 statically linked

  export MARKETFEED_SMTP_USER='发件邮箱'   # 邮件可选
  export MARKETFEED_SMTP_PASS='SMTP授权码'

首次（无 marketfeed.sqlite）
---------------------------
  ./marketfeed init
  ./marketfeed bootstrap          # 较久，仅需一次
  ./run-daily.sh

日常
----
  ./run-daily.sh

邮件
----
  config.toml 中 [report.email] enabled = true 并填写 from / to / smtp_host
