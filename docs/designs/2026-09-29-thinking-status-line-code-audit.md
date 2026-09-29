# 代码审计：Thinking 状态行 + 工具动作汇总

> 状态：**审计记录**（只核查，未改代码）
> 日期：2026-09-29
> 分支：`feat/thinking-status-line`
> 对照设计：`docs/designs/2026-09-29-thinking-status-line-and-tool-summary.md`（PRD）、`docs/designs/2026-09-29-render-effect-mock.md` / `...-mock-dynamic.md` / `...-preview.html`（效果稿）
> 审计范围：工作区全部未提交改动（24 个修改文件 + 1 个新增测试文件）

---

## 第一轮审计（改造后代码 vs 设计，2026-09-29）

结论：**核心骨架（改动 A 状态机 + 改动 B 计数注入 + Bash 摘要）已落地，但未达标**。发现 1 个 P0 数据链路 bug、4 处 P1 规格未实现、3 处 P2 边界/偏差。

### P0

**1. Anthropic 路径 `duration_ms` 链路断裂**

- `peri-agent/src/llm/anthropic/stream.rs` 在 `content_block_stop` 时把 `duration_ms` 写入中间 JSON；
- 但解析入口 `parse_content_blocks`（`peri-agent/src/llm/anthropic/invoke.rs:249-257`）构造 `ContentBlock` 时只取 `thinking`/`signature`，`duration_ms` 被丢弃。
- 后果：Anthropic 模型下 `Thought for Ns` 永远退化为 `Thought for N chars`。OpenAI 路径正常（`reasoning_with_duration`）。
- 测试盲区：现有测试只断言「请求体不泄漏 duration_ms」，未测「流式解析保留 duration_ms」。

### P1（与设计规格不符）

**2. `∴` 前缀未移除**：设计 §3.2/§3.3/mock §3.3/HTML 一致要求去掉；`message_render.rs:1041` 仍渲染 `Span::styled("∴ ", ...)`。

**3. spinner 缺状态② `· thought for {N}s`**：设计 §2.2 四态之一（思考段结束后尾字段定格），mock 动态时间线与 HTML 回放均有演示。`thinking_status_word` 非思考段返回空串；`last_thought_ms` 只喂了热度色未用于文案。测试 `test_thinking_status_word_四态判定` 实际只断 3 种非空态。

**4. 连续思考未合并**（本轮程序员声称已修，见第二轮）：设计 §3.3「连续思考合并计数」「多轮极短 Thought 合并为一行」。

**5. 纯动作行缺失**：设计 §2.3「纯工具调用（无 reasoning）时直接输出 `{actions}`」（如 `Listed 1 directory`）。无 Reasoning 的纯工具轮折叠后无任何可见行。

### P2（边界/偏差）

**6. Bash 单行成功输出被吞**：`content.contains('\n')` 才展开前 3 行摘要；单行输出走折叠分支（非 Read/非 error）完全不显示。失败单行有 `error_summary_lines` 兜底（「失败必显」符合）。

**7. 时长来源与 PRD 文字不一致**：设计 §5 说改动 B 秒数「依赖 §A-2 的计时点」（spinner）；实际在 LLM 流式层独立计时并持久化。效果更好（可 restore），但属实现偏离 PRD，按设计 §8 应回填差异。

**8. 进行时态 `Reading 1 file…` 未做**：设计 §6 开放问题 #7 明确「本次暂不纳入」，不算违规；差异总表 #6 仍 open，需文档标注。

### 核对通过项

`thinking`/`thinking more`/`still thinking` 三态判定与 `still > more` 优先级、10s 可调阈值、配色四档 5/15/30s 且仅 verb+状态词变色、工具段热度沿用、方案 A verb 整轮固定（含 SubAgent 不换词）、尾字段英文固定、Bash 前 3 行 + `... (N more lines)` 措辞、只读计数单复数、`is_error` 失败必显、Write/Edit diff 不动、`duration_ms` 不泄漏进 LLM 请求、旧数据无字段优雅降级。

---

## 第二轮审计（「连续思考合并」修复复查，2026-09-29）

程序员以 `merge_consecutive_thinking`（`peri-tui/src/app/message_pipeline/transform.rs`）替换原 `inject_action_summary`，并入 `messages_to_view_models()` 后处理链。复查结论：**合并主路径（中间只夹只读工具的连续思考）已实现且结构合理，但存在 1 个语义矛盾 bug、1 个边界文本重复、1 个计数遗漏，且合并行为零测试覆盖。**

