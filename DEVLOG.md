# Perihelion Devlog — March 20 – May 20, 2026（附 cc-code 时代番外）

## March 20 · Hello World

项目第一行代码诞生。agent 框架的骨架开始搭建——ReAct 循环、BaseMessage 消息类型、中间件 trait 雏形。这一天只有一个 commit：把想法变成代码。

---

## March 21 · Agent 定义文件

Agent 配置文件读取能力上线。从这一天起，每个 agent 都可以用自己的 `.claude/agents/` 文件定义行为——model、maxTurns、tools 白名单。

---

## March 22 · SubAgent 诞生 / 存储层设计 / UI 重构

SubAgent 能力第一天就完成了——子 agent 通过 `Agent` 工具触发，继承或覆盖父 agent 的工具配置。

底层存储层（SQLite 持久化）也在今天敲定设计。TUI 前端经历了一次架构重构。**三条线并进，像是第一天铺了三根平行的铁轨。**

---

## March 23 · Remote Control / Headless / 工具展示

Remote Control 面板完工——可以远程连接和控制 agent。Headless 模式就位——不启动 TUI，纯后台运行。工具调用的视觉展示迎来第一次优化。

**今天的关键决策**：HITL 拒绝不再终止 Agent，改为反馈错误结果让 LLM 调整——这个设计选择影响了后面无数轮工具的交互逻辑。

---

## March 24 · Relay Server 样式 / Langfuse 接入 / Compact 指令

Relay Server 的 Web 前端样式更新了一版。Langfuse 追踪集成第一次接通。图片上传支持、markdown 渲染加入。`/compact` 指令诞生。

也是修复日：Bash 超时自动 SIGKILL、WebSocket HITL 补齐 4 种决策路径、渲染 channel 从有界改无界消除静默丢弃、文件路径 canonicalize 防路径遍历……

---

## March 25 · Langfuse 集成 / Web Crawler / 零 Clippy 警告

Langfuse 深度集成——token 统计、session 管理、工具定义上报全链路打通。Web Crawler 能力加入（后来演变成 WebFetch/WebSearch 中间件）。SubAgent 开始共享中间件链。

毫不动摇地把全 workspace 的 clippy 警告清零。**从今天起，"零警告"成为了基线。**

---

## March 26 · Relay Server 加固 / 远程控制面板 / Skill Preload

Relay Server 补了连接数上限防 DoS、spawn 错误可观测性、四处安全漏洞修复。远程控制面板的 TUI 端就位。`#skill-name` 预加载机制上线。

今天还修了 3 处 unwrap panic 风险、2 处 Lagfuse 追踪质量漏洞、1 处 JoinHandle 泄漏。**安全加固日——每一个潜在 crash 都被提前堵上。**

---

## March 27 · 架构债清偿 / Relay 数据同步 / 移动端适配

Arch.md 中标记的 M1/M2/M3/M4 架构问题全部修复。Relay Server 完成了数据同步、弹窗、命令行传递三件套。前端从 React 迁移到 Preact。移动端适配完成。

也是修复日：DashMap 锁跨 await 反模式、WebSocket 消息限制/心跳/超时/字段校验、内存无界增长防护……

---

## March 28 · 跨平台构建 / 交互统一 / Skill 预加载

Cross 编译问题大决战——macOS/Linux 构建修复、rustls-tls 替代 native-tls 消除 OpenSSL 依赖、cross-rs 入 CI。TUI 交互统一化——Skill 发送后自动预加载全文。Relay Server 协议层解耦支持多用户。

样式重构完成。ThreadStore 与 AgentState 合并。

---

## March 29 · peri-cli 发布 / Langfuse Client / 大重构

peri-cli 正式发布——Node.js 安装/更新/卸载工具独立出去。Langfuse Client 作为独立 crate 完成。Cron 面板开发完毕。

Sticky header 上线、颜色系统微调、配置文件 env 注入就位。YOLO 模式设为默认。**今天还是一个"大扫除日"——基础代码大重构，历史遗留代码清理。**

---

## March 30 · Langfuse Client 独立 / Cron 完成 / 颜色收尾

Langfuse Client 作为库独立发布。`/cron` 命令完成。颜色收尾微调。Sticky header 完工。相对平静的一天。

---

## March 31 · Setup 向导 / 历史筛选 / Langfuse 修复

Setup 初始化向导设计完成。历史对话筛选功能上线。Langfuse 的层级关系和 warning 细节修复。OTel 相关代码移除。

---

## April 27 · 组件库诞生 / Token 计数 / 权限模式 / Relay 移除

**休息了近一个月后重新开工**。

`cc-widgets` 组件库诞生——独立 TUI 组件只依赖 ratatui + pulldown-cmark。Token 计数和自动压缩功能就位。权限控制模式开发完成——Bypass/Default/AcceptEdit/DontAsk/AutoMode 五种模式。模型选择面板重构。Relay Server 代码移除以简化架构。

---

## April 28 · 消息管线统一 / 压缩完成 / LLM 重试

消息渲染管线彻底统一——`messages_to_view_models` 成为唯一转换入口。压缩（Compact）能力验收完成——micro + full 两层架构就位。LLM 自动重试机制上线。Markdown 表格自适应换行、Spinner/ToolCall/MessageBlock Widget 化、代码块语法高亮用 syntect 实现。输入框历史恢复、浮层 Up/Down 导航。

---

## April 29 · 四十二个修复的马拉松

**当天 48 个 commit，是整个项目最密集的修复日。**

从 SubAgent 栈泄漏到 write_file 并发安全，从 executor 竞态到 HITL 批次一致性，从 SQLite 外键到 ContentBlock::Unknown 序列化透传——42 个逻辑修复覆盖了消息管线、工具系统、安全、UX 的方方面面。

Auto-classifier 的 ALLOW/DENY 否定词误判修复尤其关键——一个 bug 差点让危险命令自动放行。

---

## April 30 · 系统提示词重构 / Prompt Cache 修复 / ContextBudget 连接

