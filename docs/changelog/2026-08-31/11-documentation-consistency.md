# 2026-08-31：架构文档与实现对齐

- 将活跃 roadmap 的入口、crate 和 UI 构建说明同步到当前 `gateway-app` / `gateway-kit` 单二进制实现。
- 明确发布运行时不需要 Node，但编译嵌入式 UI 需要 Node 20+。
- 安装文档补充旧版 Router 检测会阻止 Gateway apply 的迁移前置条件。
