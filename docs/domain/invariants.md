# 不变量

1. Rust 不实现 TCP/IP 或代理协议栈；只生成并应用数据面配置。
2. 只管理 nft 表 `gateway_kit` 与策略路由表 `51820`，以及本产品生成的文件与单元。
3. 安装与观察模式不得改主默认路由、不得 NAT、不得拉起代理。
4. 变更顺序：discover → plan → 确认（或开机恢复已成功代际）→ apply → health；失败回滚。
5. 原始密钥只在 `secrets.toml`（0600），不进普通配置与日志；由其渲染出的含凭证运行时文件也必须是 0600。
6. Web 默认 bind `0.0.0.0`；API 使用管理员用户名/密码登录和进程内短期 HttpOnly 会话，密码仅以 Argon2 哈希存入 `secrets.toml`。生成的 `gateway_kit` input 链允许配置的 LAN、WAN 与 loopback 网卡访问 UI 端口，其它接口仍拒绝；不接管其它防火墙表。HTTP 不加密链路，公网管理需使用 HTTPS 反向代理或 VPN。
7. 本机 SSH、UI、agent 流量默认不进代理。
8. 解析/渲染/校验必须可注入命令，在无 root 环境单测。