系统提示词大重构——从单块文本拆成 11 个段落文件，引入 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` 边界标记分隔缓存/非缓存段。Anthropic Prompt Cache 的 `cache_control` 位置修复。ContextBudget 连接到 executor 上下文监控。

批量 `before_tool` 处理优化让 HITL 可以一次审批多个工具。工具名与参考实现原生界面彻底对齐。

---

## May 1 · 修修补补的仪式感

五一劳动节没休息——但是干的都是"舒服的活"。给 Thread Browser 加了删除确认、空列表引导、新建反馈，把颜色系统对齐了 Claude Dark Theme，模型面板塞进了 thinking effort 旋钮。CJK 截断修了又修（这已经是第 N 次被字节/字符混淆坑了）。**今天像是给房子做大扫除——每个角落都擦了一遍，住着舒服多了。**

---

## May 2 · MCP 降临

重头戏：MCP 中间件落地了。外部 MCP 服务器的工具和资源现在可以直接注入对话。ACP 协议也通了第一版。

Thread Browser 彻底重设计——搜索框、两行紧凑列表、内容大小一目了然。还顺手清了全项目的编译警告，补了 widgets 和 TokenTracker 的测试。**今天是"打下两根大桩"的日子——MCP 和 ACP 让 Perihelion 对外部生态张开了双臂。**

---

## May 3 · SubAgent 大爆炸

一天之内，三个重量级能力同时上线：Background Agent（后台执行 + 自动 continuation）、Fork Agent（继承上下文分叉）、多 session 分屏。Alt+M 还能循环切模型了。

peri-dag 工作流引擎也在今天诞生——带 Web UI 的 DAG 编排器。**这一天像是 feature 工厂开足马力，SubAgent、多 session、dag 引擎三线并进，气势很猛。**

---

## May 4 · 疯狂的设计评审马拉松

45 个 commit，整个 sprint 最密集的一天。

acpx-g（原 peri-dag）经历了 **Round 1 到 Round 22+** 的设计评审——从 UX 打磨到架构加固，从 schema 校验到并发限流器，从前端安全到 CSS 变量化。系统提示词也被大刀砍了 51%（281→137 行），把依赖收敛到根 Cargo.toml。rusqlite 迁移到 sqlx，Ctrl+C 中断后能恢复输入文本了。**今天是"量变到质变"的一天——acpx-g 从一个想法变成了一个经得起审视的系统。**

---

## May 5 · 前端炼狱与网络之眼

acpx-g 的设计评审从 Round 2 一路杀到 Round 22——事件委托、内联 handler 消灭、undo/redo 防腐败、CSS 变量替换硬编码色……每一天都在把前端代码从"能跑"推向"体面"。

同时 WebFetch/WebSearch 中间件上线，peri-cli 支持多包安装，双击 Ctrl+C 退出成为了肌肉记忆。**今天大概是这个 sprint 里最像"debug 地狱"的一天——但 acpx-g 终于在审美的意义上站稳了脚跟。**

---

## May 6 · 插件商店开张

插件系统正式兼容参考实现的 Marketplace。MCP 中间件的四阶段流水线重构完成。Marketplace 面板有了安装量排序、自动刷新、状态持久化。

**Perihelion 从此有了自己的"应用商店"。这一天像一个里程碑——生态的种子播下了。**

---

## May 7 · Hook 全家桶与零警示

Hooks 系统完整实现（对齐参考实现）——4 种执行类型、14 种事件、SSRF 防护。全项目 clippy 警告清零。ContentBlock 测试补全，MCP 工具名 sanitize 到 API 规范。

**如果 May 6 是"开门"，May 7 就是"铺路"——hooks 让插件有了和宿主对话的完整语言。**

---

## May 8 · 架构瘦身日

TUI 层大重构——PanelComponent trait 组件化、core.rs 拆解为独立模块。Setup Wizard 升级了多 provider 迁移和 Browse/Edit 双模式。

**身体变轻了，姿态更好了。从这以后，加新面板只要实现一个 trait。**

---

## May 9 · 改名与延迟加载

`.zen-code` 正式更名为 `.peri`。Tool Search 延迟加载上线——非核心工具按需发现，不再拖慢启动。built-in agents（explore、plan、general-purpose、verification）就位。executor.rs 拆成目录模块。

**今天是"重命名日"——但更重要的是，工具系统从"全家桶"变成了"自助餐"。**

---

## May 10 · 思考的细节

OpenAI 兼容 provider 的 reasoning_content 回传机制踩了一圈坑——DeepSeek thinking 模式、extended thinking 验证、模型能力条件回传。status bar 的计时器换成了进程内存监控。鼠标可用性检测让键盘党也能看到快捷键提示。

**今天在处理"看不见的东西"——token 在网络上来回时，每一个字段的位置都决定了下一个请求能不能命中缓存。**

---

## May 11 · 编译引擎与渲染革命

LSP 中间件落地——10 种代码智能操作。auto-compact 两层架构（micro + full）就位。TUI 渲染管线统一为 RebuildAll 架构——增量渲染的复杂性和各种边界 bug 终于画上句号。agent_ops.rs 从 2158 行拆成 7 个模块。

**今天做了一件勇敢的事——扔掉一个复杂的增量系统，换一个更简单的全量重建。代码行数没变少，但心智负担降了一半。**

---

## May 12 · Anthropic 缓存周 · 第一天

**这是整个 sprint 最高光的一天。**

Anthropic prompt cache 命中率从 ~70% 飙升到 98.5%+——3 断点缓存策略。背后是 cache_control 位置排序、tools 前缀稳定化、skill 预加载改用 add_message、system prompt 边界标记分离缓存块……一堆细节纠缠。

同时修了 3 个 background agent 的展示 bug、SubAgent 事件泄露、CJK 鼠标定位（第三次）、pipeline 切片 panic、compact 后的残留通知。还加上了缓存命中率在 status bar 的实时显示。

**今天像是在和 Anthropic 的缓存机制下棋——每一步都有意为之，最终赢了。98.5% 不是优化，是逆袭。**

---

## May 13 · 缓存稳定与流式新生

缓存战役从"命中率"打到"稳定性"——跨进程工具排序、deferred tools 会话级缓存、fork agent 的缓存破坏问题逐一解决。

流式渲染迎来增量 markdown 解析和前缀缓存复用。多 SubAgent 完成后出现了树形汇总视图。面板的滚轮滚动和鼠标点击终于统一了。

**如果说 May 12 是在"造火箭"，May 13 就是在"调轨道"——让那 98.5% 的缓存率在各种边界条件下依然稳定。**

---

## May 14 · 流式 LLM 与收尾之美

LLM 流式输出支持正式上线——文本逐字出现在 TUI 上。llm-gateway 透明代理诞生、Git 安全提示词就位、GlobalUiState 从 ServiceRegistry 中分离、Grep 工具增强到 8 个新参数。

CLAUDE.md 同步了 29 个 commit 的架构变更和 TRAP。issues 归档了一大批，整个 spec/global 知识库焕然一新。

**最后一天，像写完小说后校对标点——整理文档、归档 issue、补充 TRAP。所有螺丝都拧紧了。**

---

## May 15 · 延迟写入革命与国际化的种子

tool_dispatch 迎来一次从根上消灭 bug 的架构重构——延迟写入模式彻底解决了并发工具调用中的孤儿 tool_use 问题。anthropic、openai、filesystem、web、MCP 五大模块全部拆成目录结构，anthropic 的缓存逻辑独立成 cache 子模块。

工作区 crate 集体改名 `peri-*` 前缀。i18n 国际化框架落地——FluentBundle + include_str! 编译时嵌入翻译资源。TUI 加上了 scroll-to-bottom/top 箭头按钮。Thinking 尾部预览上场——最后一条无正文的 AI 消息展示思考最后 4 行。

resize 事件去抖合并消除了流式加载时拖动窗口的 CPU 暴涨。one-liner install + self-update 脚本就位。GLM Anthropic 端口的 tool_result id 缺失补上了防御。批量依赖升级了 10 项直接依赖 + 84 个 patch 包。

**今天的主题是"拆"和"消"——拆大文件，消灭系统性 bug。延迟写入不只是修 bug，是从设计层面堵住了同一类问题发生的可能。**

---

## May 16 · 安装向导的成人礼

Setup 向导迎来一次全面升级：语言选择步骤、LLM 真实验证连通性测试、1M context toggle、/exit 和 /setup 命令。9 个 P2/P3 级别的 bug 修复让向导从"能用"走向"体面"——ProviderType 切换不再丢失输入、Ctrl+C 可用、API Key 字符级遮蔽、Browse 模式错误提示。

ACP 协议补上了 `$/cancel_request`——pending request 追踪 + 权限转发循环可取消 + AvailableCommandsUpdate 推送。工具输出截断后的完整内容持久化到磁盘。

max_tokens 从硬编码改为可配置（默认 32000），Model Panel 里多了 UI 控制。frozen_subagent_vms 跨轮次累积的深坑终于被填上。

**Setup 向导不再是那个"凑合能用"的初版了。它开始像一个正经产品里的 onboard 流程——引导、验证、反馈，每个步骤都有充分的容错。**

---

## May 17 · 同步之门与并发死锁的终结

config-sync 功能上线——跨机器的配置文件同步。SubAgent 系统迎来两场关键手术：（1）Agent 工具改回顺序执行，从架构上消除并发 SubAgent 的死锁；（2）SubAgent 事件路由从位置栈改为 source_agent_id 精确匹配。

`$/cancel_request` 补了三块短板——一次性取消所有 pending 请求、每次权限请求前检查 cancel_token、cancel 后重置 CancellationToken 让后续 prompt 正常工作。`/split` 后左侧 session 命令注册表永久丢失的 bug 被修复。SubAgent 加上了短 hash 标识符，不再是一堆相同的名字。

langfuse-client 和 main_ui 各自拆成目录。**今天解决的都是"同时发生"类问题——并发 SubAgent 的事件路由、死锁、取消——这些是最难 debug 的，因为出问题的时候系统已经处于不可预测的状态。**

---

## May 18 · ACP/TUI 脱钩——分层的最后一块拼图

ACP/TUI 分离正式完成。`executor::execute_prompt()` + `EventSink` trait 成为 TUI 和 stdio 的共享执行管线。ACP 标准 modes/models/configOptions 在 session/new 中完整实现。`peri/*` 自定义通知命名空间取代 agent_event 承载 SubAgent/Compact/LSP/Background 事件。

HITL 审批和 AskUser 问答通过 ACP transport 走通了双向通道——Esc 拒绝、选项渲染、响应回传。markdown 表格的 CJK 列宽挤压终于修了——比例缩放改为最小宽度优先。

**大规模文件拆分日：** message_pipeline、message_view、acp/dispatch、setup_wizard、login_panel、mcp_panel 六个模块拆成目录，commands 从 25 个扁平模块重组为核心/面板/session 三组，plugin/installer 和 plugin/marketplace 各自成目录，subagent/tool 拆出 tool/ 子目录。

并发 Agent 工具执行恢复——带 per-child 事件处理器，重复 SubAgentState 的产生被根除。Git Attribution 从消息注入迁移到 system prompt 预处理。

**今天是架构上"刻度最深"的一天。ACP 的分层完工意味着 TUI 不再知道 agent 是怎么执行的——它只是一个 ACP client。从今往后，外部 IDE 和 TUI 在协议层面是平权的。**

---

## May 19 · ACP 全栈贯通

ACP session 生命周期全链路打通：initialize 声明能力 → session/new 创建 → prompt 执行 → list/close/resume/fork/load 完整覆盖。compact_runner 模块从 TUI 抽到 cc-acp，auto-compact 循环内置于 executor——从触发到执行到 resubmit 全部共享。

StdioEventSink 上线——IDE 路径通过 stdout JSON-RPC 消费 ExecutorEvent。Config 类型和 LlmProvider 在 cc-tui 和 cc-acp 之间统一，消除类型重复。SubAgent 有了唯一的 instance_id——同名 agent 并发时事件路由不再混乱。

Session 持久化到 ThreadStore、todo 事件通过 executor pipeline 推送、background 任务完成后的内存泄漏修复。文件拆分继续：event.rs 拆成 event/ 目录、panel_ops 拆成 per-panel 子模块、agent_ops 拆成 handler 模块。

**如果说 May 18 是"打地基"，May 19 就是"盖房子"。ACP 从一个协议骨架变成了一个完整的服务层——session 管理、事件路由、类型统一，每个角落都通了。**

---

## May 20 · 主题解放与面板的触觉

Theme/Markdown 颜色系统彻底解耦——ThemeMarkdownAdapter 桥接 Theme trait 到 syntect 的 MarkdownTheme，diff 颜色（add/remove/hunk）加入 Theme trait。从此换主题不再需要改 markup 渲染逻辑。

**ScrollbarMetrics 革命：** ScrollableArea 返回 ▲/▼ 按钮区域和滚动条几何信息，PanelManager 统一分发 mouse 事件，所有可滚动面板实现了 set_scroll_offset。面板滚动条支持鼠标拖拽、点击、按钮跳转——这是 TUI 的"触觉体验"。

Compact 从 agent 内嵌逻辑迁移到 CompactMiddleware（before_model hook），整个 compact 生命周期（hooks + 事件 + 取消 + resubmit）由中间件统一管理。遗留的手动 compact 代码全部删除。

防御性修复日：session 恢复后过滤 System 消息防止 prompt 泄漏到渲染、compact 后 LLM 失败不再清空视图、Agent 执行失败发送 ExecutionFailed 事件通知前端、langfuse tracer 缓存 token 重复累加 bug 修复、sync 模块 Windows 路径分隔符归一化。

文件拆分最后一波：agent_ops 拆成子模块、plugin_panel handlers 拆成 9 个子模块、render 拆成 6 个子模块、app/mod.rs 用 include! 分组模块声明、panel dispatch 宏提取到 macros.rs、edit_utils 独立成文件。

**今天是"收束日"——把散落的细节收拢回设计框架。滚动条从视觉装饰变成了交互元素，主题从硬编码变成了可插拔，compact 从分散逻辑变成了单一中间件。Perihelion 的内部结构，在这一天之后，终于有了清晰的骨架。**

---

## 九周缩影

| 日期 | 主旋律 | commits |
|------|--------|---------|
| 3/20 | Hello World | 1 |
| 3/21 | Agent 定义文件 | 2 |
| 3/22 | SubAgent + 存储设计 + UI 重构 | 4 |
| 3/23 | Remote Control / Headless / 工具展示 | 11 |
| 3/24 | Relay 样式 / Langfuse / Compact 指令 | 19 |
| 3/25 | Langfuse 深度集成 / 零 Clippy | 20 |
| 3/26 | Relay 加固 / 远程控制面板 / Skill Preload | 24 |
| 3/27 | 架构债清偿 / Relay 同步 / 移动端 | 22 |
| 3/28 | 跨平台构建 / 交互统一 / 样式重构 | 21 |
| 3/29 | peri-cli 发布 / Langfuse Client / 大重构 | 14 |
| 3/30 | Langfuse 独立 / Cron 完成 | 4 |
| 3/31 | Setup 向导 / 历史筛选 | 8 |
| 4/27 | 组件库 / Token 计数 / 权限模式 / Relay 移除 | 10 |
| 4/28 | 消息管线统一 / 压缩完成 / LLM 重试 | 13 |
| 4/29 | **四十二个修复的马拉松** | 48 |
| 4/30 | 系统提示词重构 / Prompt Cache 修复 | 32 |
| 5/1 | TUI 抛光 | 12 |
| 5/2 | MCP + ACP 落地 | 18 |
| 5/3 | SubAgent 三件套 + dag 诞生 | 24 |
| 5/4 | acpx-g 设计马拉松 | 45 |
| 5/5 | WebFetch/Search + acpx-g 收尾 | 38 |
| 5/6 | 插件商店 | 17 |
| 5/7 | Hooks 系统 + 零警示 | 24 |
| 5/8 | TUI 架构重构 | 15 |
| 5/9 | Tool Search + 改名 | 21 |
| 5/10 | Thinking 模式打磨 | 19 |
| 5/11 | LSP + RebuildAll 统一 | 28 |
| 5/12 | **缓存逆袭 70%→98.5%** | 35 |
| 5/13 | 缓存稳定 + 流式渲染 | 31 |
| 5/14 | 流式 LLM + 收官 | 24 |
| 5/15 | 延迟写入重构 + crate 改名 + i18n | 29 |
| 5/16 | Setup 向导升级 + ACP cancel | 36 |
| 5/17 | config-sync + 并发 SubAgent 死锁终结 | 17 |
| 5/18 | **ACP/TUI 脱钩 + 大规模文件拆分** | 41 |
| 5/19 | ACP 全栈贯通 + session 生命周期 | 43 |
| 5/20 | 主题解耦 + 滚动条交互 + compact 中间件化 | 42 |

**总计：730 个 commit，60 天，9 周。**

从 ReAct 循环的第一行代码到 Anthropic 缓存 98.5%，从 Relay Server 的远程控制到插件商店的开张，从 cc-widgets 组件库的诞生到 acpx-g 的 22+ 轮设计评审，从 ACP 协议骨架到 TUI/IDE 平权的完整服务层，从硬编码主题到可插拔的 ThemeMarkdownAdapter，从粗糙的 Setup 向导到带语言选择和真实连接验证的 onboard 流程。**这不是一个 sprint——这是一段旅程。Perihelion 从 0 到 1，再到 1.0。**

---

## 番外 · cc-code 时代（2026 年 6 月 – 9 月）

九周缩影之后，项目换了个名字重新出发——**cc-code**。6 月 25 日仓库以「Initial commit: cc-code」重置，npm 包从 `v0.6.13` 起跳。此后三个月，约 260 次提交把项目从「作者的玩具」推向「别人能装、能用、能在 Windows 上跑一整天」。

### 六月 · 装进 npm，走向世界

关键一跃是**分发形态的改变**：从源码编译安装转为 `npm install -g cc-code`，`install.js` 自动下载对应平台的预编译二进制。伴随而来的是 Windows 生态的持续填坑（`cmd /C` 引号语义、Git Bash fallback、控制台代码页、流式输出编码）与 **i18n 落地**——中英双语 `/lang` 切换，面板文案统一接入 `lc.tr()`。

命令行也迎来一波扩充：`/commit` 一键提交、`/review` PR 审查、`/export` 导出对话、**Ctrl+B 前台 shell 后台化**、`/gc` 手动内存回收。6 月 27 日还做了一轮安全加固（sqlite/配置目录权限、MCP OAuth 回调 state 校验、ACP session 归属检查）。

### 七月 · 模型自由与界面的骨架

主题是**多 Provider 平权**。`v0.6.39` 起模型选择值改用 `provider_id::alias` 格式，切换模型携带 Provider 上下文，OpenAI / Anthropic / DeepSeek / GLM 之间热切换不再串味；`/model` 命令改为从当前 Provider **动态收集非空 alias**，不再硬编码三选一。权限模式循环也定下最终语义——`Default → AcceptEdit → AutoMode → Bypass`，**跳过 DontAsk**，状态栏给出 `(Shift+Tab to cycle)` 提示。

状态栏本身重构为 **codebuddy-hud 风格**的动态 2-3 行布局，支持多工具并行展示。后台 shell 补齐「人机感」：超时自动后台化、Ctrl+B 先聚焦入口再打开面板、面板内显示已运行时长。

### 八月 · 静默期

没有提交。像一次深呼吸。

### 九月 · 十四个版本的大爆发

从 `v0.6.48` 到 `v0.6.80`，一个月连发十余版，是继 5 月之后最密集的冲刺。几条主线：

- **工具系统换引擎**：Grep/Glob 引入 **rg CLI 双引擎**（外部 ripgrep 优先，回退进程内 Rust 引擎），npm 安装时自动捎带 rg 二进制；`normalize_params` 感知工具名，防止篡改 path 参数。
- **输出压缩双轨制**：接入 **RTK 代理**，对 git/cargo/npm/docker 等 23 类命令执行 `rtk rewrite` 降低 Token；`output_filter` 增加 RTK git status 噪音清洗，同时移除会吞并代码上下文的「毒性通用折叠」。
- **多模态与识图**：Read 工具支持图片读取并加 **魔数（Magic Bytes）校验**防伪图，工具包装器补齐 `invoke_content` 透传。
- **TUI 输入与渲染打磨**：引入**独立输入泵 InputPump** 隔离 Windows 控制台重入风险，安全启用鼠标悬停；有界事件批处理合并高频滚轮；滚动条滑块相对拖拽；悬挂缩进（列表/引用块/长气泡续行）；Markdown 超链接点击唤起浏览器；动态终端标题 Status Surface；状态栏 CPU 指标换成 **Prompt Cache 命中率**。
- **执行中消息队列**：Agent 跑着时的输入不再打断或丢失，而是按轮次以增量 StateSnapshot steering 注入。
- **/recap 会话回顾**：一句话「目标 + 任务 → 下一步」，独立 `aux_model` 与 compact 解耦；终端失焦 + 足够轮次 + 静默后**自动回顾**；`-c`/`-r` 恢复会话时 recap 与任务完成总结行也一并持久化。
- **工具调用可靠性**：参数 Schema 预校验（对齐参考实现 `formatZodValidationError`）+ 工具错选启发式诊断 + 连续失败熔断；动作签名循环检测，连续 3 轮相同动作即注入纠正。
- **模型四档**：`fable` 作为第四档别名加入（opus / sonnet / haiku / fable），配置迁移自动回填。

