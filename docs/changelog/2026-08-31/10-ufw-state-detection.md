# 2026-08-31：按实际状态识别 UFW

- 不再把 `systemctl is-active ufw` 的 active/exited 状态直接视为 UFW 已启用。
- discovery 改为读取 `ufw status`，仅 `Status: active` 产生 apply blocker。
- 在真实远端 Ubuntu 上验证：UFW inactive 不再阻止 Observe/Gateway 预检，Docker 仍保持外部观察状态。
