# Active generation state

原因：仅按 generations 的时间排序无法表达当前正在运行的代际，也无法保证 apply 提交与活动状态更新的一致性。

意图：schema version 3 新增 `runtime_state`，以 `active_generation` 指针表示当前成功运行代际。成功 apply 通过单事务写入 generation 和活动指针；开机恢复优先读取活动指针；rollback 排除活动代际后选择上一成功代际，并同步恢复 `config.toml`；disable 清空活动指针，避免历史成功记录被误当成当前运行态。

验证：新增活动指针原子提交测试；workspace 测试与 Clippy 通过。