**三个月，约 260 个 commit，从 `v0.6.13` 到 `v0.6.80`。** 如果说前九周是把 Perihelion 从 0 造到 1，那 cc-code 时代就是把它从「作者的玩具」变成「别人能装、能用、能在 Windows 上不闪屏地跑一整天」的工具。名字变了，内核还是那个 ReAct 循环——只是现在它装进了 npm，说起了两种语言，认得出 ripgrep，也学会了在跑长任务时顺手做一次 recap。

---

## 十月 · 把读数变成事实（v0.6.81 → v0.6.103）

九月的冲刺没有停在月底。9 月 28 到 10 月 8 日，短短十一天又推了 **23 个版本**（v0.6.81 → v0.6.103），主题从「加功能」转向「**把工具自己说的话验证为真**」——诊断指标要准、渲染承诺要兑现、跨平台路径要一致。一句话概括：**这一轮修的不是功能，是「可信度」。**

### 权限门真正关上了

- **Jev 语义门按官方语义重做，权限模式折叠为 `auto` / `bypass` 两档（#247）**：实测下来 `Default`/`DontAsk`/`AcceptEdit` 要么每次弹窗、要么半自动，体验差且不智能，索性收敛成两档。Shift+Tab 只在 Auto ↔ Bypass 间循环，未知取值一律回退 Auto，绝不因陈旧值滑进 Bypass。
- **默认启用审批，fail-closed（#284）**：`YOLO_MODE` 未设置不再等于免审批；`-y/--yolo` 才真正接线；HITL 门控改为评估 **rtk 改写后的实际执行命令**，而不是原始命令。
- **ACP stdio 权限审批转发给客户端（#328）**：`cc-code acp` 此前直接放行所有工具审批，HITL 形同虚设；现改为 `AcpPermissionBroker`，敏感操作以 `session/request_permission` 交给 IDE 客户端，不支持/调用失败一律拒绝。
- **审批弹窗三选**：一次性 / 本次会话 / 拒绝，`allow-always` 复用会话级审批记忆。
- 一批安全加固（#288–#290, #17/#18/#19/#21/#22）：预审批不再执行 `rtk` 二进制、项目 hooks 需显式信任、npm tarball sha256 校验、sync KDF 随机 salt、自更新脚本验 hash。

### 工具输出与渲染的「诚实」