### R1 [bug] 「非只读工具不打断段」声明与实现矛盾（跨 Bash 的连续思考不合并）

函数头部注释两处自相矛盾：

- 算法说明：「**非只读工具**（Bash/Write/Edit）不参与计数，但**不打断**合并段」；
- 段边界说明：「非『thinking bubble / 只读工具组』的 VM」结束当前合并段。

实现行为取了后者：内层工具扫描遇非只读 `ToolBlock` 即 `break`，`k` 停在 Bash 上，随后的 `is_thinking_bubble(&vms[k])` 判否 → **段在 Bash 处终止**。

复现序列 `Bubble1[Reason] → Bash → Bubble2[Reason]`：两段思考各渲染一行 `Thought`，不合并。

设计 §3.3 改后示例明确跨 Bash 连排（`Thought for 4s` 后连续 4 条 `● Bash(...)`，中间多段思考合并为一行）——与实现不符。**需裁决语义后二选一修正**：

- 若「Bash 不打断」：工具扫描对非只读只跳过不计数（`k += 1` 继续），并补跨 Bash 合并测试；
- 若「Bash 打断」：修正注释，并回写设计 §3.3 的合并跨度描述。

### R2 [bug] 同一 bubble 内多个 Reasoning block 时文本重复渲染

累加阶段 `for b in blocks` 把**所有** Reasoning block 的 text 拼进 `merged_text`；写回阶段 `blocks.iter_mut().find(...)` 只更新**第一个** Reasoning block。其余 Reasoning block 原文保留 → 渲染时首行含全文、后续行重复原文。

触发条件：单条 AI 消息 content 含 ≥2 个 reasoning block（Anthropic 多 thinking block 场景）。写回时应清空/移除段内被合并的其余 Reasoning block，或改为只拼接段内其他 bubble 的 text。

### R3 [bug] 并行工具调用时只读计数遗漏

序列 `Bubble1[tool_calls: Bash+Read] → ToolBlock(Bash) → ToolBlock(Read) → Bubble2[Reason]`：

- 段1 扫描在 Bash 处 break，Read 不计入 Bubble1；
- Read 的 `ToolBlock`/`ToolCallGroup` 直接落入 `out`，也不计入 Bubble2（Bubble2 只扫描**其后**工具）。

结果 `read 1 file` 不出现在任何 Thought 行，Read 成功结果折叠态又不显示内容 → **该次 Read 完全不可见**。与设计「只读工具折叠为计数」的信息保全要求冲突。计数归属规则需定义（如：非只读打断段时，其后的连续只读工具归下一思考段，或归上一段）。

### R4 [缺口] 合并行为零专项测试

- `message_pipeline_test.rs` 保留的是原「单 bubble 计数注入」测试（`test_只读工具计数注入_*`），对单 bubble 段仍绿，但**不覆盖**多 bubble 合并、段边界、文本拼接、`duration_ms` 累加、跨 Bash、计数归属；
- `real_session_replay_test.rs` 为真实旧数据单消息降级测试，同样不覆盖合并；
- 「程序员说修了」目前无测试证据支撑。

### R5 [轻微] `first_recompute` 空占位函数

`fn first_recompute(_blocks: &mut [ContentBlockView]) {}` 无任何逻辑，注释自承「占位以明确意图」。建议删除（意图可由写回代码自解释），避免死代码味道。

### R6 [轻微] `has_ms` 部分缺失时耗时被低估

段内部分 bubble 有 `duration_ms`、部分无（旧数据混入）时，`tot_ms` 只累加已知值且 `has_ms=true` → 显示累加秒数（低估）。更稳的策略：任一段缺失即整体 `None` 退化为 `Thought for N chars`。

### R7 [文档] 注释引用「PRD §2.8」不存在

`merge_consecutive_thinking` 注释写「PRD §2.8」，设计文档只有 §2.1–§2.7，合并规则实际在 §3.3。且设计文档 §8 要求「编码完成后回填实现 vs PRD 差异」，至今未回填（含第一轮 P2-7 的时长来源偏差）。

### 本轮未处理（第一轮遗留，状态不变）

