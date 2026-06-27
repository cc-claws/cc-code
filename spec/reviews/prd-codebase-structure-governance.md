# PRD: 代码库结构治理

**文档版本**：v1.0
**创建日期**：2026-06-27
**基于扫描**：spec/reviews/2026-06-27.md
**状态**：Draft

---

## 1. 背景与动机

### 1.1 现状

Peri 是一个 Rust Agent 框架，7 个 Workspace Crate + side-projects，总计约 14 万行 Rust 代码。经过 3 个月密集开发，核心 TUI 层多个文件突破千行，职责边界模糊，持续拖累开发效率。

### 1.2 驱动力

- **开发效率下降**：`message_render.rs`（1253 行）和 `message_pipeline/mod.rs`（1070 行）每次修改都需要在大量无关代码中定位目标，心智负担高
- **合并冲突频发**：大文件是多人协作的冲突热点，`event/mod.rs` 和 `message_pipeline/mod.rs` 在过去 1 个月各膨胀 200-600 行
- **新功能入口不清**：新增渲染类型（如 subagent group）需要在 1200+ 行的 `message_render.rs` 中找到正确插入点
- **测试维护成本**：对应测试文件同步膨胀（`headless_test.rs` 3303 行、`message_pipeline_test.rs` 2010 行）

### 1.3 历史回顾

| 扫描日期 | 大文件数 | 死代码 | 测试混合 | 架构问题 |
|---------|---------|--------|---------|---------|
| 2026-05-23 | 21 | 0 | 0 | 3 |
| 2026-05-26 | 46 | 0 | 0 | 3 |
| 2026-06-27 | 15 (非测试) | 0 | 0 | 3 |

> 05-26 到 06-27 期间成功拆分了 `keyboard.rs`（1222→多文件）、`plugin_panel/mod.rs`（827→拆分）、`anthropic/invoke.rs`、`openai/invoke.rs`。但 `message_render.rs`（+641）和 `message_view/mod.rs`（+319）出现显著膨胀，抵消了部分成果。

---

## 2. 目标

### 2.1 核心目标

将所有非测试源文件控制在 **800 行以内**（严重阈值），理想目标 **400 行以内**（警告阈值）。

### 2.2 成功指标

| 指标 | 当前值 | 目标值 | 衡量方式 |
|------|--------|--------|---------|
| ≥800 行文件数 | 8 | 0 | `wc -l` 扫描 |
| ≥400 行文件数 | 15 | ≤5 | `wc -l` 扫描 |
| 最大文件行数 | 1253 | ≤600 | `wc -l` 扫描 |
| `app/mod.rs` 子模块数 | 65 | ≤40 | `grep -c 'mod '` |
| 死代码 warning | 0 | 0 | `cargo clippy` |
| 测试混合 | 0 | 0 | `#[cfg(test)]` 扫描 |
| `cargo test` 通过率 | 100% | 100% | CI |
| 编译时间变化 | baseline | ≤+5% | `cargo build --timings` |

### 2.3 非目标

- 不改变任何功能行为（纯重构）
- 不修改公共 API 签名（仅 `pub(super)` 可见性调整）
- 不触碰 side-projects/（独立治理周期）
- 不重构 app/mod.rs 子目录重组（Phase 3，另行立项）

---

## 3. 问题诊断

### 3.1 大文件清单（≥800 行，按严重程度排序）