- **/gc 内存诊断修正（#339）**：原来的诊断数字会误导人——`active` 用的是 mimalloc 历史触及高水位（7500 MB 级）、碎片估算混入虚拟地址，容易被读成「内存爆炸」。这一轮给每个指标标注真实语义（`active`＝历史高水位勿用于诊断、`resident`＝真实驻留），把 `view_messages` 估算纳入，让「已知 vs 未识别」的账对得上。
- **/gc RSS 变化符号修正（#346）**：`/gc` 输出的 `RSS: 197.9 → 196.6 (+1.3 MB)` 方向是反的——**下降**被显示成 `+`。根因是 `delta = before - after` 却只在 `>= 0` 时加 `+`。同批修掉 `allocated` 差值文案的方向性，抽 `fmt_signed_delta()` 统一 `+/-/±0` 语义。
- **详细模式渲染的两处「说了却做不到」**：
  - **超长命令的状态行刷新不再覆盖 header 续行（#341）**：详细模式下 header 折成多行后，tick 增量刷新仍写死 `lines[1]`，把命令续行改写成 `Running…` 并残留右括号、真正的状态行却被冻结。改为**按内容定位状态行**。
  - **运行中工具头前缀不再随指示器闪烁左移（#343）**：指示灯在 `●` 与空格间闪烁，而折行会 trim 行首空白——熄灭帧把 `● Bash(` 前缀整个吃掉、整行左移。修复为**保留工具头的结构性前缀**，其它折行路径行为不变。
  - 更早的 **#287** 也让「详细模式超长无空格命令」的头行不再孤立成 `●`。
- **思考尾部预览移除（#337）**：非详细模式下 `Thought for Ns` 只保留摘要行，不再附推理尾部；详细模式（Ctrl+O）仍可展开完整推理。
- **i18n 收口（#338）**：后台 shell 通知文案（「已完成/超时终止/已取消…」）此前硬编码中文，现接入 `shell-notify-*` FTL key 并引入进程级语言注册表，供静态构造路径读取。

### 界面与交互的持续打磨

- **Thinking 状态行 + 工具动作汇总（#249）**：非详细模式对齐参考实现，运行中 spinner 行加入 `· thinking / · thought for Ns / · still thinking` 四态；只读工具折叠为计数文案。
- **前台 `!` 命令结果回流上下文（#270）**：`!` 跑的命令结果能被 Agent 读到，不再是一次性输出。
- **排队消息键盘操作（#282, #335, #336）**：`Ctrl+S` 立即发送 / `Ctrl+X` 删除；10 月又把快捷键从 `Ctrl+S/Ctrl+X` **迁移为 `Alt+S/Alt+X`**——因为 Windows conhost 会把 `Ctrl+S` 当成终端流控键截走，按键根本到不了应用。
- 零碎修复一箩筐：`Ctrl+C` 面板优先（#330）、git 分支探测异步化（#278）、markdown 水平线按宽度渲染（#273）、Bash 非零退出颜色口径统一（#251）、Thought 归并吸收前置只读计数（#280）。

### 平台与工程基建

- **Windows 启用 mimalloc 全局分配器（#285）**：顺带让 `/gc` 能读到分配器明细。
- **配置目录统一到 `~/.cc-code`（#289）**：新目录优先、旧 `~/.peri` 仅在存在时回退；MCP / OAuth / threads / skills / 历史 / sync 共 10 处调用点迁移。
- **crate 改名 `peri-*` → `cc-*`（#333）**：与上游彻底切割，6 个 workspace crate 重命名（二进制名 `cc-code` 不变，npm 用户无感）；清理 6 个无用 side-projects。
- **Windows CI 治 flake（#323, #329）**：加 Defender 扫描排除、后台任务输出文件创建加重试（治跑盘 NotFound）；CI 稳了下来。
- **文档收口**：README 修正与代码不符的陈旧描述（删 RISC-V 声明等）、补版权署名与 Cargo 元数据、`lefthook.yml` 对齐 CI（`-D warnings`）。每修一个 bug 都留一份 `spec/issues/*.md` 根因分析。

**十一天，23 个版本，从 `v0.6.81` 到 `v0.6.103`。** 这一轮没有新功能的大跃进，做的是另一件事：**让工具对自己说的话负责**——内存诊断不再吓人、RSS 增减不再反着写、详细模式承诺的「完整命令」真的完整、权限门默认就是关着的。如果说九月是「跑起来」，那十月就是「**值得信**」。

---

## 十月中旬 · 抽屉、缓存与热替换（v0.6.104 → v0.6.131）

10 月 8 日到 13 日，六天又走了 24 个版本。这一轮没有单一主题，而是三条长期欠账同时到期：**Jev 规则的磁盘缓存从无到有、Windows/npm 安装链学会了「运行中换二进制」、子 Agent 的呈现层做了一次伤筋动骨的重构。**

### Jev 规则缓存成型

- **磁盘持久化缓存落地（#398、#399）**：Jev 安全规则提炼结果落盘复用，不再每次启动重新提炼。
- **提炼超时阶梯重试 + 低思考档（#405、#406）**：顺带修掉一个隐蔽bug——缓存此前**从未真正落盘**，写了等于没写。
- **可读与归一（#412、#433、#434、#436、#437）**：缓存文件改多行 JSON 输出、人手可读；再归一为单文件、人性化目录命名，并与模型签名解耦，换模型不再整表失效。
- **Schema 熔断补强（#379、#404、#408）**：失败计数改为跨工具聚合 + 滑动窗口，堵住「交替错配」绕过连续失败计数的逃逸路径。

### Windows 与安装链：运行中也能换

- **npm 全局入口直起 exe（#410、#411）**：修掉 npm 安装下 Ctrl+C 直接退出 TUI 的老问题；传统 CMD 的控制码刷屏与星形缺字一并修复（#415）。
- **postinstall 热替换（#430、#431）**：进程运行中也能替换二进制，历史备份自动清理。
- **`update` 锁安全（#453）**：运行中执行 `cc-code update` 不再被误报成 UTF-8 错误——真因是便捷副本被文件锁占用、错误信息又是 GBK 编码被严格解码盖住；安装脚本改锁安全复制，输出改容错解码。

### 界面小步快走

- **「回到最新」浮层按钮（#448、#458）**：上滚看历史时一键回到底部，按钮按输入框白条居中。
- **状态信息密度**：动词流光动效与思考状态词的色温阶梯（#387、#388）、状态栏第二行改「最近工具」实时摘要（#390）、底部 Todo 列表默认折叠溢出（#392、#393）。
- **i18n 收尾（#314、#315、#316）**：语言检测、TUI 与 CLI 国际化补齐，硬编码中文清零。
- **`@` 文件面板（#463）**：输入 `@` 即开，默认只枚举当前目录、最多 15 项，不再递归扫全项目。
- **性能零碎**：`LlmCallStart` 载荷按需构造、无订阅者不再全量克隆消息历史（#369）；diff 缓存加字节预算、`/gc` 补齐渲染与 diff 缓存统计（#420、#421）。

### 子 Agent 收官：只有后台、只有抽屉（#467 / #466）

- **抽屉 + 独立视口**：子 Agent 按实例 ID 进底栏抽屉（最多五行），Enter 进入独立视口——独立的思考、正文、工具、草稿与滚动位置，互不串扰。
- **统一「只有后台模式」**：`Agent` 工具删掉 `run_in_background` 与整条前台同步路径；fork 与普通子 Agent 合并到同一条构建路径；事件按来源 ID 路由到各自会话管线。
- **静默失效根除**：`peri/subagent_lifecycle` 通知不带 `notifications/` 前缀、原先被客户端按未知通知丢弃，抽屉整块不显示——服务端一直在发，界面永远为空。现按方法名显式登记，回归用例守住。
- **独立审计闭环**：合并前两轮审计共修掉十处缺陷，最关键的是终态内存回收——已完成的子会话在抽屉行移除的同帧回收、注册表终态快照只保留最近 8 条。设计原则就一句：**没有消费者的留存不保留，要回看去 SQLite，不占内存。** #466、#373、#31、#370、#275 五个关联 issue 随 v0.6.130 一并关闭。
- **上线即回归、当天修掉（#468）**：v0.6.130 修「卡片漏回主时间线」时剔除范围过大，`/rewind` 全量重放会把归档卡片一并删掉。v0.6.131 把剔除下移到活跃轮次尾部的唯一产地，一对互为反面的回归用例锁死边界。

### 子 Agent 补完：让用户看得见「下发了什么指令」

抽屉解决了「子 Agent 住哪儿」，但主视口的启动摘要只有 `├── 任务` 这一行摘要——用户想核验主 Agent 到底给子 Agent 写了什么 Prompt，还得一个个人视口点进去看。这一轮把它补完：

- **`Ctrl+O` 展开 Prompt 卡片**：详细模式下摘要切换为方案 B 经典卡片（`● Agent(任务预览)` + `└ Backgrounded agent` + `└ Prompt:` + 指令全文），与普通模式的极简树形同源、由 `detail_mode` 单选渲染；先在有 `docs/subagent-mock.html` 里把静态方案做出来给用户确认，再落到终端渲染。
- **事件层补全文**：`SubagentStarted` 此前只带 40 字 `task_preview`，指令全文在事件流里就被丢掉了——新增 `prompt` 字段从中间件一路透传到 TUI 会话句柄。**数据不到，渲染就是空谈。**
- **字形统一（用户当场发现的）**：卡片标题一开始误用了小圆点 `•`（U+2022），用户一眼看出「比别处的圆点小」。统一为系统大实心圆点 `●`（U+25CF，与 `● Bash(...)` 同字形）——**视觉一致性上，用户的眼睛比代码规范更灵敏。**
- **顺手修掉一个必崩 bug**：逐行解析 `prompt` 时用了字节下标切片 `&line[4..]`，`└` 占 3 字节，直接 `not a char boundary` panic；改用 `strip_prefix` 后由新用例双态锁定（普通模式不含 `Prompt:`、详细模式含全部卡片元素）。

