# 兼容带 first_seen 的旧 conflicts 表

`--local agent` 遇到早期 state.db（`conflicts.first_seen NOT NULL`）时 INSERT 不含该列会直接退出，Web 起不来。写入补上 first_seen，并对缺列的库 `ALTER` 增加该列。
