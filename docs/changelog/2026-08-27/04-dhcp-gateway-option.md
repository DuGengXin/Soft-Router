# DHCP 通告本机为网关

原因：工作 PC 经 AP 拿地址后必须把 Linux LAN 当作默认网关，否则分流与 NAT 不会发生。  
意图：dnsmasq 写入 `dhcp-option=3` 与 `listen-address`；健康检查在启用 WG 时确认网卡存在；向导可改 UI bind。