| # | 文件 | 行数 | 函数数 | 根因 | 膨胀趋势 |
|---|------|------|--------|------|---------|
| 1 | `peri-tui/src/ui/message_render.rs` | 1253 | 15 | 工具渲染+ANSI 渲染+通用渲染混合 | +641（严重恶化） |
| 2 | `peri-tui/src/main.rs` | 1138 | 15 | CLI+TUI 启动+信号处理混合 | +355（恶化） |
| 3 | `peri-tui/src/app/message_pipeline/mod.rs` | 1070 | 47 | 流式管线+节流+子代理混合 | +291（恶化） |
| 4 | `peri-tui/src/ui/message_view/mod.rs` | 1033 | 24 | ViewModel 定义+工厂方法+Hash/PartialEq | +319（恶化） |
| 5 | `peri-tui/src/event/mod.rs` | 1003 | 20 | 事件路由巨型 match（468 行） | +95（持续恶化） |
| 6 | `peri-tui/src/acp_stdio.rs` | 988 | 3 | `run_acp_stdio` 单函数 902 行 | +90（恶化） |
| 7 | `peri-tui/src/event/keyboard/normal_keys.rs` | 808 | 16 | keyboard.rs 拆分产物仍偏大 | 新增 |
| 8 | `peri-middlewares/src/hooks/middleware.rs` | 783 | 15 | matcher+executor+loader 混合 | +98（恶化） |

### 3.2 次要大文件（400-800 行）

| 文件 | 行数 | 状态 |
|------|------|------|
| `peri-acp/src/langfuse/tracer.rs` | 761 | 已记录 |
| `peri-widgets/src/markdown/render_state.rs` | 748 | 已记录 |
| `peri-tui/src/clipboard/copy.rs` | 746 | 新增 |
| `peri-tui/src/ui/main_ui/status_bar.rs` | 680 | 新增 |
| `peri-middlewares/src/plugin/loader.rs` | 669 | 已记录 |
| `peri-tui/src/app/mod.rs` | 668 | 恶化 |
| `peri-middlewares/src/mcp/config.rs` | 654 | 已记录 |

### 3.3 健康维度（无问题）

| 维度 | 状态 | 说明 |
|------|------|------|
| 死代码 | 🟢 满分 | clippy 零 warning，TODO 仅 8 处 |
| 测试分离 | 🟢 满分 | 全部 `#[cfg(test)] mod xxx_test;`，无内联测试 |
| 依赖方向 | 🟢 满分 | Cargo.toml 依赖方向全部正确 |
| 万能模块 | 🟢 满分 | 无 utils/common/helpers 滥用 |

---

## 4. 技术方案

### 4.1 拆分原则

1. **纯提取，零行为变更**：每个拆分步骤只是将函数/结构体移动到新文件，不修改逻辑
2. **保持编译通过**：每完成一个拆分立即 `cargo build` + `cargo test` 验证
3. **最小公开面**：提取的模块使用 `pub(super)` 或 `pub(crate)`，不扩大 API
4. **测试跟随**：对应 `_test.rs` 文件同步拆分（如有大量测试覆盖被拆函数）

### 4.2 Phase 1：低风险纯函数提取（1-2 天）

#### 4.2.1 `event/mod.rs` → `event/{paste,mouse}.rs`

**当前结构**：
```
event/mod.rs (1003 行)
├── record_mouse_event()          L33-40
├── last_mouse_event_ms()         L42-55
├── next_event()                  L57-93      ← 事件泵入口
├── coalesce_drag_events()        L95-151     ← 鼠标事件合并
├── detect_simulated_paste()      L153-187    ← Paste 检测
├── is_simulated_paste_start()    L189-205
├── key_event_to_text()           L207-250    ← 通用辅助
├── point_in_rect/hit_bar()       L252-268    ← 鼠标命中检测
├── handle_message_scrollbar_*()  L270-337    ← 滚动条交互
├── handle_event()                L339-803    ← 468 行巨型 match
│   ├── Event::FocusGained/Lost   L341-348    (8 行)
│   ├── Event::Resize             L349-352    (4 行)
│   ├── Event::Key → delegate     L353-355    (3 行)
│   ├── Event::Paste              L356-465    (110 行) ← 提取
│   ├── Event::Mouse              L467-798    (330 行) ← 提取
│   └── _ => {}                   L797
└── handle_oauth_prompt()         L807-855    (49 行) ← 提取
```

**拆分方案**：

