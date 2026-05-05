# acpx-g Design Review Progress

> 2026-05-04 ~ 05-05，22 轮迭代，测试 16 → 155

## 按主题归类

### 核心架构（R7/8/10/11/13）
- SQLite 事务保护 + 外键启用 + 索引优化（created_at/run_id/status）
- `execute_with_retry()` 泛化，统一 shell/agent 重试循环，消除 ~120 行重复
- API 分页（page/per_page）、并发限制（`ACPX_MAX_CONCURRENT`）、CORS 可配置（`ACPX_CORS_ORIGIN`）
- NodeDefaults 实际应用到执行器（timeout/retry 回退）、优雅关机（CancellationToken）、自依赖检测
- 并发工作流信号量（`ACPX_MAX_CONCURRENT_RUNS`，默认 8）、Watcher 优雅关机

### DAG 执行正确性（R4/8/10/14）
- `continue_on_error` 下游逻辑多次迭代修正：失败节点 → completed 集合 → 下游可引用输出
- 失败传播语义：硬失败终止 workflow，软失败（continue_on_error）不终止
- 重复/自依赖 node ID 检测、Node ID 字符白名单 `[a-zA-Z0-9_\-/]`
- 重试输出累积（`--- Attempt N ---`）、超时输出捕获、退避溢出保护（checked_shl + 60s 上限）
- 关机竞态修复（原子 UPDATE）、远程 URL 循环引用检测、分页溢出保护

### Schema/数据校验（R3/5/9/14/20）
- `validate_workflow`：空名称、空节点列表、不存在的引用、重复 ID、自依赖
- 输入类型校验（number/boolean/default 值）、前端 required 字段校验
- YAML 大小限制（1MB → 413）、空提交拒绝（400）
- 提交时 depends 引用校验，错误信息列出可用节点

### 新功能（R15-18）
- **Cancel**：POST cancel，CancellationToken 终止子进程，幂等
- **Rerun**：持久化 inputs（幂等迁移），支持覆盖合并
- **Workflow timeout**：YAML 可选字段，超时触发取消
- **条件执行**：`if` 字段，truthiness + `==`/`!=` 比较运算符

### 生产可靠性（R12/13）
- Health Check（GET /health）、DELETE Run（级联事务化）
- 输出截断（256KB，字符边界安全）、并发限制信号量
- Shell 覆盖支持（`shell: "zsh -c"`）

### 前端 UX（R1/2/5/6/19/21/22）
- Toast 通知、inputs 表单 + 前端校验、执行耗时、进度条、日志 spinner
- 消除所有内联 onclick → data-* + addEventListener（XSS 防护）
- CSS 选择器注入修复（cssEsc → domId）、API 文档模态框
- Cancel/Re-run/Delete 按钮 + 确认对话框
- 敏感输入遮蔽（`***`）、描述性 404、confirmDelete 函数声明修复

## 测试增长

| 轮次 | 测试数 | 关键新增 |
|------|--------|----------|
| R1   | 24     | validate_inputs 类型校验 |
| R3   | 29     | schema resolve |
| R4   | 34     | DAG 拓扑 + continue_on_error |
| R5   | 43     | loader/prefix_id/template |
| R7   | 56     | executor 泛化 |
| R8   | 67     | topo + template + watcher |
| R9   | 84     | schema + input validation |
| R11  | 94     | self-dep/defaults |
| R13  | 114    | concurrent/shell/CLI/分页 |
| R14  | 119    | node ID 校验 |
| R15  | 128    | cancel |
| R17  | 131    | timeout |
| R18  | 146    | 条件执行 |
| R21  | 155    | 敏感输入遮蔽 |

## R23 — Design Review (用户思维)

修复拓扑实时指示器双重 display 属性 bug、卡片视图补充删除/重跑按钮、节点列表高亮改用 data-node-id 替代 onclick.toString()、Toast 关闭按钮去除内联 onclick、节点日志复制按钮改用 data-log-id、运行详情页 Escape 键返回列表、修复失败统计图标重复 style 属性。

## R24 — Design Review Round 2 (用户思维)

修复搜索/过滤后分页总数不更新、run-detail 轮询定时器未纳入全局清理导致页面切换泄漏、confirmDialog 关闭/取消按钮去除内联 onclick、加载失败时隐藏骨架屏和布局区、编辑器加载模板增加未保存确认。

## R25 — Design Review Round 3 (用户思维)

工作流设置弹窗 6 处内联 onclick 替换为 addEventListener、YAML 应用后自动验证并提示错误数、运行详情 ID 可点击复制完整值、验证错误徽章可点击查看详情（title+toast）、筛选无结果区分"无数据"与"无匹配"两种空状态文案。

