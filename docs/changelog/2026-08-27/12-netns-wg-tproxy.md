# netns 境外 TCP 经 WG + AllowedIPs 主表路由

原因：仅 LAN ping WAN 不能证明境外路径；`Table = off` 时非 default 的 AllowedIPs 若不写主表，bind wg0 仍可能走 WAN default。sing-box unit 写死 `/usr/bin/sing-box` 会错过 `/usr/local/bin`。  
意图：apply 对非 `0.0.0.0/0` 的 AllowedIPs 做 `ip route replace … dev wg0`（仍禁止主表 default）；unit 用 PATH 找 sing-box；CI netns 增加 VPS 对端 + tproxy TCP 到 203.0.113.1。仍不是真机重启验收。