**一个体面的界面不只是「能看见」，而是「想看什么就能看见什么」。** 抽屉给了子 Agent 一间隔音房，`Ctrl+O` 则给了用户一扇随时能推开的观察窗——模糊的需求变成静态原型、用户的直觉变成字形规范、崩溃的切片变成受测的分支。

### 主 Agent 学会「叫停」

Prompt 能看了，还剩一个更根本的缺口：**用户说「取消厦门那个子agent」时，主 Agent 只能回答「我做不到，你自己按 x」**。抽屉的 `x` 键解决了「人主动停止」，但协调本该由 Agent 完成——把用户的一句话变成一堆键盘操作，不叫能力，叫推卸。

这一轮补上的是**主 Agent 的取消权**：

- **扩展现有工具，不新增**：`Agent` 工具加 `action`（`spawn` 默认 / `stop`）与 `task_id`。工具清单保持稳定，`Agent` 的语义也从「只管派发」变成「派发 + 收尾」。
- **`prompt` 必须从 required 里拿掉**（反直觉但关键）：`action=stop` 时不需要 prompt，可 schema 若强制必填，模型会**为通过校验而凭空编造一个 prompt**。必填性因此下沉到运行期按 `action` 分支校验——**校验的位置比校验本身更重要**。
- **只认精确 `task_id`，拒绝模糊匹配**：兄弟任务的描述高度相似（「查询广州天气」/「查询厦门天气」），模糊匹配会停错对象。宁可让模型多查一次，也不误停。`task_id` 由 spawn 回执 `Background task bg-<uuid> started …` 给出，闭环自洽。
- **停止与完成、失败是三种语义**：终态各自成行（`was stopped by main Agent` / `finished` / `failed`），抽屉行改用**红色实心 `●`** + `stopped by …`——原来的橙色 `✗` 太柔，红实心才能让人一眼确认「这个停了」。用户按 `x` 停的标 `stopped by user`，与主 Agent 停的区分来源。

顺手补了 `✻ Waiting for N background agents to finish` 计数行——用户始终该知道「还差几个」。

**给 Agent 装上「刹车」，比给它装上更强的引擎更考验设计。** 一个 `action` 字段的取舍里藏着三件事：别让模型为了过校验而撒谎（`prompt` 不能必填）、别让相邻任务互相误伤（只认 `task_id`）、别让三种终态糊成一团（停止独立成行）。

**六天，24 个版本，从 v0.6.104 到 v0.6.131。** 上一轮的主题是「把读数变成事实」，这一轮是「把欠账变成资产」：缓存让 Jev 快起来，热替换让 Windows 用户不再被迫关进程，抽屉让子 Agent 第一次有了体面的住处——而且每个大改动都过了独立审计才上线。

---

## 十月下半场 · 记忆、主题与命令中心（v0.6.134 → v0.6.152）

上一段的收尾其实已经涵盖了 v0.6.132 / v0.6.133（Prompt 卡片与主 Agent 取消）。从 v0.6.134 接起，10 月 14 日到 16 日三天又走了 **19 个版本**。三条主线同时收口：**模型第一次有了跨会话的长期记忆、外观与动画成建制地对齐参考实现、散落各处的命令与通知收敛到唯一的语义判定入口。**

### 先做减法：一个「紧急」缺陷的终点是下线

`peri sync` 被整体删除——连同 Cloudflare Worker 服务端、客户端模块与 3 个专用依赖。安全审计（Finding H1，置信度 9/10）指出：relay 服务器生成 `pair_code` 后**自己持有**，即可派生 AES 密钥解开全部同步内容，其中包含 `ProviderConfig.api_key` 与 OAuth client secret。三种修复方案都要求改造加密协议，而该功能使用率低、长期还得维护一套独立服务端——于是选择消除攻击面，而不是打补丁。

**有些功能的正确归宿不是修好，而是拿掉。**

### 模型终于有了长期记忆（#507）

自动记忆（Auto-Memory）按参考实现规范完整落地，架构选择很克制：**纯文件、零专有工具**。

- 记忆目录对模型呈现为 `~/.cc-code/projects/<sanitized>/memory/`，模型用现成的 `Write`/`Edit` 维护带四类型 frontmatter（user / feedback / project / reference）的 Markdown 文件，并在 `MEMORY.md` 登记单行指针；会话启动时索引有界载入（≤200 行且 ≤25KB，超限安全截断并附 warning）。
- 路径全面脱敏：`D:\code\peri` 清洗成 `D--code-peri`，系统提示词里不再出现盘符与本地用户名；`~` 自动展开，去掉了「必须写绝对路径」的隐性约束。
- **双向兼容**：优先复用 `~/.claude/projects/` 已有的记忆资产，缺目录时才落 `~/.cc-code/projects/`——与参考实现的目录布局互通，不制造两套记忆。
- 安全门为记忆目录开了 carve-out（模型可自主读写，不被受保护目录规则拦截）；索引快照在会话创建时冻结、置于动态边界之后，Cache 前缀不受影响。TUI 侧把记忆读写从只读统计里剥出来，单独呈现 `recalled N memories` / `updated N memories`，折叠态 Read 显示 `Recalled memory`，长路径精简为 `memory/<文件名>`。

**记忆不是一件新工具，而是一份约定加一个目录。** 让模型用已有的手（Write/Edit）去写，比给它另造一件武器更可能被真正用起来。

### 提示词与工具边界：对齐参考实现的三次搬运（#504）

静态段落与工具描述都按参考实现官方文案做了一轮对齐，原则是**只搬不需要我方缺失能力的静态句子**：

- `01_intro` 补「信息足够即行动」与 Corrections（只在会改变用户决策时纠正）；`03_doing_tasks` 补 **Scope discipline**（不超范围加功能与抽象、优先编辑现有文件、删除无用代码而非留兼容垫片）；`04_actions` 补授权与影响判定框架；`06_tone_style` 明确「用户只看文本输出」与结果诚实性。
- Bash / WebFetch 改成 **NOT 反向枚举**（读→`Read` NOT `cat`/`head`/`tail`，改→`Edit` NOT `sed`/`awk`，内容检索→`Grep` NOT `grep`/`rg`……），Grep 首行同步禁用 `grep`/`rg`，两处形成双向引导。
- **`Write` 改为参考实现的 NEVER 语义**：默认不再主动创建 `.md`/README。这是一条**行为变更**——agent 不再自作主张落库文档，历史上那种「顺手写 spec」的习惯被规范掉了。

### 本轮最值钱的洞察：纠正提示为什么没被模型看见（#495）

连续失败、Schema 熔断、动作循环三类纠正提示此前以独立 `BaseMessage::System` 注入，看似合理，实则两处致命缺陷：

- **模型侧不可见**：Anthropic/OpenAI 适配器会把 System 消息抽取到请求顶层 `system` 字段，提示因此**脱离对话流末端**，自回归解码时根本看不到——这正好解释了实测里模型连续 5 次复读同一错误参数却毫无反应。
- **导出与会话快照丢失**：`StateSnapshot` 与 `/export` 都以 `!is_system()` 过滤，提示既不进快照也不出现在导出里。

修法是**并入其所属 `ToolResult` 的正文**：位置精确（贴着触发它的错误结果）、模型可见（留在对话流末端）、导出可见、零 UI 污染、多模态安全。**同一句话，注入的位置选错就等于没注入。**

### 外观与动画：成建制对齐，以及一次越界的回滚

- **`/theme` 主题系统（#509）**：把 UI 颜色从编译期常量抽成统一调色板（69 个与参考实现相同的语义字段 + cc-code 扩展字段），渲染层 900 余处引用改走当前调色板；7 套配色里 6 套与参考源码的 `theme.ts` **逐值一致**，由一致性测试逐字段锁定防漂移；光标移动实时预览、Enter 确认、Esc 还原，选择持久化。
- **Spinner 两连（#510、#511）**：删除自研的「思考越久越黄」时间热度四档——对参考实现 2.1.300 二进制实证下来，其并无热度分档；改为成对流光（±1 列窄带、50ms 步进、周期随文本宽度）+ 停滞转红（无新 token 超 10s 向 error 色插值）。tips 调度换成「每条 tip 有 id 与会话冷却、每轮挑选并扣留、轮末才发布」，展示历史持久化到 `~/.cc-code/spinner-tips.json`，并支持自定义条目。本轮还明确标注了一条**不做**的事：cc-code 无 `/btw`，参考实现那条 30 秒覆盖 tip 不移植。
- **状态栏配色回滚（#523）**：`/theme` 上线时顺带做了一次「颜色统一到调色板」的扫描，把状态栏原本写死的颜色也改成了调色板字段——模型名 / 分支 / 上下文仪表于是随主题漂移，而**并没有任何工单要求改它**。状态栏配色是打磨定稿的固定值，现抽出固定配色模块 `sb`、52 处引用改回原色，渲染结果与 #509 之前逐字节一致。**能统一的底色，和不该被统一的定稿，是两件事**：一次改造天然带着自己的边界。
- 小修两处：主题面板补上键盘与粘贴路由（#521，此前 `↑↓` 与 `Esc` 被穿透）；抽屉中间列只由助手文本驱动并加 200ms 节流（#526，服务端与 TUI 争写同一字段，导致该列在工具名与助手文本之间来回闪动）。

### 命令与通知：把「是不是命令」收敛到唯一入口

这一轮的交互修复有个共同点——**问题都不在功能缺失，而在同一个语义被拆成两份、各判一次**：

