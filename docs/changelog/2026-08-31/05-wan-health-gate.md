# 生产 WAN 健康门禁

原因：生产模式下即使配置的上级 WAN 网关不可达，apply 仍可能只因 nft/策略规则存在而判定成功。

意图：生产健康检查将配置的 WAN gateway 不可达视为失败，触发 apply 失败守卫或 watchdog 恢复；WireGuard 尚未产生 handshake 仍作为启动提示，不在首次检查中误判失败。

验证：新增 WAN 不可达回归测试；workspace 测试、Clippy、fmt、架构边界检查通过。
