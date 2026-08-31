# LAN 客户端 DNS 统一经网关

## 变更

- DHCP 不再把 WAN 上游 DNS 直接下发给 LAN 客户端，改为下发网关 LAN 地址（默认 192.168.50.1）。
- nftables 在透明代理前明确放行发往网关自身的 TCP/UDP 53，交由 Gateway-Kit dnsmasq 处理。
- 初版保留了 dnsmasq 的上游解析能力；该行为已由后续的 `19-singbox-dns-and-docker-forward.md` supersede：当前 dnsmasq 仅把请求交给本机 sing-box DNS 入站，国内/境外分流统一由 sing-box 负责。

## 原因

部分移动设备会缓存 DHCP 下发的外部 DNS，导致解析路径绕过网关，表现为国内站点可用但境外站点失败或解析不稳定。

## 验收

重新连接 LAN Wi-Fi 后，客户端 DNS 应为网关 LAN 地址；网关本机对该地址的 Google 域名查询成功，Gateway-Kit、sing-box、dnsmasq 和 Docker 服务均保持 active。