| # | 问题 | 状态 |
|---|------|------|
| P0-1 | Anthropic `parse_content_blocks` 丢 `duration_ms` | 未修 |
| P1-2 | `∴` 前缀未移除 | 未修 |
| P1-3 | spinner 缺 `· thought for {N}s` | 未修 |
| P1-5 | 纯动作行缺失 | 未修 |
| P2-6 | Bash 单行成功输出被吞 | 未修 |
| P2-7 | 时长来源 PRD 回填 | 未做 |
| P2-8 | 进行时态文档标注 | 未做 |

### 第二轮核对通过项

- 主合并路径：`Bubble1[Reason] → Read → Bubble2[Reason] → Read` 正确合并为单行（文本 `\n\n` 拼接、`duration_ms` 累加、计数累加、段内工具 VM 保留原位、段首 bubble 写回）；
- 段边界含可见正文 `Text` 时正确终止（不吞并后续思考）；
- `readonly_action_summary` 单复数（1/N、directory/directories、pattern/patterns）正确；
- `i = seg_end.max(i + 1)` 防死循环，`seg_end` 前进性成立；
- 旧单 bubble 计数测试语义与新实现兼容。

---

## 测试命名规范符合性报告（2026-09-29）

规范来源：`CLAUDE.md` →「测试编写风格」（本次定死）——**测试函数/helper 命名必须全英文 `snake_case`，禁止中文字符与中英混排**，格式 `test_<被测对象>_<场景>`，风格基准 `test_parse_and_reserialize_thinking_with_tool_use`。

### 判定说明（重要）

- **规范生效时点**：英文命名规范于 2026-09-29 本审计中途才写入 `CLAUDE.md`，属**新增规范**。
- **仓库既有惯例**：存量 75 处中文命名（B 表）证明仓库历史长期中英混用；原 `CLAUDE.md` 仅要求「注释、断言消息用中文」，从未禁止中文函数名。
- **本次测试的命名**跟随了各文件既有局部风格（如 `message_pipeline_test.rs` 历史即中文场景命名），**属随大流，不构成本 PR 的违规，不归责于本次改动**。
- **结论**：中文混合命名是**全仓风格债**，应提交独立 `chore/test-naming` PR 统一清理（A 表 24 处 + B 表 75 处一并处理），**不作为本 feature PR 的合入条件**。

### A. 本次改动中的中文命名（风格债，并入 chore PR 清理；非本 PR blocker）

**peri-agent**

| # | 文件 | 现名（违规） | 建议英文名 |
|---|------|-------------|-----------|
| 1 | `messages/content_test.rs` | `test_reasoning_反序列化_旧数据无duration字段` | `test_reasoning_deserialize_legacy_data_without_duration` |
| 2 | `messages/content_test.rs` | `test_reasoning_序列化往返_带duration` | `test_reasoning_serde_roundtrip_with_duration` |
| 3 | `messages/content_test.rs` | `test_reasoning_无耗时序列化不带字段` | `test_reasoning_serialize_omits_duration_when_absent` |
| 4 | `llm/anthropic_test.rs` | `test_anthropic请求体thinking字段集合精确` | `test_anthropic_request_thinking_block_exact_fields` |
| 5 | `llm/openai_test.rs` | `test_reasoning在openai请求中字段集合精确` | `test_reasoning_exact_fields_in_openai_request` |

**peri-widgets**

| # | 文件 | 现名（违规） | 建议英文名 |
|---|------|-------------|-----------|
| 6 | `spinner/mod.rs` | `test_begin_thinking_幂等_重复调用不重置起点与轮次` | `test_begin_thinking_is_idempotent_keeps_origin_and_round` |
| 7 | `spinner/mod.rs` | `test_end_thinking_记录耗时并支持多轮` | `test_end_thinking_records_duration_and_supports_multiple_rounds` |
| 8 | `spinner/mod.rs` | `test_end_thinking_未开始时为幂等无操作` | `test_end_thinking_without_begin_is_noop` |
| 9 | `spinner/mod.rs` | `test_reset_清空思考追踪状态` | `test_reset_clears_thinking_tracking` |
| 10 | `spinner/mod.rs` | `test_thinking_elapsed_ms_不在思考中返回零` | `test_thinking_elapsed_ms_returns_zero_when_not_thinking` |

**peri-tui**