| 新文件 | 提取内容 | 行数 | 依赖 |
|--------|---------|------|------|
| `event/paste.rs` | `detect_simulated_paste` + `is_simulated_paste_start` + `Event::Paste` 分支 + `handle_oauth_prompt` | ~170 | `&mut App` |
| `event/mouse.rs` | `record_mouse_event` + `last_mouse_event_ms` + `coalesce_drag_events` + `point_in_*` + `handle_message_scrollbar_*` + `Event::Mouse` 分支 | ~400 | `&mut App` |
| `event/mod.rs` | `next_event` + `handle_event` 路由骨架（Focus/Resize/Key → delegate, Paste → paste::, Mouse → mouse::） | ~150 | — |

**验证**：`cargo build -p peri-tui && cargo test -p peri-tui`

#### 4.2.2 `message_render.rs` → `render_{tool,shell}.rs`

**当前结构**：
```
message_render.rs (1253 行)
├── 工具渲染函数           L15-340
│   ├── parse_exit_code()
│   ├── dim_markdown_lines()
│   ├── wrap_line_spans()
│   ├── error_summary_lines()
│   ├── tool_args_header()
│   ├── read_summary()
│   ├── glob_summary()
│   ├── render_batch_summary()
│   └── render_ask_user_block()
├── ANSI/Shell 渲染        L345-570
│   ├── shell_fg_color()
│   ├── apply_sgr_codes()
│   ├── ansi_spans()
│   ├── shell_output_line()
│   └── render_shell_command()
└── render_view_model()    L571-1253  (主渲染入口)
```

**拆分方案**：

| 新文件 | 提取内容 | 行数 | 说明 |
|--------|---------|------|------|
| `ui/render_tool.rs` | 工具渲染函数全部（L15-340） | ~325 | 纯函数，零状态 |
| `ui/render_shell.rs` | ANSI/Shell 渲染全部（L345-570） | ~225 | 纯函数，零状态 |
| `ui/message_render.rs` | `dim_markdown_lines` + `wrap_line_spans` + `render_view_model` | ~500 | 保留主入口 |

**验证**：`cargo build -p peri-tui && cargo test -p peri-tui`

#### 4.2.3 `message_view/factory.rs` 提取

**当前结构**：
```
message_view/mod.rs (1033 行)
├── build_diff_input()                    L24-178
├── impl PartialEq/Hash for MessageVM     L180-520
├── impl MessageViewModel                 L523-1010
│   ├── from_base_message()               L527-766   (240 行！)
│   ├── append_chunk/toggle_collapse      L768-834
│   ├── 工厂方法: user/assistant/tool...  L836-990   (155 行) ← 提取
│   └── content_hash/recompute_hash       L995-1010
```

**拆分方案**：

| 新文件 | 提取内容 | 行数 |
|--------|---------|------|
| `message_view/factory.rs` | `user()`/`user_with_expanded()`/`assistant()`/`tool_block()`/`tool_block_with_id()`/`shell_command_pending()`/`shell_command_completed()`/`system()`/`cache_warning()`/`subagent_group()` | ~160 |
| `message_view/view_model.rs` | struct 定义 + `from_base_message` + `append_chunk` + 查询方法 + Hash/PartialEq | ~850 |

**验证**：`cargo build -p peri-tui && cargo test -p peri-tui`

---

### 4.3 Phase 2：中风险结构拆分（2-3 天）

#### 4.3.1 `message_pipeline/mod.rs` → `chunking.rs` + `subagent.rs`

**当前结构**：
```
message_pipeline/mod.rs (1070 行)
├── AdaptiveChunkingPolicy impl    L103-293   (190 行) ← 提取
│   ├── new/on_chunk/on_reasoning_chunk
│   ├── check/drain/reset/update_mode
│   └── is_catch_up
├── MessagePipeline impl           L296-1070
│   ├── 核心管线: handle_event/push_chunk/push_reasoning  L358-638
│   ├── 工具处理: tool_start_internal/tool_end_internal   L639-757
│   ├── 子代理: push_tool/push_chunk/update/drain         L758-907 ← 提取
│   ├── 生命周期: done/interrupt/clear/begin_round        L830-985
│   ├── 节流: check_throttle streaming/block              L988-1030
│   └── 查询: completed_messages/completed_stats          L1032-1065
```

**拆分方案**：

