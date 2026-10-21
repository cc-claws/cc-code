# Issue #629：实现与收益验证

日期：2026-10-21。状态：实现与本地验证完成，通过 PR 关联 #629；尚未发布，不手动修改 GitHub Issue 状态。

## 结论

以当前 npm 线上版 **v0.6.168 / 60e3f795dbb8072677915bda5d96c2c9c0d54fee** 为基线，完成分段视口渲染、预算淘汰与内容锚点。默认 auto 已验证能对长历史升档，cache 保留兼容路径。开发位于 `feat/viewport-render-cache` 独立 worktree，没有覆盖主工作区的其他任务。origin/main 比此线上 tag 多出的两条提交仅为已发布版本文档回补，不涉及运行时代码。

## 可复现收益

Windows，本机 Rust 1.99.0 debug 构建，120 列正文、40 行视口。每 10 条消息包含 1 条 User、2 条 Markdown/Reasoning、4 条各 400 行 Bash、2 条各 120 行 Read、1 条双态系统提示。走真实 RenderEvent → worker → RenderCache 发布路径，不访问模型或外部业务服务。cache 是原全量渲染兼容路径，virtual 是本次新路径；不是拿空 mock 与完整渲染比较。

计时从事件入队到缓存版本发布，**不含夹具构造、App 全量 VM 快照复制、真实终端绘制、LLM 调用**。切换耗时为单次样本，不能当线上 p95 或 release 性能承诺；窗口遍历另行给出分布。所有数值单位均为毫秒。

| 消息数 | 详情冷开 cache → virtual | 详情热开 cache → virtual | 追加消息 cache → virtual |
|---:|---:|---:|---:|
| 100 | 1674.14 → 42.70 | 13.14 → 0.51 | 18.93 → 1.59 |
| 500 | 8457.66 → 43.38 | 65.98 → 0.53 | 96.55 → 2.39 |
| 1000 | 16898.15 → 42.34 | 138.43 → 0.60 | 192.79 → 3.30 |

1000 条消息：冷开约 **399 倍**、热开约 **232 倍**、追加约 **58 倍**。两路径自身的「折叠初始=关详情」「详情冷开=热开」文本/样式/wrap 指纹断言通过；小窗口与原 renderer 的逐行/样式/换行等价另由单元测试验证。虚拟全局行数含离屏估算，不能拿它当全部历史已精确测量的行数。

### 默认 auto 与淘汰

- auto / 1000 条：冷开 42.64 ms、热开 0.68 ms、追加 3.20 ms，实际发布 virtual 元数据。
- auto / 2000 条：冷开 43.45 ms、热开 0.81 ms、追加 5.09 ms；全局估算约 34.3 万行，窗口成品仍为 420 行。
- virtual / 1000 条遍历 100 个不同 Bash 窗口：首次物化 p50 41.48 / p95 43.16 ms；驻留窗口再滚一行 p50 0.43 / p95 0.55 ms。发生 42 次淘汰，驻留峰值估算 16,658,670 B，低于 16,777,216 B 预算。
- auto / 2000 条遍历 200 个窗口：发生 142 次淘汰，驻留峰值估算 16,686,740 B；普通分段不会随累计访问历史无限增长。

### 内存口径

1000 条详情冷开时，共享发布表估算 **70.54 MiB → 0.206 MiB，减少 99.7%**。新路径还另有约 0.253 MiB 驻留分段与约 28 KiB 的发布元数据，不能把 0.206 MiB 当总缓存或进程内存。遍历后驻留分段可增长至预算附近。

16 MiB 是渲染成品估算预算，并非 RSS 上限；不包含原始历史/VM、预解析 Markdown 与其他全局缓存、元数据、剪贴板最终字符串、旧快照引用和分配器。可见巨型消息按段原子渲染且被 pin，可突破预算，单消息内切片/临时峰值治理仍属于 #627。没有以最大分段大小冒充真实临时堆峰值。App 在 1000 条时仍有约 5.5 ms 的全量快照复制，该项没有被本次优化消除。

## 实现与分工

