# 网络子菜单与端口映射

原因：向导过长；工作电脑需访问上级 LAN；A 侧用 DNAT 访问网关 LAN。  
意图：侧栏「网络」拆接口/分流/隧道/接入设备/端口映射；WAN 前缀强制直连+MASQUERADE；nft DNAT；WG 粘贴/base64 解析。限速与整段 DMZ 留后期。
