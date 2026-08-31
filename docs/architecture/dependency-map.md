# 依赖方向

```text
gateway-model  （无内部依赖）
      ↑
gateway-core   （只依赖 model）
      ↑
gateway-app    （依赖 core；单二进制 gateway-kit）
```

禁止 core 依赖 app。禁止 model 做 I/O。

| 代码 | 放置 |
|---|---|
| 配置/plan/冲突类型与校验 | gateway-model |
| 发现、渲染、apply、健康、SQLite/migration | gateway-core |
| CLI、HTTP、应用服务编排、嵌入 UI | gateway-app |
| Vue 控制面源码 | `crates/gateway-app/ui/`（Vite 构建后嵌入） |
| install.sh、systemd | packaging/ |

`gateway-app/src/service.rs` 是 CLI 与 HTTP 共用的应用服务边界；它只编排
`discover → plan → confirm → apply → health`，不承载数据面实现。SQLite schema
由 `gateway-core::init_database` 显式迁移并通过 `schema_meta.schema_version` 管理。
