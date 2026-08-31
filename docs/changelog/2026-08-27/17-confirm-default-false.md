# 无确认 apply 默认为否

原因：`POST /api/v1/apply` 若省略 `confirm` 会 422，调用方可能误以为要再试带 true。  
意图：`confirm` serde 默认 false，空 JSON 与硬约束一致（必须明确确认）。向导可填 WG 网卡名；install 的 secrets 骨架与 example 对齐。
