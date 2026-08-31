# systemd 配置路径与 UI 应用回显

原因：单元文件声明了 `GATEWAY_KIT_CONFIG`，二进制却未读取，安装契约会 silently 漂移。  
意图：生产 Paths 尊重该环境变量；Web 确认应用后重新拉 plan，不再把 apply 回包当成计划渲染。
