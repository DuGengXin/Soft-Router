# 2026-08-31：持久化 apply 恢复日志

- 在写入主机网络状态前，将待应用代际和配置快照写入 SQLite `runtime_state.pending_apply`。
- 成功提交代际或进入 disable 时清除日志。
- 进程在 apply 中途退出后，下一次启动先记录恢复事件并进入安全 bypass，再决定是否重新应用成功代际。