| # | 文件 | 现名（违规） | 建议英文名 |
|---|------|-------------|-----------|
| 11 | `ui/main_ui/main_ui_test.rs` | `test_thinking_status_word_四态判定` | `test_thinking_status_word_state_machine` |
| 12 | `ui/main_ui/main_ui_test.rs` | `test_thinking_heat_color_四档升温` | `test_thinking_heat_color_four_level_warming` |
| 13 | `ui/message_render_test.rs` | `test_bash_非详细模式仅显示前三行` | `test_bash_normal_mode_shows_only_first_three_lines` |
| 14 | `app/message_pipeline/message_pipeline_test.rs` | `test_只读工具计数注入_read_and_grep` | `test_readonly_action_summary_injects_read_and_grep` |
| 15 | `app/message_pipeline/message_pipeline_test.rs` | `test_只读工具计数注入_复数与glob` | `test_readonly_action_summary_plurals_and_glob` |
| 16 | `app/message_pipeline/message_pipeline_test.rs` | `test_bash不参与只读计数` | `test_bash_excluded_from_readonly_count` |
| 17 | `app/message_pipeline/message_pipeline_test.rs` | `test_readonly_action_summary_纯函数` | `test_readonly_action_summary_pure_function` |
| 18 | `app/message_pipeline/real_session_replay_test.rs` | `test_真实旧数据_反序列化不panic且降级` | `test_legacy_data_deserializes_without_panic_and_degrades` |
| 19 | `app/message_pipeline/real_session_replay_test.rs` | `test_真实会话_只读工具注入计数_bash不注入` | `test_real_session_injects_readonly_count_excludes_bash` |
| 20 | `app/message_pipeline/real_session_replay_test.rs` | `test_真实旧数据_渲染管线不panic` | `test_legacy_data_render_pipeline_without_panic` |
| 21 | `app/message_pipeline/real_session_replay_test.rs` | `test_多轮思考合并为一行_累加秒数与计数` | `test_merge_multi_segment_thinking_accumulates_duration_and_counts` |
| 22 | `app/message_pipeline/real_session_full_test.rs` | `test_真实会话_可完整跑通渲染管线不panic` | `test_real_session_full_render_pipeline_without_panic` |
| 23 | `app/message_pipeline/real_session_full_test.rs` | `test_真实会话_多轮思考被合并` | `test_real_session_merges_multi_segment_thinking` |
| 24 | `app/message_pipeline/real_session_full_test.rs` | `test_真实会话_旧数据无duration降级为chars` | `test_real_session_legacy_without_duration_falls_back_to_chars` |

### B. 历史遗留中文命名（存量，同样并入 chore PR）

| 文件 | 违规函数数 | 示例 |
|------|-----------|------|
| `peri-tui/src/app/background_shell_test.rs` | 9 | `test_spawn_stall_watchdog_检测stall并通知` |
| `peri-tui/src/app/background_tasks_panel_test.rs` | 3 | `test_detail_output_大终端显示全部行` |
| `peri-tui/src/app/shell_command_test.rs` | 7 | `test_poll_agent_shells_前台结束不注入后台通知` |
| `peri-tui/src/acp_server/requests_test.rs` | 14 | `test_update_config_切换provider后cfg_provider更新` |
| `peri-tui/src/acp_server/prompt_test.rs` | 4 | `test_strip_leaked_prepends_有历史时剥离头部system消息` |
| `peri-tui/src/event/keyboard/shortcuts_test.rs` | 2 | `test_shortcuts_ctrl_p_与_alt_p_均触发命令面板` |
| `peri-tui/src/ui/message_render_test.rs`（本次 diff 外） | 14 | `test_parse_exit_code_非零退出码`、`test_dim_markdown_lines_空文本` |
| `peri-widgets/src/file_tree_test.rs` | 22 | `test_flatten_展开目录显示子节点` |

> A + B 合计 **99 处**，统一由 `chore/test-naming` PR 清理（规范已在 `CLAUDE.md` 定死，此后新增测试直接走英文）。

### C. 符合项

- Mock/helper 命名 `make_ai_with_tools`、`make_merge_sequence`、`first_action_summary`：全英文 + `make_` 前缀，合规 ✓
- 英文命名的存量测试（如 `test_parse_and_reserialize_thinking_with_tool_use`）合规 ✓
- 注释与断言消息均为中文，符合「注释、断言消息用中文」条款 ✓

### D. 处置建议

1. **本 feature PR 不处理测试改名**（风格债非本次引入，不归责于本次改动）；
2. 另开 `chore/test-naming`：A+B 共 99 处一次性改为英文（建议名已备好），只动函数标识符，注释/断言中文不变；
3. 新规范生效后（2026-09-29 起）**新增**测试必须全英文，由 code review 把关。

---

## 第三轮审计（全量修复复查，2026-09-29）