## R26 — Design Review Round 4 (用户思维)

页面标题随路由动态更新（编辑器/运行记录/运行详情）、编辑器运行按钮增加 loading 状态防重复点击、运行详情轮询失败时 toast 通知用户实时更新已停止、relativeTime 处理负值（时钟偏差）避免奇怪显示。

## R27 — Design Review Round 5 (用户思维)

拓扑图 depends JSON.parse 加 try-catch 防格式错误崩溃、属性面板 textarea 增加 input 事件 500ms 防抖实时同步 YAML、拓扑节点 ID 超 16 字符截断加省略号防溢出、保存按钮增加 loading 状态防重复提交。

## R28 — Design Review Round 6 (用户思维)

全局 Escape 键关闭模态框、YAML 导入成功/失败区分 toast 提示、运行详情页标题包含工作流名称。

## R29 — Design Review Round 7 (用户思维)

运行记录页有运行中工作流时自动每 10 秒刷新、节点日志切换时重置滚动位置、错误/警告 toast 延长显示时间（5s/4s）、拓扑图节点添加 hover tooltip 显示完整 ID。

## R30 — Design Review Round 8 (用户思维)

导入模态框支持 Escape 关闭、编辑器自动布局按钮添加成功 toast 反馈、运行详情页标题栏添加手动刷新按钮。

## R31 — Design Review Round 9 (用户思维)

运行详情返回按钮和关闭日志按钮去除内联 onclick 改用 addEventListener、修复 loadRunDetail 刷新时旧轮询定时器未清除导致叠加泄漏。

## R32 — Design Review Round 10 (用户思维)

运行详情 context 面板（节点列表/操作按钮/ID 复制/拓扑 SVG）全部改用事件委托替代内联 onclick、导入成功后清空 textarea 防止误操作。

## R33 — Design Review Round 11 (用户思维)

运行记录页表格行/卡片/操作按钮/分页全部改用事件委托替代内联 onclick（消除前端最后一批内联处理器）、删除当前页最后一条记录后自动回退前一页。

## R34 — Design Review Round 12 (用户思维)

API 文档复制按钮去除内联 onclick 改用 addEventListener、Drawflow 画布 ondrop/ondragover 改用 addEventListener。至此前端 JS 文件全部消除内联事件处理器。

## R35 — Design Review Round 13 (用户思维)

修复 undo/redo 恢复快照时历史栈被污染的 bug（`_restoringSnapshot` 标志位阻止 `pushHistory` 在 `restoreSnapshot` 期间写入）、删除 run-detail.js 中未使用的 `totalOutputs` 变量、节点重命名时 ID 被标准化后向用户显示 warning toast 提示。

## R36 — Design Review Round 14 (用户思维)

confirmDialog 的 `.onclick` 赋值改为 `addEventListener`（与全项目事件绑定风格统一）、验证状态徽章的 onclick 改用 clone-replace 模式避免重复绑定（修复"无错误"状态点击仍弹出旧错误的 bug）、运行详情轮询新增 `renderContextActions` + `renderRunDetailHeader` 调用（run 完成时操作按钮和头部状态指示器即时更新）。

## R37 — Design Review Round 15 (用户思维)

修复 `debounceTimer` 共享 bug：`pushHistory`（300ms）、`saveDraft`（1000ms）、`nodeMoved` 三个独立防抖操作共用同一个 timer 变量导致互相取消（saveDraft 清除 pushHistory 的 pending setTimeout，历史快照可能丢失）。新增独立的 `draftTimer` 变量给 `saveDraft` 使用。

## R38 — Design Review Round 16 (用户思维)

`destroyEditor` 新增 `clearTimeout(debounceTimer/draftTimer)` 防止离开编辑器后 pending setTimeout 操作已销毁的编辑器状态；`loadDraft` 中 `nodeIdCounter` 赋值从 `||` 改为 `??`（nullish coalescing）避免 counter 为 0 时错误回退导致节点 ID 冲突。

## R39 — Design Review Round 17 (用户思维)

YAML 编辑器 textarea 新增 `:focus` 可见焦点状态（`box-shadow: inset 0 0 0 1px var(--brand)`）修复键盘可访问性；循环依赖验证从"依赖图中存在循环"改为显示具体循环路径（如 `step-1 → step-2 → step-1`），帮助用户快速定位问题。

## R40 — Design Review Round 18 (用户思维)

