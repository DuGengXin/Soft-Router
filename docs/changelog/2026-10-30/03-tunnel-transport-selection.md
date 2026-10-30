# 出口方式改为显式单选，GeoFiles 定期刷新

隧道页不再假设 WireGuard 是默认出口。页面顶部改为「不启用 / WireGuard / VLESS 链接」单选：选 VLESS 时只显示链接输入框，选 WireGuard 时才显示 WG 字段，保存时把另一方的凭证清空，避免同时存在两种境外出口。

分流页切换运行模式时不再强制打开 `wireguard.enabled`。此前该行为会让只用 VLESS 的用户在保存时触发后端 `choose exactly one overseas transport` 校验失败。

概览与 `/api/v1/status` 改用 `tunnel.transport`（`none` / `wireguard` / `vless`）描述出口，由 `AppConfig::tunnel_transport` 从配置和 `secrets.toml` 推导；VLESS 模式下隧道卡片不再显示 "WireGuard handshake"，`tunnel_uplink` 也不会把未启用的 WireGuard 误报为已关闭的隧道。

生成的 sing-box 远程规则集显式设置 `update_interval: "1d"`，与 sing-box 在启用 `cache_file` 时的默认行为一致，便于在配置层面确认 GeoIP/GeoSite 会定期刷新。

本次仅修改项目代码与文档，未部署到网关。