程序员宣称完成修复。本轮逐条复验前两轮全部问题 + 新增回归测试 + PRD 回填。**结论：前两轮 11 项问题中 11 项已修复或补齐，剩余 3 个轻微问题（1 个降级路径 bug、1 个调试残留、1 个测试缺口）。**

### 修复核验（逐条）

| 轮次 | 问题 | 状态 | 证据 |
|------|------|------|------|
| P0-1 | Anthropic `parse_content_blocks` 丢 `duration_ms` | ✅ 已修 | `invoke.rs:249-260` 直接构造 `Reasoning{duration_ms: b["duration_ms"].as_u64()}`，流式→解析链路贯通 |
| P1-2 | `∴` 前缀未移除 | ✅ 已修 | `message_render.rs` 渲染改为单 `Span::styled(title)`，无 `∴ ` |
| P1-3 | spinner 缺 `· thought for {N}s` | ✅ 已修 | `thinking_status_word` 增第 4 参 `last_thought_ms`，非思考段且有已结束思考时返回 `thought for Ns`；测试 `test_thinking_status_word_four_states` 覆盖状态② |
| P1-5 | 纯动作行缺失 | ✅ 已修 | `ToolCallGroup.standalone_action` 字段 + `merge_consecutive_thinking` 对前置非 thinking 的只读组注入计数；渲染折叠态显示 |
| R1 | 跨 Bash 不合并（注释/实现矛盾） | ✅ 已修 | 非只读工具改为 `_ => k += 1`（跳过不打断）；PRD §2.8 明确「非只读工具**不断开**，仅不计数」，注释与实现一致 |
| R2 | 同 bubble 多 Reasoning 文本重复 | ✅ 已修 | 写回后 `blocks.retain` 仅保留第一个 Reasoning，其余移除；测试 `test_merge_removes_extra_reasoning_blocks_in_same_bubble` |
| R3 | 并行工具计数遗漏 | ✅ 已修 | 非只读跳过后后续 Read 仍被扫描计入；测试 `test_parallel_tools_read_counted_alongside_bash` |
| R4 | 合并行为零测试 | ✅ 已补 | `real_session_replay_test.rs` 新增 4 个合并专项测试 + `real_session_full_test.rs` 真实会话回放测试；`cargo test -p peri-tui --lib message_pipeline` 93 passed |
| R5 | `first_recompute` 空占位 | ✅ 已删 | 函数已移除 |
| R6 | 部分缺耗时被低估 | ✅ 已修 | `missing_ms` 标记任一段缺耗时 → 整段 `duration_ms=None` 降级；测试 `test_partial_missing_duration_degrades_whole_segment` |
| R7 | 注释引 §2.8 不存在 | ✅ 已修 | PRD 补 §2.8「连续多轮合并为一行」（含合并规则表、与 §2.3 关系、待确认项）；§8 实现回填已写 |
| P2-6 | Bash 单行输出被吞 | ✅ 已修 | `content.contains('\n')` → `!content.is_empty()` |
| P2-7 | PRD 时长来源回填 | ✅ 已做 | PRD §8.1：LLM 流式层计时 + 持久化，restore 后仍可用，偏差已声明 |
| P2-8 | 进行时态文档标注 | ✅ 已做 | PRD §8.3 未实现项表明确标注「本次暂不纳入」 |
| — | 测试命名中英混排 | ✅ 已改 | 触及文件内全部改为英文（`test_reasoning_deserialize_legacy_data_without_duration` 等）；历史文件未动（按约定走 chore PR） |

### 测试执行结果

| 测试集 | 结果 |
|--------|------|
| `peri-widgets` spinner | 16 passed |
| `peri-agent` 全量 lib | 516 passed |
| `peri-tui` message_pipeline（含合并专项） | 93 passed |
| `peri-tui` message_render | 58 passed |
| `peri-tui` main_ui（含四态/热度） | 58 passed |

### 剩余问题（轻微，不阻塞）

**N1 [bug·降级路径] 合并写回后 `char_count` 未同步**

`transform.rs:339` `*text = merged_text` 时 `char_count` 仍为段首 bubble 的旧值（未参与解构写回）。当 `missing_ms=true` 降级为 `duration_ms=None` 时，渲染走 `Thought for {char_count} chars` 回退分支，显示的是**段首字数**而非合并后全文的字数（低估）。触发条件：合并段内混入旧数据（无 `duration_ms`）的 thinking。修复：写回时同步 `*char_count = merged_text.chars().count()`，或渲染时用 `text.chars().count()`。