- 主代理：`ui/render_layout/{mod,estimator,layout_test}.rs`，`ui/render_thread{.rs,/worker.rs}`，收益探针 `examples/issue629_render_probe.rs`。Fenwick 高度定位、分段不可变成品、共享字节 LRU、detail/diff 热变体复用、只克隆当前窗口、保序合并 Resize/Viewport；主题换代按进入视口的消息惰性作废预解析样式。`subagent_session.rs` 补立即取消旧复制。
- 配置/兼容子任务：`cc-acp/src/provider/config{,_test}.rs`、`app/{chat_session,config_panel,config_panel_test,panel_config,panel_ops,mod,thread_ops}.rs`、配置面板、本地化与使用文档；新增 `ui/render_thread/worker_test.rs` 验证 auto 升档/历史重判、策略往返释放、Clear/回滚与控制事件顺序。
- 交互/选区子任务：`event/{mod,viewport_interaction_test}.rs`、`app/{agent_render,message_state,text_selection,text_selection_test,ui_state,thread_ops}.rs`、`ui/main_ui/message_area{,_test}.rs`、`ui/render_layout/interaction{,_test}.rs`。缺页期间保留已显示内容，鼠标按实际呈现窗口命中；逻辑消息/原始行/字素锚点支持跨淘汰复制，复制异步且有代次校验；旧发布不能覆盖最新滚轮目标，拖滚动条期间固定比例。
- `/gc` 分开显示当前发布表和虚拟驻留预算/淘汰/最大分段估算；未改系统提示词、ACP 协议或原始对话。

auto 在估算详情 ≥300000 行或估算成品 >64 MiB 时升档，同一历史代际保持升档，历史替换后重判。`/config` →「详情渲染」可随时切换 auto/cache/virtual，保存为 `config.detail_render_strategy`。宽度/主题改变释放旧布局；detail/diff 的不同成品共用预算，热切换可复用。

## 复现命令

在本 worktree 执行（target-dir 可换成本机路径）：

```powershell
cargo build --locked --offline -p cc-tui --example issue629_render_probe --target-dir D:\code\peri\target
$env:PERI_RENDER_PROBE_STRATEGY='cache'
& 'D:\code\peri\target\debug\examples\issue629_render_probe.exe' 100 500 1000
$env:PERI_RENDER_PROBE_STRATEGY='virtual'
& 'D:\code\peri\target\debug\examples\issue629_render_probe.exe' 100 500 1000
$env:PERI_RENDER_PROBE_STRATEGY='auto'
& 'D:\code\peri\target\debug\examples\issue629_render_probe.exe' 1000 2000
```

原始结果：[cache](issue-629-render-cache.json)、[virtual](issue-629-render-virtual.json)、[auto](issue-629-render-auto.json)。JSON 保留原始 u64 指纹，未通过 JavaScript 数值解析后重写。

## 最终验证

- TUI crate 全部库单测：**1432 通过，0 失败，11 跳过**，84.41 秒。
- ACP crate 全部库单测：**176 通过，0 失败，0 跳过**。两项合计 1608 通过，不代表整个 workspace 全量测试。
- `cargo clippy --locked --offline -p cc-tui -p cc-acp --all-targets --target-dir D:\code\peri\target -- -D warnings`：通过，包含测试及收益探针的编译检查。
- `cargo fmt --all -- --check`、`git diff --check`：通过。

TUI 使用以下命令运行，ACP 使用本轮编译完成的 `cc_acp-45a63a03fb2280fe.exe --test-threads=1 --format terse` 运行完整库测试：

```powershell
cargo test --locked --offline -p cc-tui --lib --target-dir D:\code\peri\target -- --test-threads=1 --format terse
cargo test --locked --offline -p cc-acp --lib --target-dir D:\code\peri\target -- --test-threads=1 --format terse
```

回归覆盖原 renderer 逐行/样式/wrap 等价、170000 行历史定位、预算淘汰后跨段 Unicode 复制、主题惰性刷新、自动策略升档与重判、Clear/历史替换后的 URL 和选区清理、Resize 后控制事件不丢失、流式更新导致复制端点版本变化时拒绝过期结果。真实无头终端测试还运行实际渲染线程与 45 次鼠标滚轮事件，确认新目标内容进入终端缓冲区、旧帧被替换、缺页期间不显示空白；这不是人工终端验收。

## 未覆盖

没有运行真实模型长会话、人工鼠标验收、release 构建性能压测、Linux/macOS 实机验证，也没有执行生产环境写操作。回退方式为将本地配置设为 cache；提交 PR 不包含发版、外部通知或合并操作。