| 新文件 | 提取内容 | 行数 |
|--------|---------|------|
| `message_pipeline/chunking.rs` | `AdaptiveChunkingPolicy` impl + `DrainPlan` + `StreamingMode` 类型 | ~200 |
| `message_pipeline/subagent.rs` | `push_tool_start_to_subagent`/`push_chunk_to_subagent`/`update_tool_end_in_subagent`/`find_running_subagent_mut`/`drain_subagent_stack` | ~160 |
| `message_pipeline/mod.rs` | `MessagePipeline` 核心：构造+事件处理+节流+生命周期+查询 | ~400 |

**注意**：`SubAgentState` 类型需保留在 `mod.rs` 或提取到 `types.rs`。

#### 4.3.2 `acp_stdio.rs` → `acp_stdio/{setup,session,config}.rs`

**当前结构**：
```
acp_stdio.rs (988 行)
├── StdioBroker struct + impl       L45-84     (40 行)
└── run_acp_stdio()                 L86-988    (902 行单函数)
    ├── 初始化: telemetry/cwd/config/provider   L87-107
    ├── 初始化: cron + MCP pool                 L109-200
    ├── SessionState 构建                       L200-300
    ├── 消息循环: session/prompt 处理           L300-600
    ├── 配置: config option + model switching   L600-800
    └── 清理: session 保存 + 资源释放           L800-900
```

**拆分方案**：

| 新文件 | 提取内容 | 行数 |
|--------|---------|------|
| `acp_stdio/setup.rs` | provider 加载 + cron 初始化 + MCP pool 初始化 + SessionState 构建 | ~200 |
| `acp_stdio/session.rs` | session loop + prompt dispatch + cancel handling + 配置处理 | ~500 |
| `acp_stdio.rs` | `StdioBroker` + `run_acp_stdio` 调度骨架（调用 setup → session loop） | ~200 |

**注意**：需要将 session state 作为参数在函数间传递，可能需要定义 `StdioSession` struct 封装共享状态。

#### 4.3.3 `hooks/middleware.rs` → `hooks/dispatch.rs`

**当前结构**：
```
hooks/middleware.rs (783 行)
├── HooksMiddleware struct          L1-30
├── Middleware trait impl           L30-783
│   ├── before_agent               ← hook 触发逻辑
│   ├── after_agent                ← hook 触发逻辑
│   ├── before_model               ← hook 触发逻辑
│   ├── after_model                ← hook 触发逻辑
│   ├── before_tool                ← hook 触发逻辑
│   └── after_tool                 ← hook 触发逻辑
```

**拆分方案**：

| 新文件 | 提取内容 | 行数 |
|--------|---------|------|
| `hooks/dispatch.rs` | `trigger_hooks()` 统一触发函数 + hook 匹配/执行/结果收集逻辑 | ~450 |
| `hooks/middleware.rs` | `HooksMiddleware` struct + Middleware trait impl（每个钩子仅调用 `trigger_hooks()`） | ~300 |

---

### 4.4 Phase 3：高风险长期治理（独立立项）

#### 4.4.1 `main.rs` 拆分

| 新文件 | 内容 | 行数 |
|--------|------|------|
| `cli/launch.rs` | TUI app 构建 + session 恢复 + 信号处理 | ~400 |
| `main.rs` | `fn main()` + 子命令 dispatch | ~200 |

#### 4.4.2 `app/mod.rs` 子目录重组

65 个子模块按功能域聚合为 `panels/`、`interaction/`、`input/` 子目录。涉及大量 `use` 路径变更，建议分 3-4 个 PR 渐进推进。

---

## 5. 执行计划

### 5.1 里程碑

| 阶段 | 内容 | 预估工时 | PR 数量 | 风险 |
|------|------|---------|---------|------|
| Phase 1 | event/render/factory 纯函数提取 | 1-2 天 | 3 | 低 |
| Phase 2 | pipeline/stdio/hooks 结构拆分 | 2-3 天 | 3 | 中 |
| Phase 3 | main.rs + app/mod.rs 重组 | 3-5 天 | 4-5 | 高 |
| **合计** | | **6-10 天** | **10-11** | |