**N2 [卫生] 调试残留测试 `tmp_print_real_merge_result`**

`real_session_full_test.rs:122`：`tmp_` 前缀、无断言、仅 `eprintln!` 打印合并结果。属调试脚手架，不应进主干。建议删除（正式断言已由 `test_real_session_multi_round_thinking_merged` 覆盖）。

**N3 [测试缺口] P0 修复无正向回归测试**

`parse_content_blocks` 现在会保留 `duration_ms`，但 `anthropic_test.rs` 仅有「请求体**不含** duration_ms」的防泄漏测试，**没有**「解析后 `duration_ms` 被保留」的正向测试。建议补：喂含 `"duration_ms":4200` 的 raw thinking block → 断言 `Reasoning.duration_ms == Some(4200)`。

**N4 [cosmetic] `test_begin_thinking` 命名丢失场景描述**

改英文后名字过简（原「幂等_重复调用不重置起点与轮次」信息丢失）。建议 `test_begin_thinking_idempotent`。

### 本轮核对通过项

- 合并主路径（多 thinking + 只读工具）、跨 Bash 合并、段边界（可见文本断开）、`duration_ms` 累加/降级、单复数计数、纯动作行、Bash 前 3 行摘要（含单行）、`still thinking` 优先级、热度四档——均有实现与测试对应；
- `standalone_action` 的防重复判定（`is_thinking_bubble_with_summary`）逻辑正确：已被合并计数的工具组不重复显示；
- 所有触及文件测试命名全英文，符合新定规范；
- PRD §2.8 与实现语义一致（跨 Bash 合并、秒数累加），待确认项已诚实标注。

---

## 第四轮审计（N1–N4 收尾复查，2026-09-29）

程序员完成第三轮遗留 4 项修复。逐条复验 + 定向跑测试。**结论：N1–N4 全部修复，代码侧无遗留问题。**

### N1–N4 核验

| # | 问题 | 状态 | 证据 |
|---|------|------|------|
| N1 | 合并写回后 `char_count` 未同步 | ✅ 已修 | `transform.rs:333-343` 新增 `merged_char_count = merged_text.chars().count()` 并写回；配套测试 `test_merged_char_count_reflects_all_segments_on_fallback`（降级路径下字数反映全部段） |
| N2 | 调试残留 `tmp_print_real_merge_result` | ✅ 已删 | `real_session_full_test.rs` 仅剩 3 个正式断言测试，无 `tmp_` 函数 |
| N3 | P0 修复无正向回归测试 | ✅ 已补 | `test_parse_content_blocks_keeps_thinking_duration`（含 `duration_ms:4200` 的 raw block 解析后保留）+ `test_parse_content_blocks_missing_duration_is_none`（旧数据 → None 兼容） |
| N4 | `test_begin_thinking` 命名丢场景 | ✅ 已改 | `test_begin_thinking_is_idempotent` |

### 定向测试执行结果

| 测试 | 结果 |
|------|------|
| `test_parse_content_blocks_keeps_thinking_duration` | ok |
| `test_parse_content_blocks_missing_duration_is_none` | ok |
| `test_begin_thinking_is_idempotent` | ok |
| `test_merged_char_count_reflects_all_segments_on_fallback` | ok |
| `peri-agent` anthropic 模块（51 passed） | ok |
| `peri-tui` message_pipeline（93 passed，含合并专项） | ok |

### 第四轮核对通过项

- N1 修复位置正确：仅在合并写回路径更新 `char_count`，流式 bubble 构造路径不受影响；
- N3 测试有区分度：正向（保留 4200）+ 反向（旧数据 None）双向锁死链路；
- `real_session_full_test.rs` / `real_session_replay_test.rs` 测试命名全部符合新规范（全英文）；
- 无新增改动引入的回归。

### 遗留（非本次改动范围，仅记录）

| 项 | 说明 |
|----|------|
| 历史中文测试命名（75 处） | 按约定走独立 `chore/test-naming` PR |
| 进行时态 `Reading 1 file…` | PRD §6-7 / §8.3 明确「本次暂不纳入」 |
| 配色阈值 15s/30s、`thinking more` 判定 | 待实测校准（PRD §8.3） |

---

## 第五轮：PRD 文档齐全性审计（2026-09-29）

