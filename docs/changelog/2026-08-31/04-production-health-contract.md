# 生产健康检查契约

原因：健康检查此前只看命令退出码，空 stdout 也可能被判为健康，且没有确认策略表路由或生产数据面单元实际存在。

意图：nft 输出必须包含受管表名；策略规则必须包含独立 table id；WireGuard 接口必须在 link 输出中出现。生产模式额外验证策略表 local default route、sing-box active 和按需 dnsmasq active。测试运行器提供与这些契约对应的最小命令输出 fixture。

验证：workspace 测试、Clippy、fmt、架构边界检查通过。
