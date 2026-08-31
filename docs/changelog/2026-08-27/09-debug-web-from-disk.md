# debug 读磁盘 UI

原因：Web 静态资源 `include_str` 导致改 HTML/CSS 必须重编，小屏抽屉无法即时验收。  
意图：debug 优先读 `crates/gateway-app/web/`，release 仍嵌入单二进制；不引入 npm。
