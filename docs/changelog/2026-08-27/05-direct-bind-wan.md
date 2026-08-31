# 国内直连绑定 WAN

原因：sing-box 若 auto-detect，直连可能误走 WireGuard，国内流量会进隧道。  
意图：`direct` 绑定 WAN 网卡，`wg-out` 绑定 WG；关掉 auto-detect。iptables-legacy 与 WAN/LAN 同卡视为 apply 阻断。
