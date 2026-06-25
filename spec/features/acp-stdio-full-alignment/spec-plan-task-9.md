### Task 9: 更新 ACP_COMPATIBLE.csv

**背景：** 所有新增 handler 就绪后，更新兼容性矩阵。

#### 执行步骤

- [ ] **Step 9.1**: 修改 `docs/ACP_COMPATIBLE.csv`，将以下行的 `stdio_transport` 列从 `NA` 更新为 `✅`：

| method | stdio_transport 旧值 | stdio_transport 新值 | notes 更新 |
|--------|---------------------|---------------------|-----------|
| session/load | NA | ✅ | 2026-05-21 新增 stdio handler |
| session/close | NA | ✅ | 2026-05-21 新增 stdio handler |
| session/resume | NA | ✅ | 2026-05-21 新增 stdio handler |
| session/fork | NA | ✅ | 2026-05-21 新增 stdio handler |
| session/compact | NA | ✅ | 2026-05-21 新增 stdio handler |
| session/clear | NA | ✅ | TUI 路径专有，stdio 同步实现 |
| session/set_thinking | NA | ✅* | 若已实现；否则 "TUI only (equiv: set_config_option thinking_effort)" |
| $/cancel_request | NA | ✅* | 若已实现；否则 "TUI only (handled by session/cancel)" |

**实际 update 时，根据 Task 5 的结果调整 set_thinking 和 $/cancel_request 的标注。**

- [ ] **Step 9.2**: 添加 `session/delete` 行（如果不存在）

当前 CSV 中 `session/delete` 已有条目（第 18 行），状态为 NA/NA/NA。保持不变。

- [ ] **Step 9.3**: 更新 CSV 注释行（第 1 行后的描述）或添加生成日期/版本信息（如果需要）

#### 检查步骤

- [ ] CSV 格式正确（逗号分隔，引号正确）
- [ ] 所有已实现的方法 stdio_transport 列为 ✅
- [ ] CSV 行数与实际方法数匹配

---