审计对象：`2026-09-29-thinking-status-line-and-tool-summary.md`（主 PRD）及其配套文档集。**结论：规格内容齐全度约 90%，主要问题不在"缺内容"而在状态同步滞后与收尾文档缺失。**

### 文档集与结构覆盖

- **载体齐全**：主 PRD（601 行）+ 本审计文档 + 效果稿 3 份（`render-effect-mock.md` / `-mock-dynamic.md` / `-preview.html`）。
- **结构覆盖**：✅ 背景诉求、需求规格、决策记录（§2.6）、差异总表（§4）、实现路径（§5）、开放问题（§6）、代码锚点（§7，逐条核验）、实现回填（§8）——骨架完整，修订轨迹诚实（多次记录「已作废推断」）。

### 缺口（4 处）

| # | 问题 | 位置 | 严重度 | 处置 |
|---|------|------|--------|------|
| 1 | **状态头过期**：头部写「设计评审中（未开始编码）」，§8.4 却写「编码已完成（含审计修复）」——自相矛盾 | PRD 头部 vs §8 | 🔴 必修 | 提 PR 前更新状态头 |
| 2 | **§2.8 两个「待确认项」未收敛进 §6**（秒数累加 vs 跨度、合并是否跨 Bash）——第三轮已裁决「跨 Bash 合并、秒数累加」并实现，但 §2.8 待确认项与 §6 开放问题清单未同步勾销 | PRD §2.8 vs §6 | 🔴 必修 | 将裁决结果回填两处 |
| 3 | **无验收标准/测试计划**：审计中反复出现「合并行为零测试」「测试盲区」，根因是 PRD 未定义验收口径 | PRD 全文 | 🟡 建议 | 可引用本文档「测试执行结果」作为事实上的验收记录 |
| 4 | **无文档联动与回滚说明**：`duration_ms` 随消息持久化是数据格式变更（§8.1），无兼容/回滚说明；按 `CLAUDE.md` 文档维护规则，本功能属用户可见变更，需 CHANGELOG / README 特性表 / TUI-STYLE.md 联动清单——当前工作区均未见修改 | PRD §8.1 | 🟡 建议 | 提 PR 前补齐联动清单 |

### 合规项 ✓

- 分支名 `feat/thinking-status-line` 符合仓库命名规则；
- §8.5 已声明锚点行号为编码前快照；
- §4 表 #10/#11 明确「已对齐勿改」，防止误改；
- 开放问题 11 条中 3 条已划线裁决，追踪清晰。

---

## 第六轮：反思性盲区审查（2026-09-29）

背景：功能已 commit（`9191194f` 主功能 + `f1c1db01` 截断提示追加引导语）。本轮不复审已闭环项，专找前五轮**方法论盲区**（口径一致性、数据资产、文档集内部矛盾、发版流程）。**发现 5 项，其中 2 项建议提 PR 前处理。**

### F1 [风险·数据] `fixtures/session_01a0ebee.json` 含内网基础设施信息，已随 commit 入库

- **事实**：284KB 真实用户会话 dump 已提交（`9191194f`）。扫描结果：
  - `API_KEY`/`PASSWORD` 字样均为**键名**（如 `ANTHROPIC_API_KEY = 未设置`、grep 模式串），**无真实密钥值** ✓；
  - 但含 **内网 VPN 域名 `vpn.example.com`**（来自真实会话里 `.env` 的 `DB_HOST`），以及用户真实工作项目路径/代码片段（`acme_order` 业务代码）。
- **风险**：若 `cc-claws/cc-code` 仓库可见范围超出本组织，属内部基础设施信息外泄面。mock 文档（`render-effect-mock.md`）同样含该域名。
- **建议**：① 确认仓库可见性；② 若公开或可公开，将 fixtures 中域名脱敏（如 `vpn.example.com`）或替换为合成会话；③ mock 文档同步脱敏。测试对内容本身不敏感（只测管线行为），脱敏不影响测试有效性。

### F2 [bug·口径] spinner 与消息区「thought for Ns」两处数值口径不一致

| 位置 | 取整 | 示例（3200ms / 单段） |
|------|------|---------------------|
| `message_area.rs:45`（spinner 尾字段） | `last_thought_ms / 1000` **向下**取整 | `thought for 3s` |
| `message_render.rs:1047`（消息区 Thought 行） | `ms.div_ceil(1000)` **向上**取整 | `Thought for 4s` |