- **运行中敲命令不再被静默当成消息（#533）**：`Enter` 在**任何命令判定之前**就把输入塞进队列，文本的命令身份随之丢失，出队时只有 ACP 那 7 个命令会被重新识别，`/theme`、`/model`、`/gc`、`/exit` 等约 24 个 TUI 独有命令连同未知命令、`!` shell 一起被喂给模型——表现是 `/exit` 不退出、敲错命令反被模型一本正经地回答。现收敛到唯一判定入口 `CommandSystem::resolve`（空闲与运行中走同一条分流规则，`find` 与 `dispatch` 共用匹配逻辑），能否在运行中延后由 `blocked_while_busy` 显式声明，并用白名单回归测试锁死分类；顺带修掉 `steer`（`Ctrl+Enter`）把命令插入在飞轮次后退化成普通文本。
- **Rewind 改走 `/rewind` 命令，双击 `Esc` 触发移除（#531）**：`Esc` 的通用语义是取消/中断——中断 Agent 后下意识连按两下就弹出回滚选择器；反过来想主动唤起时又无命令入口。现 TUI 注册 `rewind`（别名 `undo`）承接入口，`Esc` 回归取消语义。
- **输入框 `/command` 高亮（#529）**：按参考实现的 `findSlashCommandPositions` 同一规则着色，`!` shell 命令态跳过，避免把 `/etc` 之类路径染蓝。
- **`/history` 改名归位**：CLI 语境里 `history` 指「你输入过的内容」，而 peri 的 `/history` 打开的是历史会话列表，想看输入历史只能 `Ctrl+↑/↓` 一条条回溯。新增输入历史面板（数据源与 `Ctrl+↑/↓` 是同一份，Enter 只回填不代发），会话浏览整体迁到 `/resume`——它本来就是 `/history` 的别名。
- **未知命令提示立即出现（#572）**：提示其实已经写进 `view_messages`，但消息区渲染是事件驱动的，`push_system_note` 只写状态不发事件，于是要用户再敲一次键才看得见。重绘内置于 `push_system_note`，一次性覆盖运行中被拒命令、`/rewind` 无历史等同类路径。
- **安全判定降级提示至少停留 5 秒**：Jev 语义门不可用而降级到兜底分类器时，这条提示的清除时机比显示时机还积极——`ToolStart`/`AssistantChunk`/`Done`/`Error` 四条路径都会把它清掉，用户只看见闪一下，事后只剩日志。按仓库既有的「最短可见时长」范式垫一档：安全能力降级是用户**应当知情**的事实。
- **后台通知不再等轮次结束**：`Bash(run_in_background)` 的完成通知此前只等 `Done` 才注入，延迟上限 = 任务剩余时长（实测被积压十几分钟）；而 `Done` 出口本身还不可靠——轮末优先 flush 用户排队消息会把 drain 整段跳过，中断与报错两条分支根本不消费队列。现复用 steering 通道把注入前移到「通知产生时」，延迟从「任务剩余时长」降到「一次 LLM 来回」，走尾部追加不破坏 Cache 前缀；消费回执的确认钉在「真正写入 state 之前」（`-32001` 未入队 / `-32002` 已入队未消费，据此决定重试，无法判定的错误宁可不送也不重投）。中断/报错后不自动唤醒新一轮，滞留通知随用户下一次主动提交一并送出。
- **等后台子 Agent 期间标题保持动画（#575）**：主轮结束就清 `loading`，可后台子 Agent 还在跑——标题于是落到 `Done`/`Idle`，动画停住，「还在等」与「已结束」分不出来。补一档 `Working` 判定，与主视口 `✻ Waiting for N background agents to finish` 同一判据。

### Agent Center：从「当前项目的历史」到「所有项目的会话」（#542、#554）

`/agents` 升级为全页根会话中心：按 Project / Status / Model 分组，五档过滤（All / Needs you / Working / Ready / Inactive）、真分页与 Show more、标题搜索、重命名、永久删除二次确认、150ms 防抖详情。更实质的是**会话目录跟随恢复目标**——跨项目恢复使用目标会话的目录，系统提示词按 ACP session 冻结，文件补全、相对路径、hooks、MCP、技能与分支缓存都随目录切换，而**不改变进程 cwd**；运行中任务、Shell、排队消息或审批未结束时拒绝切换，避免中断与结果串台。会话状态与快照一并落库（root 轮次终态 + model / reasoning_effort / branch 独立列，旧库幂等迁移）。

上线当天修的三处：状态栏 `← {n} agents` 原先统计的是「当前目录累计历史会话」（实测 194 条），改为与 Ready 过滤同口径（194 → 10）；详情面板按参考稿重排、`Last message` / `Prompt` 预览改走消息区同款 markdown 渲染；「内联 Diff」开关此前与 `Ctrl+O` 详细模式共用同一个渲染标志，打开后会连带放大 shell 输出行数、强制展开所有工具、取消参数折行——现拆成两个标志，普通页面只多出 diff 块。同时 agent 定义入口从 `/agents` 改名 `/agent-design`，`/tasks` 由会话中心取代、旧面板与接线在独立 PR 中删除。

### 工程与资源：三个只在特定机器上现形的坑

- **会话资源引用环（#567）**：中间件工具每轮转成共享对象，但包装器未析构内部 Box，工具持有的资源持续累积；延迟工具代理与子 Agent 模型工厂又反向持有注册表与会话池。现补唯一析构、注册表改弱引用访问、工厂持有会话池改弱引用——**最后一个持有者释放后正常析构，环拆开了**。修复范围是已复现的泄漏，实际 RSS 降幅要等新版重启后测量。
- **Windows CI 的 flaky（#538）**：`cc-tui` 的 shell / 面板用例用固定墙钟超时（5s / 10s，另有一处 500ms）等真实子进程，libtest 默认多线程并发跑时多个用例抢进程与 IO，4 核 runner 上启动延迟被放大到超出窗口；失败形态统一是「等不到异步结果」而非断言值不符。新增全局异步锁串行化真起进程的用例、超时统一为 30s 共享预算——只做串行化与放宽，不改用例语义。同类问题还有 mimalloc 两个测试共享进程级全局选项而互相污染（一方设了 `MIMALLOC_SHOW_ERRORS=1`，另一方读到的「默认值」就是 1）；以及 **`headless_app_real_rss` 在 Windows 上从未编译过**（`current_rss_kb()` 只有 `#[cfg(unix)]` 分支）——这正是仓库内存类 issue 长期只有 macOS 数据的直接原因。补上 `GetProcessMemoryInfo` 分支并默认开启四档扫描后，实测单档 1× 335 KB / 8× 619 KB / 20× 1052 KB / 30× 1471 KB，拟合 296 KB + 39 KB×内容倍数。
- **一批整理（#549、#551）**：66 个中英混排的测试名清扫为全英文 `snake_case`，代码标识符纯英文达标；`message_render.rs`（90KB+ 单文件）升格为目录并抽出 `decorators/`（bash / glob / read / ask_user），Bash 非零退出码提升为首行 `⎿ Error: Exit code {n}` 置顶、正文加 4 列悬挂缩进；rmcp 升级到 3.5.1 对齐 MCP 2026-07-28 数据模型；测试数据里的开发机用户名与私有项目名做了中性化。

**三天，19 个版本，从 v0.6.134 到 v0.6.152。** 如果说上一轮是「把欠账变成资产」，这一轮是**把散落的语义收回一处**：记忆收回一个目录与一份约定，外观收回一套调色板（外加一条明确的例外——状态栏不许跟），命令与通知收回唯一判定入口，纠正提示收回对话流末端。同时删掉了一个不必存在的功能——**给一个本就不该存在的东西打补丁，是成本最高的一种修法**。
---

## 十月下旬 · 视口、超链接与渲染管线的第一刀（v0.6.153 → v0.6.170）

从 10 月 16 日接上 v0.6.152，到 10 月 21 日走了 **18 个版本**。这一轮几乎没有新功能，三件欠了很久的基础工程同时收口：**首屏不再霸占整屏、渲染管线第一次真正提速、URL 与配色被迫反复定稿**。顺带揪出一条贯穿全仓的病：有些代码写得挺好，只是从没被调用过。

### 首屏从「占满整屏」变成「随内容长出来」（#652）

终端 TUI 一直有个没被质疑过的假设：启动即占满视口。空会话下那片留白既是视觉噪音，也让人分不清「程序在跑」和「程序卡住了」。v0.6.171 改成紧凑启动——从 shell 输出下方开始，随输入、回复、面板按实际视觉行数增长，长满宿主高度后才锁定全屏，且这份增长状态只属于本次终端运行。

随后两轮都在修它的副作用。输入 `/` 弹出候选浮层会把高度并入 `required_rows`，而 `allocated_rows` 是单调递增的，删掉 `/` 后浮层消失、内容高度回落，多出来的行全分给消息区，底部成片空白（#658）。第一次的修法是把 `required_rows` 拆成 `total` / `persistent`，让浮层只影响当帧面积；第二次才摸到真正的根因（#659）：**临时浮层本来就是覆盖在已有空间之上的 Overlay，根本不该进持久视口需求**——沿着「减少污染」的思路打补丁，结果在 24 行终端下把视口永久锁死全屏。最终是彻底重构：浮层不进 `required_rows`，恢复单调递增判定。

**给一个测量方法打补丁，不如先问这个测量到底在测什么。**

### 209 倍：Ctrl+O 不再全量重渲染（#626 / PR #631）

`Ctrl+O` 切换详细模式此前会清空全部消息的渲染哈希、对整段历史重渲染重折行——270 条消息的会话单次切换 6.9 秒，10,000 条约 239 秒（debug 实测），体验上等于「按一下，界面僵住」。

修法是三件套，核心只有一个念头：**别把没变的维度当成变了**。① **渲染键**——内容哈希只与真正影响该消息渲染的维度合成；② **双变体缓存**——每条消息同时保留展开与折叠两份成品，切换时整表交换；③ **分段 wrap 缓存**——折行结果按消息分段缓存，段键含逐行 epoch，指示器秒数与动画变化只让对应段失效，流式输出时只重折尾段。

结果：270 条约 33ms，10,000 条 1.38 秒。执行期反复切换与从零渲染逐行一致，并有回归测试钉住（过程中还顺手修了两处自身回归：分段越界 panic、指示器行复用旧 wrap）。

### 长会话的第三条路：按视口渲染（#629）

