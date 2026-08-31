# Linux CI 门禁

原因：开发机是 Windows，无法在本机证明 Debian/Ubuntu 上 cargo 与 install.sh 能跑。  
意图：GitHub Actions 在 ubuntu-latest 跑 fmt/test/clippy、架构边界，以及 `install.sh --dry-run`（仍零网络变更）。双网口分流仍需真机。
