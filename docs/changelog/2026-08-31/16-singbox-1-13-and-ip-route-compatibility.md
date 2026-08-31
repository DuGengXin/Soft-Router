# 兼容 sing-box 1.13 与 iproute2 的真实输出

远端真实 apply 暴露两处兼容问题：sing-box 1.13 已移除 inbound 内的旧版 `sniff` 字段；iproute2 5.15 展示本地默认路由时使用 `local default`，而非固定的 `local 0.0.0.0/0` 文本。

变更：将 sniff 迁移为 tproxy inbound 关联的 route action；健康检查同时接受两种合法的 iproute2 输出格式。apply 失败保护已确认会清理本产品 nft、策略路由和数据面单元，恢复旁路。
