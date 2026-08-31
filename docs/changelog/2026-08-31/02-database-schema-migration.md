# SQLite schema migration

原因：状态库此前依靠 `CREATE TABLE IF NOT EXISTS` 和被忽略错误的 `ALTER TABLE`，无法可靠处理已存在的旧表结构。

意图：引入 schema version 2；新库与旧库都经过同一迁移入口。旧版缺少可重放配置快照的 `generations` 表会重命名为 `generations_legacy_v1`，不伪造回滚数据；冲突表缺失的时间列会显式迁移；未来版本会拒绝启动并报告明确错误。

验证：新增旧 schema 迁移回归测试；`cargo test --workspace`、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all -- --check`、架构边界检查通过。
