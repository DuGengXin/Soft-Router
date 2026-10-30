# VLESS REALITY 改由 Xray-core 建立客户端连接

现象：同一条 VLESS REALITY 链接可在 v2rayNG 使用，但原 sing-box 客户端在网关环境无法建立连接。

变更：保留 sing-box TProxy、DNS 和分流；新增独立 Xray-core VLESS 客户端，从 `secrets.toml` 的单条 `proxy_uri` 渲染连接参数，并只在 `127.0.0.1:10808` 提供 SOCKS5/UDP。sing-box 海外出口改为该本地 SOCKS5。配置、服务、健康检查、依赖探测与脱敏计划均覆盖 Xray；生成配置保持 0600。

范围：当前支持 VLESS TCP + TLS/REALITY，识别 `sni/serverName`、`pbk`、`sid`、`fp`、`spx` 与 `flow`。非 TCP 传输或携带 WebSocket/gRPC 等专属参数会明确拒绝，避免误按 TCP 应用。系统需预装 Xray-core。此改动不触碰远端服务端，也没有执行网关 apply。

验证：`cargo check --workspace` 已通过；尚未运行测试，也未用实际 Xray-core 校验生成配置或验证真实客户端、生产网关。上线前仍需在可回滚窗口验证 Xray 与 sing-box 健康状态和 LAN 客户端访问。