缓存优化之后仍然有天花板：一条超长会话的**全部**细节都要被折行和渲染一遍，哪怕屏幕上只看得到十行。v0.6.169 给出 `detail_render_strategy` 三档——`cache`、`virtual`、`auto`：auto 在估算详情 30 万行或成品 64 MiB 时升档，virtual 用全局高度索引 + 逻辑内容锚点只发布当前消息窗口，两种模式的离屏成品共用 16 MiB LRU 预算。跨窗口选区按「源消息 / 行 / 字素」导出，复制不阻塞输入；宽度、主题、历史切换立即使旧选区失效。

边界写得很清楚：这个预算不覆盖源 VM、单条可见巨型消息、剪贴板与进程 RSS——**内存治理的每一层都要说清自己不管什么，否则下一轮就会有人把它当成万能承诺。**

### 一种颜色，三轮定稿（#649 / #656 / #667）

工具结果与 `!` 命令输出里的裸 URL，配色连改三轮：先是走 `theme::cur().ide`，随 `/theme` 漂移且与正文亮度过近（#667 之前）；v0.6.167 加上链接色与下划线的识别；v0.6.170 定稿为**固定 ANSI Cyan + 下划线，不随主题切换**（#649），与状态栏模型名、分支名同一口径；v0.6.173 又发现继承消息区背景后，同为 Cyan 的链接与状态栏模型名出现可见色差，补上显式终端默认背景（#656）。

这条线最终沉淀成一句可复用的判断：**颜色要么属于主题，要么属于定稿，不能在路上来回改主意。** 同一批定稿里，`/theme` 新增 `auto`（#604）也刻意留了一处「不做」——不从 Terminal.app 的 `Default Window Settings` 推断背景色，因为那只是下一个新窗口用哪个 profile，窗口可以单独指定（本机默认 profile 是黑底 Pro，实际窗口却是白底 Basic），据此判定必错，整条链路删掉。

### 三条「写了却没接上」的线（#578 / #585 / #592）

这一轮最值得记的不是功能，而是三次同型缺陷——**代码存在、单测存在、逻辑正确，只差一个调用点**：

- **`Ctrl+Enter` 从未真正生效（#592）**：`keyboard_modes.rs` 没有被 `lib.rs` 声明，`enable_keyboard_enhancement()` 全仓零调用点，应用从未向终端推送 kitty keyboard protocol。iTerm2 ≥3.5 / WezTerm / Ghostty / kitty 上，`Ctrl+Enter`、`Ctrl+I`、`Esc` 拿不到区分信号，排队消息的「立即补充」因此一直失效。
- **i18n 框架完整却没有接线（#578）**：en / zh-CN 各 550+ key 一一对应、占位符零错配，但大量用户可见文案硬编码在 Rust 里，中英界面互相残留；同时 FTL 里有 93 个 key 从未被任何代码引用。一次接线补 70 个 key、删 72 个死 key，并顺手把布局空格从 FTL 值搬回代码——**文案文件不该负责排版**。
- **macOS 的 PATH 写错了文件（#585）**：安装脚本一律写 `.bashrc`，而 macOS 登录 shell 读的是 `.bash_profile`，bash 用户装完新开终端仍然找不到命令。

**编译通过、单元测试通过、CI 全绿，都证明不了「它被调用过」。** 这三处后来都补了可执行的守护——契约测试、死 key 校验、按平台选 rc 文件——而不是靠再读一遍代码。

---

## 十月底前夜 · OSC 8 的三次返工与工具块的三段状态机（v0.6.171 → v0.6.190）

10 月 21 日到 26 日，**19 个版本**。这一轮的主角是两件看起来纯苦力、实际全是语义问题的事：**让终端认识「这是一条链接」**，以及**让一个 Bash 工具块如实说清自己处在哪一步**。两件事都反复返工了三轮，每次返工都推翻了上一轮的一个隐含假设。

### 终端超链接：一次注入，三次返工（#671）

消息区的链接在支持 OSC 8 的终端里**点不动**——悬停无提示、Ctrl/Cmd+点击没反应、右键菜单也没有「打开链接」。v0.6.178 的落地方案是「把转义序列注入 `Cell` symbol，不参与布局计算」。上线后症状是：**URL 只剩首字母**（`origin h`），链接后面的文字错位或残留上一帧内容。

v0.6.182 找到了真根因，是这一轮最值得记的一条：`ratatui` 的 `Buffer::diff` 用 `symbol.width()` 判断一个 cell 占几列，然后把后续 `width - 1` 个 cell 从差分里跳过不画。而 OSC 8 序列的载荷（URL）全是可打印字符——`width()` 于是等于 **URL 的长度**，链接后面几十个 cell 被整段跳过。修法是把注入点整体后移到字节出口：buffer 只存纯文本，`WidthSafeBackend` 写字节时按坐标查表包裹（线程局部、每帧重置），并加回归测试锁死两条不变式——「buffer 内不得出现转义序列」「链接后面的 cell 必须照常落笔」。

同批还修了两处同源：启用条件原本借道 `conpty::hover_available()`，而它衡量的是「宿主是否上报 ConPTY 鼠标悬停」，与「终端是否支持 OSC 8」并不等价，VS Code 集成终端、tmux、SSH 远端全被判 false，连正文 Markdown 链接也点不动——改为默认开启，仅 `DISABLE_HYPERLINKS` 显式关闭。v0.6.200 再修跨帧残留：链接颜色在帧差分前写入普通 Cell 样式，URL / 锚点 / 启用状态变化时只补画受影响的字符，每个命中区用稳定显式 ID。

**转义序列不是字形数据。它可以参与最终字节流，但不能参与任何按列宽、按 grapheme 走位的计算。** 三轮返工本质上是同一个假设的三次破产。

### Markdown：从「像」到「逐 grapheme 相同」（#666）

此前同一条回复在两边的观感肉眼可辨：标题配色、列表编号、引用条、分隔线、行内代码、表格框线全不一致。v0.6.176 逐项对齐——H1 为 `BOLD|ITALIC|UNDERLINED`、H2 及以下仅 `BOLD` 且不带前景色；无序列表统一 `- `，有序列表按嵌套深度在数字 / 字母 / 罗马数字间切换；引用块 `▎` 用 DIM、正文 ITALIC；分隔线 `---` 无颜色；行内代码改用 `Theme::permission()`。表格另写了一套盒线算法：`MIN_COLUMN_WIDTH=3`、`SAFETY_MARGIN=4`、`MAX_ROW_LINES=4`，列宽按「最长单词宽 / 整格内容宽」分配，表头居中并在**每行之间**补 `├─┼─┤`，超出上限回退纵向键值渲染。

更关键的是验收方式换了：新增 `cc_markdown_parity.rs` 与 126 例金标准 `fixtures/cc_markdown_golden.json`，逐 grapheme 比对纯文本与归一化样式标签，把「看起来一样」变成「逐字符一样」，并给已知偏离单独登记 14 项。

**没有对拍基线的「视觉对齐」只是一次主观验收。** 这条基线是后面所有渲染类改动的安全网。

### Bash 工具块拆成三段状态机（#675）

一个前台 Bash 命令的生命周期其实有三段，此前界面上只呈现「跑完了」：排队等前面的工具、等人类审批、真正执行。v0.6.180 先补 L1——显式执行阶段，空输出完成显示 `Done`（详细模式 `(No output)`），显式 timeout 以 `Running… (Ns · timeout Nm)` 展示并在结果底部保留尾注；v0.6.184 补 L2 与 L3——审批前显示 `Waiting…`、人工审批按调用 ID 显示 `Waiting for permission…`，实时输出首次立即采样、后续每秒最多一次、stdout/stderr 共用 64 KiB 展示缓存、运行两秒后显示最后五个视觉行 + 剩余行数 + 耗时 + 字节数。增量输出**不写模型历史**，print / stdio 路径忽略展示事件。

三段补齐之后，暴露的全是**段与段之间的缝**：Pending 与 Running 的闪烁节奏一快一慢（#698，改为共享 800ms 呼吸、全局同频同相位）；`Queued` 与 `Running` 都闪，导致「在等审批」和「正在执行」分不清（#706，排队改回恒亮，等待语义交给同屏文案）；执行中分支要求 `elapsed >= 2s`，于是 `Queued → Running` 切换后状态行整段消失约两秒（#706，改为立即渲染裸 `Running…` 占位）。这一批还修了渲染线程的 tick 饿死（#683）：每轮循环重建 200ms `sleep`，只要事件间隔短于 200ms（流式输出、每帧视口上报），sleep 就在到期前被 `recv()` 抢先丢弃——秒数停在某个值不再动。改为跨事件单调推进的 `tick_deadline` + `sleep_until`，`select!` 加 `biased`。

**状态机的价值不在于多出几个状态，而在于相邻状态之间不能再有缺口。** 用户看到的「闪一下」，全都是某段分支切换的缝。

### 同一份结果进了两次上下文（#613）

后台子 Agent 在主线尚未结束时完成，完整输出会经两条通道各进一次上下文：`drain_notifications` 以 Human 消息写入 history，自动续接又由 `inject_bg_result_messages` 注入合成的 `AgentResult` tool_use + tool_result。两份文本同源逐字相同，模型在同一段对话里读了两遍，续接轮直接退化成复述——实测 messages 109 → 113，续接回复是「已在上一条消息中完整给出」。修法是在注入前按 `[Background task {short_id} ` 前缀（与 `to_notification()` 同源的 8 位 id）判重，三种时序——工具步骤后 drain、final answer 后 drain、`Done` 之后完成——都要正确。

### 安装与发布链：四连修与一条「配置从未被读取」

