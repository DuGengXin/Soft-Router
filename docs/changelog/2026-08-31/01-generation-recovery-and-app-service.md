# 代际恢复语义与应用服务编排

原因：rollback 原先读取“最近成功代际”，成功 apply 后可能重新应用当前配置；CLI 与 HTTP 也各自编排 discover/plan/apply 流程，失败路径容易出现行为漂移。

意图：区分最新成功代际与上一成功代际；健康连续失败时优先尝试上一代，失败才进入紧急旁路；apply 使用失败守卫清理半套数据面并恢复旧 generated 文件；CLI 与 HTTP 统一通过 `gateway-app` 内部 `AppService` 进入控制面生命周期。

验证：`cargo test --workspace`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all -- --check`、架构边界检查。
