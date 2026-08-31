# 2026-08-31：旧版 Router 迁移保护

- discover 只读检测 `/root/work/soft-router`、`gateway-firewall.service` 和旧 `inet router` 表。
- Gateway 模式发现旧版安装时阻止 apply，避免 Rust 版本与旧 Python 版本争抢 nft、策略路由、sing-box 和 DHCP。
- 不自动删除旧项目、不 flush 全局 nftables；迁移清理必须先备份并显式执行。
