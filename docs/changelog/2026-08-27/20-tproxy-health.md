# 生产 apply 要求 tproxy 在听

原因：systemd 重启 sing-box 失败时 apply 仍记 success，LAN 境外流量进 tproxy 黑洞。  
意图：仅 `/etc/gateway-kit/config.toml` 路径等待并检查 `ss` 含 `:7895`；`--local`/netns 不要求，避免 CI 在 apply 之后才拉 sing-box。