`editorValidate` 服务端验证响应的 `.map(e => e.message)` 增加 `|| String(e)` 防御，避免 API 返回非标准错误格式时显示 `undefined`；`loadDraft` 损坏草稿不再静默忽略，改为清除 localStorage 并显示 warning toast 通知用户。

## R41 — Design Review Round 19 (用户思维)

CSS 硬编码颜色统一为变量：`#E2E8F0`（run-detail 进度条轨道）→ `var(--border-default)`、`#f8fafc`（code-block 背景）→ `var(--surface-void)`、`#dc2626`（danger 按钮悬停）→ `color-mix(in srgb, var(--danger) 85%, black)`、`#8250df`（代理节点紫色）→ 新增 `--node-agent` CSS 变量并替换 editor.css 中两处引用。

## R42 — Design Review Round 20 (用户思维)

`rerunRun` 新增 `confirmDialog` 确认对话框（用户不再意外触发资源消耗型操作）、修复 `result.run_id` 为空时 URL 变为 `#run/undefined` 的 bug；`.table-action` 新增 `:focus` 规则使键盘用户 Tab 聚焦时操作按钮可见。

## R43 — Design Review Round 21 (用户思维)

新增 2 个 Reference 节点解析测试（`test_parse_workflow_reference_valid` 验证基本解析、`test_parse_workflow_reference_with_params` 验证 with 参数和 depends 依赖），测试 155 → 157。

## R44 — Design Review Round 22 (用户思维)

移除 `run-detail.js` 中未使用的 `typeColors` 变量（死代码清理）；修复 `app.js` 中 `nodeTypeColor` 仍使用硬编码 `#8250DF` 改为 CSS 变量 `var(--node-agent)`（与 R41 CSS 变量统一保持一致）；Toast 容器增加 `aria-live="polite"` + `aria-atomic="true"` 使屏幕阅读器可感知动态通知；Modal 叠加层增加 `role="dialog"` + `aria-modal="true"` 语义属性；Modal 打开时自动聚焦首个可交互元素（键盘/屏幕阅读器用户无需 Tab 寻找）；`copyToClipboard` 增加 `execCommand('copy')` fallback 兼容非 HTTPS 环境的剪贴板操作。

## R45 — Design Review Round 23 (用户思维)

修复卡片视图状态图标逻辑：`cancelled` 使用 `octagon` 图标、`skipped` 使用 `minus` 图标（此前均 fallback 到不相关的 `clock`）；`runs.css` 补充 `.skipped` 样式（图标背景色和状态标签背景色）；`run-detail.css` 新增 `@media (max-width: 768px)` 响应式断点（双列布局折叠为单列、统计行 flex-wrap）；新增 `test_parse_workflow_per_node_exec_config` 测试验证节点级 timeout/retry/shell/continue_on_error 解析（157 → 158）。

## R46 — Design Review Round 24 (用户思维)

修复 `destroyEditor` 未清理 `wfBaseDir` 导致导航离开再返回编辑器时工作流提交错误 base_dir 的 bug；草稿保存/恢复新增 `baseDir` 字段避免工作流丢失模板目录上下文；移除 Toast 容器的 `aria-atomic="true"` 修复多条 toast 并存时屏幕阅读器重复朗读全部内容的问题（仅保留 `aria-live="polite"` 即可只播报新增 toast）。

## R47 — Design Review Round 25 (用户思维)

`importFromYaml` 新增 `wfBaseDir = null` 重置（导入新 YAML 后旧模板目录不再残留，避免工作流在错误目录执行）；`formatDuration` 对小于 1 秒的耗时显示 `<1s` 替代 `0s`（瞬间完成的工作流不再显示令人困惑的 `0s`）；新增 `test_parse_workflow_agent_with_optional_fields` 测试验证 agent 节点的 `agent`/`model`/`cwd` 可选字段解析（158 → 159）。

## R48 — Design Review Round 26 (用户思维)

实现"运行记录"Tab 运行中数量徽章（`updateSidebarStatus` 原为死代码——`statusRunningCount` 元素不存在；新增 `.topbar-badge` CSS、HTML 元素、动态显示/隐藏逻辑，用户无需切换页面即可看到有多少工作流在运行）；运行详情状态进度条颜色区分：`failed` → `danger`（红色）、`cancelled` → `warning`（橙色）、`skipped` → `muted`（灰色），此前三者均与 `pending` 相同的 `brand` 色无法视觉区分；新增 `test_parse_workflow_depends_string_instead_of_array` 测试验证 `depends` 写成裸字符串（非数组）时 serde 正确报错（159 → 160）。
