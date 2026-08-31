# GeoFile 缓存健康检查

原因：`china_direct` 依赖 sing-box 远程 geoip/geosite rule-set，原健康检查只确认 tproxy 和服务，无法识别缓存缺失或空文件。

意图：新增带缓存路径的健康检查入口；生产 apply、watchdog 和数据面状态 API 在启用 `china_direct` 时检查 `generated/sing-box-cache.db` 为非空普通文件。未提供路径的兼容调用只记录提示，不伪造文件系统状态。

验证：新增 GeoFile 缓存缺失/存在回归测试；workspace 测试、Clippy、fmt、架构边界检查通过。
