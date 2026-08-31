# 2026-08-31：sing-box DNS 接管与 Docker 转发兼容

原因：仅让 LAN 客户端把 DNS 指向网关，仍可能由 dnsmasq 直接访问上游 DNS，无法获得类似 v2rayN TUN 的 DNS 代理语义；Docker 或主机防火墙设置 FORWARD DROP 时，也可能阻断 LAN 出口。

变更：

- sing-box 增加本地 DNS 入站（127.0.0.1:5353），DNS 默认经当前海外出口转发；代理服务器域名解析使用独立的直连 bootstrap DNS。
- dnsmasq 只监听 LAN、只转发到本机 sing-box，并继续通过 DHCP 下发网关地址作为 DNS。
- nftables 对 LAN 客户端发往任意目标的 TCP/UDP 53 做非破坏性 DNAT 到网关 DNS，避免手动指定外部 DNS 绕过策略。
- sing-box DNS 使用 `ipv4_only`，避免当前 IPv4 TProxy 架构出现 IPv6 旁路。
- 新增带 `gateway-kit` 标记的 Docker `DOCKER-USER`/`FORWARD` 兼容单元；只添加/删除本产品自己的接口限定规则，不刷新或接管 Docker/UFW/firewalld 规则。
- 健康检查要求 TProxy、sing-box DNS 和转发兼容单元在生产模式可用。

意图：让 LAN 侧行为接近 sing-box TUN 客户端，同时保持 Gateway-Kit 只管理自身生成对象、可回滚、可与宿主机已有网络栈共存。