### 5.2 Phase 1 详细计划

| PR | 目标文件 | 拆分动作 | 预估行数变化 | 验证 |
|----|---------|---------|------------|------|
| PR-1 | `event/mod.rs` | → `event/paste.rs` + `event/mouse.rs` | 1003 → 150+170+400 | cargo test |
| PR-2 | `message_render.rs` | → `render_tool.rs` + `render_shell.rs` | 1253 → 500+325+225 | cargo test |
| PR-3 | `message_view/mod.rs` | → `factory.rs` | 1033 → 850+160 | cargo test |

### 5.3 Phase 2 详细计划

| PR | 目标文件 | 拆分动作 | 预估行数变化 | 验证 |
|----|---------|---------|------------|------|
| PR-4 | `message_pipeline/mod.rs` | → `chunking.rs` + `subagent.rs` | 1070 → 400+200+160 | cargo test |
| PR-5 | `acp_stdio.rs` | → `setup.rs` + `session.rs` | 988 → 200+200+500 | cargo test |
| PR-6 | `hooks/middleware.rs` | → `dispatch.rs` | 783 → 300+450 | cargo test |

### 5.4 每个 PR 的验收标准

1. `cargo build --workspace` 编译通过
2. `cargo test --workspace` 全部通过
3. `cargo clippy --workspace` 零新增 warning
4. `cargo fmt --check` 格式正确
5. 被拆文件行数降至目标值
6. 无新增 `pub` API（仅 `pub(super)` 或 `pub(crate)`）
7. 对应测试文件同步更新（如有）

---

## 6. 风险与缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| 拆分导致编译错误 | 中 | 低 | 每个提取步骤立即编译验证 |
| 类型可见性问题 | 低 | 中 | 使用 `pub(super)` 替代 `pub`，必要时提取共享类型到 `types.rs` |
| 测试覆盖盲区 | 低 | 中 | 拆分后运行全量测试，关注被拆函数的测试是否仍可达 |
| 引入循环依赖 | 低 | 高 | 新模块仅依赖 `mod.rs` 中的类型，不反向依赖 |
| Phase 3 影响面过大 | 高 | 高 | 独立立项，分批推进，每批 1 个 PR |

---

## 7. 长期维护

### 7.1 预防机制

- **CI 行数检查**：在 CI 中增加 `wc -l` 检查，新文件超过 600 行时 warning，超过 800 行时 fail
- **PR Review 规则**：Review 时关注文件行数增量，单次 PR 超过 +200 行的大文件需说明理由
- **定期扫描**：每月运行一次 slop scan，跟踪趋势

### 7.2 目标状态

```
治理后预期文件分布：
  ≤200 行：~80% 文件（健康）
  200-400 行：~15% 文件（正常）
  400-600 行：~5% 文件（可接受）
  ≥600 行：0 文件（目标）
```

---

## 附录

### A. 上次行动项回顾

| 行动项（05-26 报告） | 状态 |
|---------------------|------|
| 🔴 拆分 `subagent/tool/define.rs`（1242 行） | ✅ 已完成 |
| 🔴 拆分 `event/keyboard.rs`（1222 行） | ✅ 已完成 |
| 🔴 拆分 `event/mod.rs`（908 行） | ❌ 恶化至 1003 行 |
| 🔴 拆分 `acp_stdio.rs`（898 行） | ❌ 恶化至 988 行 |
| 🔴 拆分 `plugin_panel/mod.rs`（827 行） | ✅ 已完成 |
| 🔴 拆分 `langfuse/tracer.rs`（764 行） | ⚠️ 微降至 761 行 |
| 🟡 拆分 `anthropic/invoke.rs`（692 行） | ✅ 已完成 |
| 🟡 拆分 `openai/invoke.rs`（664 行） | ✅ 已完成 |
| 🔴 重组 `app/mod.rs`（61 子模块） | ❌ 恶化至 65 子模块 |

### B. 扫描报告原文

见 `spec/reviews/2026-06-27.md`