- **升级通道被自己堵死（#632）**：`cc-code update` 从浮动的 `main` 下载 `install.sh`，而期望的 SHA256 是**编译期硬编码常量**——main 上脚本任何一次改动都会让所有已发布版本的校验失败，还把提示误报成「脚本可能被篡改」。修法是让地址与校验值同源：`build.rs` 从 HEAD 提交的 blob 原文算 SHA256（避开 CRLF checkout 差异），并把同一个 commit sha 注入为 `CC_INSTALL_SCRIPT_REF`，`raw.githubusercontent.com/.../<commit>/scripts/install.sh`。**浮动引用配固定校验，天生是一枚定时炸弹。**
- **npm 分发去掉安装脚本（#678）**：二进制拆成 5 个平台包，由主包 `optionalDependencies` 引用，npm 按宿主 `os`/`cpu` 只装一个，安装期除 registry 外不访问网络、不执行任何脚本。这条改动后来证明是最省心的一次架构选择——所有「安装脚本被安全策略禁用」类的支持问题从此不存在。
- **发布校验等 registry**（#680）、**tag 存在性改查远程**（#682）：都属同型——本地状态不代表远程事实。
- **jemalloc 启动配置从未生效（#622）**：`alloc_config.rs` 设的是 `MALLOC_CONF`，而 tikv-jemalloc 根本不读这个名字，只认 `_RJEM_MALLOC_CONF` 或静态符号 `_rjem_malloc_conf`——所有分配器调优一直在空转。改为在二进制内导出静态配置符号，并用 `stats_print` 实机验证配置被读到。

**这一轮出现三次「代码正确、通道错误」：转义序列走错层、校验值走错来源、配置变量走错名字。** 它们都不会报错，只会安静地不生效——这也正是它们能活这么久的原因。

---

## 十月底 · 折叠组成为一等公民与「判定收敛到一处」（v0.6.191 → v0.6.203）

10 月 26 日到 28 日，两天 **10 个版本**，是这一整段里最密的一轮。主线只有一条：**把「这东西该怎么显示」从协议层收回到展示层**——模型不再为渲染撒谎，工具 schema 不再承载展示参数，状态判定只剩一处入口。

### Skill 成为一级工具，伪造的消息退场（#730）

技能此前只能靠模型自己用 `Read` 打开 `SKILL.md` 触发，界面上表现为一次普通文件读取；而 `/skill-name` 这类显式调用会由中间件**伪造** `Ai[ToolUse: Read]` + `Tool[ToolResult]` 插入历史——对强制校验 thinking 回传的模型来说，这条没有 thinking 的伪工具消息随时会引爆 400。现在 `Skill` 是核心工具（白名单 11 → 12），`invoke` 返回技能指令全文与 camelCase 元数据（`commandName` / `status` / `allowedTools` / `instruction`），`tool_use` 由模型自己发起、thinking 块自然产出，协议层面天然合法。`context: fork` 的技能在隔离子代理中执行并等待结果、取消随父调用级联；压缩恢复废弃「扫描技能目录路径」的启发式，改为按工具名精确配对。TUI 侧新增专属折叠卡片，默认收成一行 `Successfully loaded skill`。

**「让模型看见的」和「让用户看见的」必须是同一件事，否则历史里迟早留下一段没人发动过的工具调用。**

### 折叠组：file_inspect 活了一天就被删了（0.6.195 → 0.6.197）

v0.6.195 给 Bash 加了可选布尔参数 `file_inspect`，让模型标记「我这条命令是只读查看文件」，前端据此把它并入 `Read` / `Grep` / `Glob` 的同一个只读组。字段本身是纯展示的，不参与执行、权限、审批或遥测——但这个设计有一个无法回避的代价：**它要求模型为渲染打标记**。模型可能忘、可能多标、可能对同一个语义给出不同判断，而这条信息只影响折叠外观，出错的后果却是界面形态不一致。

两天后的 v0.6.197 把它整个删掉：判定**完全收敛到前端命令词法**——读取器白名单（`ls` / `cat` / `head` / `tail` / `find` / `wc` / `rg` / `grep` / `tree` / `du` / `stat` / `file` / `less` / `more` / `strings` / `jq` / `awk` / `cut` / `sort` / `uniq` / `tr`，`sed` 仅 `-n`）+ 中性命令（`cd` / `pwd` / `echo` / `printf` / `true` / `false` / `:` / `export` / `set` / `which`）+ 拒绝写盘。schema 里的参数删除，历史会话里的残留字段一律忽略。词法判定同时补齐三处边界：`2>/dev/null` / `2>&1` / `>&2` 只丢弃或合并 stderr、不算写盘；引号内的 `;` `|` `&&` `>` 是参数内容、不是分隔符或重定向；命令替换与反引号仍保守判为非只读。

折叠摘要固定顺序 `read → listed → searched → ran shell → recalled → updated`，`Ctrl+O` 逐条展开，失败命令保留退出码与 stderr；运行中的只读 Bash 也进组（代价是运行期不显示独立块与实时输出），历史恢复与窗口重建按命令重算，保证 `/resume` 后形态与退出前一致。

**展示决策不该进协议。** 一旦模型开始为渲染服务，这份信息就同时失去了可靠性和权威性——错误的方向不是把参数写得更准，而是把判定搬回展示层。

### 思考行：两次重做，和三个「计时冻结」

思考行的第一轮（v0.6.195）对齐了参考实现：活跃态 `● Thinking for Ns, <现在时计数>…` 按墙钟 500ms 显隐闪烁，正文到达即冻结为暗色 `Thought for Ns`，耗时改为组内事件间隔累计。第二轮（v0.6.197）直接做了减法：普通模式只留一行 `∴ Thinking for Ns…`，符号常显、每秒刷新、不再做 500ms 显隐，思考正文不在普通模式展示（此前会闪现最后 10 行再留存 960ms，读不清还抖动），完整正文统一走 `Ctrl+O`——随之下线 `THINKING_RETAIN_MS` 与整套留存渲染态，消灭「留存期间到期无人重绘」这一整类问题。

同一批修掉三个「数字不再动」的同型缺陷：`interrupt()` 只清了 `pending_tools`、`thinking_now` 未复位，中断后 `Thinking for 54s…` 继续走秒（改为中断即冻结当前思考段）；卡住检测的 thinking 指纹只取前 200 字符，连续多轮 `Edit` 常用同一套话起手，被判成同一指纹后注入「换策略」提示并**跳过当轮工具执行**（改为全文 64 位摘要，A→B→C 周期循环仍能命中）；渲染缓存的 `is_cosmetic_change` 把「blocks 相同、只有秒数在变」判成可复用，走秒被旧成品冻住（含活跃思考的视图一律真重渲染）。

### 自动 compact 不再清空消息区，流式渲染不再每帧解析两遍（#739）

自动 compact 此前会把已显示的历史整段重置成「压缩后的上下文」，用户眼前一黑。现在自动触发保留全部已显示历史，只追加一行 `Context compacted`，并把重建起点前移到视图末尾——后续重建只处理新增部分；视图历史加 1 万条上限，超出裁最老一段并在顶部给一行省略提示，同时平移重建起点与临时通知锚点、作废跨裁剪的文本选区。**被裁的只是屏幕历史，模型上下文不受影响。** 手动 `/compact` 仍按原语义重置视图。

流式这边是另一种重复劳动：App 侧预解析 markdown 后把 `rendered_prefix_len` 报成全文长度、`dirty=false`，渲染线程的增量入口因此短路；而宽度不等于 80 时扫描线程还会按实际宽度再解析一遍——同一份文本每帧解析两次。修法是构造气泡只交 `dirty=true` + 空 `rendered`，解析统一归渲染线程；block 档的块边界改用 `BlockBoundaryScanner` 状态机（按行扫描、跨调用保留围栏状态、只扫新增行），删掉「最后一行是否以三反引号开头」的启发式。2000 行文本单次全量解析约 121ms，改后构造流式气泡约 0.37ms。同期的长会话自动切虚拟布局（`AUTO_DETAIL_ROWS` 300,000 → 5,000，并新增 300 条 / 16 MB 两个触发维度）也基于同一个判断：**「这一帧到底需要算什么」应该由视口决定，而不是由数据总量决定。**

### 最后一公里：33ms 绘制期限、20 KB 记忆索引、一个不该被清空的草稿

- **滚动不再逐帧等渲染线程（#768）**：交互、流式输出与后台更新共用 33ms 绘制期限，宽度与模式变化由后台保序准备，未就绪时继续处理输入并保留旧屏——「等渲染完再响应」正式改成「响应与渲染各走各的节奏」。
- **记忆索引收敛到 20 KB（#732）**：会话创建只读一次 `MEMORY.md`，主提示词承载精简读取规则与最多 20,000 字节索引（入口 200 行），未命中再按关键词搜磁盘；移除宿主链上重复的记忆中间件。**上下文是稀缺资源，索引必须有硬上限。**
- **后台通知不再吞掉草稿（#777）**：空闲时自动提交后台 shell 完成通知，此前会把正在编辑的输入框清空、并消费掉草稿里的粘贴内容与图片。现在文字、光标、附件都留着——**异步通知可以打断阅读，但不可以碰用户还没发出去的字。**
- **Grep / Glob 对齐最新工具规范（#773）**：Glob 统一返回 cwd 相对路径（省 token）、无匹配统一 `No files found`、截断用标准提示；Grep 补 `context` 别名（等价 `-C`）与 ripgrep 转义说明；系统提示词强化「优先 Grep/Glob、不在 Shell 里裸调 grep/rg/find」。
- **链接配色收尾（#775 / 0.6.194 / 0.6.200）**：路径链接不再被统一染成链接蓝（`LinkHit` 增加 `recolor` 标记，文件路径只包 OSC 8、不动前景色），Markdown 链接也放弃主题成功色、保留下划线交给终端链接渲染——**可点性与配色是两件事，不能用一个开关同时决定。**

---

**十二天，47 个版本段、93 个合并 PR，从 v0.6.153 到 v0.6.203。** 如果上一轮是「把散落的语义收回一处」，这一轮是**把「谁负责」这件事重新分了一遍**：渲染归渲染线程，展示归前端词法，协议只承载模型真的要用的信息，而所有「写了却没被调用」「改了却没被读取」的通道都被逐个焊上了守护测试。三次 OSC 8 返工、两次思考行重做、一次只活了一天的参数——这些反复并非走弯路，而是每推翻一个隐含假设，系统就少一处靠运气运行的地方。
