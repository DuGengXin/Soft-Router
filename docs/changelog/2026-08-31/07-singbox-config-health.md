# 2026-08-31：sing-box 配置健康校验

- 生产数据面健康检查新增 `sing-box check -c generated/sing-box.json`。
- GeoFile 缓存检查与配置语法检查同时通过后，应用或 watchdog 才认为 sing-box 数据面健康。
- 观察模式和本地模拟路径保持无副作用，不要求本机安装 sing-box。
