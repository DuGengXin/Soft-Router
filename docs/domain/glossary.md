# 术语

| 词 | 含义 |
|---|---|
| 控制面 | Rust：发现、校验、渲染、plan/apply/回滚、健康、UI |
| 数据面 | 内核转发、nftables、WireGuard、sing-box、按需运行的 Xray-core、DHCP 守护进程 |
| 观察模式 | 只发现与提供 UI，不改路由/NAT/隧道 |
| 网关模式 | 用户确认后管理 LAN/WAN 转发与分流 |
| 代际 | 一次已确认 apply 的配置快照，可回滚 |
| 旁路 | 撤走本产品 nft/策略路由/本产品拉起的数据面单元 |
| 直连 | 经 WAN 与路由器 A 出网，不进隧道 |
| 隧道 | 境外出口三选一（运行时互斥，凭证可分槽并存）：WireGuard 至用户 VPS；Xray-core VLESS 至用户 VPS（sing-box → 本机 SOCKS5）；或 SOCKS5 中继（sing-box 直连远端，如经 EasyTier 到家中） |