- PRD §2.8 明确写「与 spinner 的 `· thought for Ns` 一致」——**实现违反了自己定的规格**。同一思考段 spinner 与消息区秒数可差 1s。
- **语义差（相关问题）**：spinner 取 `last_thought_ms`（**上一单段**耗时），消息区合并后取**多段累加**。即使取整统一，多轮合并场景下两处数字也**必然不同**（spinner 定格于最后一段，消息区显示总和）。PRD §2.8「一致」措辞本身不严谨。
- **建议**：① 取整统一（建议都用 `div_ceil`，>0 至少 1s）；② PRD 措辞改准确：「格式一致」而非「数值一致」，或明确 spinner 为单段、消息区为合并段语义。

### F3 [文档·矛盾] 效果稿 mock 仍写 `still thinking` 阈值 60s，与 PRD 裁决 10s 矛盾

- `render-effect-mock.md:169,176`、`render-effect-mock-dynamic.md:89-116` 仍为「60s 阈值」。
- PRD §2.2/§2.6 已裁决**默认 10s 可调**（且 mock 静态稿自身第 176 行还写「决策 §2.6」引 60s，与 PRD §2.6 实际内容矛盾）。
- 效果稿头部有「预演不保证一致」免责，但**阈值是决策事实**不是观感细节，留 60s 会误导后续实测校准的人。
- **建议**：mock 两处改为 10s 或加「阈值以 PRD §2.6 为准（10s），本稿 60s 为早期草稿」注记。

### F4 [流程] CHANGELOG / README 特性表 / TUI-STYLE.md 未联动（第五轮缺口 4 的强化）

- 功能已 commit，但 `CHANGELOG.md` 最新条目仍是 v0.6.81（2026-09-28）；本功能是**用户可见变更**（spinner 新状态、消息区格式、Bash 输出摘要、截断提示文案变更）。
- 项目 `CLAUDE.md` 文档维护规则：**每次发版必改 CHANGELOG**；README 双语特性表「新增用户可见功能时」同步；TUI-STYLE.md「改快捷键/交互」时更新——截断提示新增 `(ctrl+o to expand)` 引导语也属交互文案变更。
- **建议**：发版（npm-v tag）前补齐三处；至少 CHANGELOG 必须有本条目。

### F5 [流程·轻微] commit 署名不符仓库规范

- 两个 commit 的 `Co-Authored-By` 为 `mimo-x-pro-preview <XiaomiMiMo@cc-code>`，与用户规范要求的 `Co-Authored-By: Claude <noreply@anthropic.com>`（或真实模型名）不符。
- 已提交的历史 commit **不建议改写**（会重写哈希）；仅记录，后续 commit 按规范署名即可。

### 反思维度（本轮方法论补充）

前五轮聚焦「实现 vs PRD」与「命名规范」，漏了四类维度，建议后续审计 checklist 固化：

1. **口径一致性**：同一概念（秒数、计数、阈值）在多处渲染时的取整/语义是否统一；
2. **数据资产**：新增 fixtures/测试数据是否含内网信息、真实凭据面（键名 vs 值）、用户隐私；
3. **文档集内部矛盾**：PRD 裁决变更后，mock/预览稿等派生文档是否同步（尤其数字类决策）；
4. **发版联动**：代码合入 ≠ 完成，CHANGELOG/README/交互文档的联动是 CLAUDE.md 强规则。

---

## 总结

| 轮次 | 结论 |
|------|------|
| 第一轮 | 骨架符合设计，P0×1 + P1×4 + P2×3 未达标 |
| 第二轮 | 「连续思考合并」主路径落地，但 R1 语义矛盾、R2 文本重复、R3 计数遗漏、R4 零测试 |
| 第三轮 | 前两轮 11 项全部修复/补齐；剩 N1 char_count、N2 调试残留、N3 正向测试、N4 命名 |
| 第四轮 | N1–N4 全部修复，测试全绿。代码侧无遗留问题 |
| 第五轮 | PRD 文档齐全性：状态头过期、§2.8/§6 未同步 2 项必修，验收口径、文档联动 2 项建议 |
| 第六轮 | 反思盲区：F1 fixtures 内网域名、F2 秒数口径/语义不一致、F3 mock 60s 过时、F4 CHANGELOG 联动、F5 署名——**F1/F2 建议提 PR 前处理，F3/F4 发版前必做** |

**最终建议**：F1（fixtures 脱敏）+ F2（秒数口径统一）→ 补 F4 文档联动 → 建 GitHub Issue 走 PR 流程。
