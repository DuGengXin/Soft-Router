# 网关模式必须启用 WG；数据面健康可见

家用路径是境外走 VPS 隧道：网关模式若未启用 WireGuard，sing-box 仍 `final: wg-out`，境外会黑洞。校验改为拒绝该组合。观察模式确认应用仍不改网络；UI 会提示先切网关。`GET /api/v1/status` 附带数据面健康；WG 无握手只记 note，不判失败、不触发旁路。
