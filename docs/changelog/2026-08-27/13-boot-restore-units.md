# 开机恢复函数化与生产单元 enable

原因：确认 apply 后 sing-box/dnsmasq 若未 enable，仅靠 agent 重启；开机恢复逻辑只在 agent 内，单测覆盖不到。`--local` 不得 enable 宿主机单元。  
意图：生产路径（`/etc/gateway-kit/config.toml`）apply 时 `systemctl enable` 数据面单元，disable 时 `disable --now`；抽出 `restore_on_boot`；netns 在拆掉 nft/WG 后跑 `agent --once` 再验 LAN 与 WG TCP。仍不是物理机重启。
