# 境外 DNS 改用 DoH，避免丢包线路上解析超时

原因：境外域名解析原先经代理出口以 UDP 发往国内解析器（`223.5.5.5`），
这条隧道实测丢包 10%、RTT 47–98 ms。UDP 查询一旦丢包只能等超时，
实测出现大量 `dns: exchange failed ... context deadline exceeded`（10 s），
表现为页面长时间转圈。

变更：

- 新增 `[dns]` 配置节：`overseas`（接受 `ip`、`ip:port`、`tls://host`、
  `https://host/path`）与 `overseas_server_name`（IP 形式 DoH 的 SNI 覆盖）。
- 渲染器把 `proxy-dns` 从“同一个 UDP 解析器经代理”改为可配置的 DoH/DoT；
  默认 `https://1.1.1.1/dns-query`，走 `proxy-out`。DoH 基于 TCP，丢包时重传
  而不是直接超时。
- `dns.final` 从 `proxy-dns` 改为 `direct-dns-0`：内部解析（如代理服务器
  域名）本就走直连解析器，不再绕道境外。
- 境内域名仍由 `direct-dns-0` 直连 UDP 解析，行为不变。

验证（2026-10-19，192.111.40.33）：`sing-box check` 通过；`gateway-kit apply
--confirm` 生成 g1792391767-605934703 且无回滚；四单元 active；经隧道 DoH
实测 Cloudflare 端点 0.68 s 返回正确 A 记录；LAN 侧 `www.google.com` 443 ms、
`github.com` 119 ms 均为 NOERROR，直连的 `www.baidu.com` 7 ms 不变。
