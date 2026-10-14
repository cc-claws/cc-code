# ============================================================
# Peri TUI — 简体中文 (zh-CN) Translation File
# Key names must match en/main.ftl exactly.
# ============================================================

# ---- i18n 基础设施测试 key ----
test-hello = 你好，世界！
test-greeting = 你好，{ $name }！
ui-empty = 无

# ---- Command Descriptions ----

command-help-description = 列出所有可用命令
command-clear-description = 清空消息列表
command-exit-description = 退出应用
command-compact-description = 压缩对话上下文（结构化摘要 + 重新注入最近文件/Skills）
command-model-description = 打开模型选择面板（Provider + 级别 + Thinking）；带参数时直接切换别名（opus/sonnet/haiku/fable）
command-login-description = 管理 Provider 配置（新建/编辑/删除）
command-cost-description = 查看当前会话费用和 token 消耗
command-context-description = 查看上下文使用率和会话统计
command-agents-description = 打开 Agent 选择面板
command-mcp-description = 管理 MCP 服务器连接
command-memory-description = 编辑用户/项目级 CLAUDE.md 记忆文件
command-history-description = 打开历史对话浏览面板
command-loop-description = 注册定时循环任务（自然语言描述，如 /loop 每隔5分钟提醒我喝水）
command-cron-description = 查看和管理定时任务
command-tasks-description = 查看 agent 线程和定时任务
command-plugin-description = 管理插件（浏览、安装、卸载）
command-config-description = 全局配置（autocompact、语言、系统提示词覆盖）
command-hooks-description = 查看 Hook 配置
command-effort-description = 查看或设置推理力度（low/medium/high/xhigh/max）
command-doctor-description = 诊断配置完整性
command-rename-description = 查看或修改当前会话标题
command-lang-description = 切换界面语言（如 /lang zh-CN）
command-setup-description = 打开配置向导，设置 Provider
command-init-description = 生成或优化项目 CLAUDE.md 知识库
command-agent-description = 设置 Agent 定义，切换不同的 Agent 角色

# ---- Command Execution Messages ----

# help command
help-available-commands = 可用命令：
help-alias-prefix = （别名: /{ $aliases }）
help-skills-count = Skills（{ $count } 个可用）: 输入 # 前缀查看
help-skills-empty = Skills: 将 .md 文件放入 .claude/skills/ 目录即可添加
help-shortcuts = 快捷键：Shift+Tab 切换权限模式 │ { $palette } 命令面板（选择模型/Provider） │ Shift+Enter 换行 │ Esc 退出 │ Ctrl+C 中断

# compact command
compact-agent-running = Agent 运行中，无法执行压缩

# history command
history-agent-running = Agent 运行中，无法打开历史面板

# model command
config-save-failed = 配置保存失败: { $error }

# effort command
effort-set = 推理力度已设为 { $effort }
effort-current = 当前推理力度: { $effort }
effort-usage = 用法: /effort low|medium|high|xhigh|max

# loop command
loop-usage = 用法: /loop <自然语言时间描述> <提示词>
loop-example = 例如: /loop 每隔5分钟提醒我喝水

# rename command
rename-no-session = 当前无活跃会话，无法重命名
rename-current-title = 当前标题: { $title }
rename-updated = 会话标题已更新为: { $name }
rename-failed = 重命名失败: { $error }
rename-untitled = （无标题）

# doctor command
doctor-header = Doctor 检查结果：
doctor-settings-missing = Missing  无法获取 home 目录
doctor-no-api-key = Missing  未设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY
doctor-no-provider = No Provider  未配置任何 Provider
doctor-provider-not-found = No Provider  active_provider_id '{ $id }' 未找到
doctor-no-mcp = None  未配置 MCP 服务器
doctor-mcp-not-init = None  .mcp.json 存在但 MCP 未初始化
doctor-no-alias = No Alias  未配置任何模型别名
doctor-no-alias-no-provider = No Alias  无活跃 Provider
doctor-no-alias-not-configured = No Alias  未配置

# lang command
lang-switched = 语言已切换为 { $lang }
lang-available = 可用语言: { $langs }
lang-unsupported = 不支持的语言: { $lang }

# ---- Status Bar ----

statusbar-permission-default = Default
statusbar-permission-dont-ask = Don't Ask
statusbar-permission-accept-edit = Accept Edit
statusbar-permission-auto = Auto Mode
statusbar-permission-bypass = Bypass
statusbar-permission-cycle-hint = (Shift+Tab 切换)
statusbar-copied = 已复制 { $count } 个字符
scroll-back-to-bottom = ↓ 回到最新 · End
scroll-back-to-bottom-new = ↓ 有新活动 · End
statusbar-no-agent = 无
statusbar-bg-indicator = [BG: { $count }]
statusbar-retrying = 重试 { $attempt }/{ $max } ({ $delay }s): { $error }
statusbar-notice = 系统提示：{ $message }
statusbar-mcp-connecting =  MCP ({ $connected }/{ $total })...
statusbar-mcp-ready =  MCP 就绪 ({ $total } 个服务器)
statusbar-mcp-failed =  MCP 失败: { $msg }
statusbar-lsp-diag = 诊断: { $errors }E/{ $warnings }W

# ---- Status Bar Shortcut Hints (main view) ----

key-command = 命令
key-switch-session = :切换Session
key-close = :关闭
key-scroll = :滚动
key-jump = :跳转
key-cancel = :取消
key-newline = :换行
key-open-browser = :打开浏览器
key-submit = :提交
key-switch = :切换
key-switch-tab = :切换标签
key-move = :移动
key-select = :选择
key-confirm = :确认
key-delete = :删除
key-reconnect = :重连
key-exit-detail = :退出详细
key-detail = :详情
key-execute = :执行
key-back = :返回
key-install = :安装
key-tab = :切换
key-effort = :力度
key-switch-model = :切换模型
key-command-palette = :命令面板
key-tab-mode = :切换模式
key-settings = :设置
key-nav = :导航
key-edit = :编辑
key-kill = :停止
key-bg-command = :选中命令转后台

# ---- Welcome Page ----

welcome-title = CC Code Agent Framework
welcome-divider = ────── 我能做什么？ ──────
welcome-feature-code = 让我帮你编写、调试或重构代码
welcome-feature-files = 管理文件和运行终端命令
welcome-feature-agents = 将任务委派给专业子 Agent
welcome-login-hint-1 = 请输入
welcome-login-hint-2 = 配置 API Key 开始使用
welcome-shortcut-quit = :退出
welcome-shortcut-stop = :停止
welcome-shortcut-newline = :换行
welcome-shortcut-mode = :模式
welcome-shortcut-model = :模型
welcome-skills-available = { $count } 个 skills 可用

# ---- Tips (18 items) ----

tip-0 = 按 / 输入命令，Tab 补全
tip-1 = Ctrl+C 中断 Agent，Shift+Tab 切换权限模式
tip-2 = Ctrl+P / Alt+P 打开命令面板（选择 Provider / Model / Effort）
tip-3 = Shift+Enter 在输入框中换行
tip-4 = 拖拽图片自动添加附件，拖拽文件插入路径
tip-5 = Alt+V / Ctrl+V 粘贴剪贴板图片（推荐 Alt+V 规避终端拦截）
tip-6 = PageUp/Down 滚动消息（输入框空时），Home/End 跳顶/底，Ctrl+↑/↓ 达边界时浏览历史
tip-7 = Ctrl+B 后台化前台任务，/history 浏览与切换历史会话
tip-8 = Esc 关闭弹窗或面板，Enter 确认选择
tip-9 = /compact 压缩上下文节省 token
tip-10 = /clear 清空当前对话
tip-11 = /model 切换 LLM 模型
tip-12 = /history 浏览历史对话记录
tip-13 = /loop 创建定时循环任务
tip-14 = /plugin 管理 Claude Code 插件
tip-15 = 在 .claude/skills/ 中添加自定义 Skills
tip-16 = 在 .claude/agents/ 中定义 SubAgent
tip-17 = 对复杂任务让 Agent 先制定计划再执行

# ---- Setup Wizard ----

setup-welcome-title =  ── CC Code 设置 ── 欢迎
setup-choose-provider =  选择如何配置你的 Provider：
setup-source-custom-api = Custom API
setup-source-migrate = 从 Claude Code 迁移
setup-source-custom-desc = 手动输入 Provider 详情
setup-source-migrate-desc = 从 ~/.claude/ 导入配置
setup-key-confirm = :确认
setup-key-select = :选择
setup-key-quit = :退出
setup-configure-title =  ── CC Code 设置 ── 配置 Providers
setup-submit = 提交
setup-key-edit-submit = :编辑/提交
setup-key-check = :勾选
setup-key-back = :返回
setup-edit-title =  ── 设置 ── 编辑: { $type } ({ $id })
setup-field-type = 类型
setup-field-id = ID
setup-field-base-url = 基础URL
setup-hint-base-url-v1 = OpenAI Base URL 需要 /v1 后缀
setup-field-api-key = API密钥
setup-field-opus = 旗舰
setup-field-sonnet = 标准
setup-field-haiku = 极速
setup-field-fable = Fable
setup-model-label = Model
setup-label-key = 密钥：
setup-provider-anthropic = Anthropic
setup-provider-openai = OpenAI 兼容
setup-confirm = 确认
setup-test-connectivity = [ 测试联通性 ]
setup-key-switch-type = :切换类型
setup-key-back-list = :返回列表
setup-complete-title =  ── 设置完成 ✓
setup-press-enter = 按
setup-to-start = 开始使用
setup-no-key = (无密钥)
setup-no-providers = 未配置任何 Provider，请选择"Custom API"或从 Claude Code 导入。

setup-language-title = ── CC Code 设置 ── 语言
setup-language-prompt = 选择界面语言：
setup-language-press-enter = 按 Enter 确认

# ---- Config Panel ----

config-panel-title =  /config — 配置
config-field-autocompact = Autocompact
config-field-compact-threshold = Compact 阈值
config-field-language = 语言
config-field-persona = Persona
config-field-tone = Tone
config-field-proactiveness = Proactiveness
config-field-diff = 内联 Diff
config-field-recap = 会话回顾
config-value-on = 开
config-value-off = 关
config-saved = 配置已保存

# Config panel groups
config-group-general = 通用
config-group-prompt-overrides = 提示词覆盖

# Config field descriptions
config-desc-autocompact = （开/关 — 上下文满时自动压缩）
config-desc-threshold = 50-99% — 自动压缩触发阈值
config-desc-recap = （开/关 — 终端失焦60秒后自动生成会话回顾）
config-desc-language = en, zh-CN，或留空为自动
config-desc-persona = 覆盖系统提示词 persona（留空=默认）
config-desc-tone = 覆盖系统提示词 tone（留空=默认）
config-desc-proactiveness = low / medium / high — agent 主动性级别
config-desc-diff = （开/关 — 显示 Write/Edit 工具的内联 diff）
config-field-streaming = 渲染模式
config-desc-streaming = streaming / block / none — LLM 输出渲染粒度

# ---- Login Panel ----

login-panel-title-browse =  /login — Provider 管理
login-panel-title-edit =  /login — 编辑 Provider
login-panel-title-new =  /login — 新建 Provider
login-panel-title-confirm-delete =  /login — 确认删除
login-no-model = （未设置）
login-empty-hint =   （无 provider，按 Ctrl+N 新建）
login-confirm-delete-label =  确认删除
login-confirm-delete-question =  ？
login-key-activate = :激活
login-key-new = :新建
login-key-delete = :删除
login-key-paste = :粘贴
login-confirm-delete = :确认删除

# ---- HITL Popup ----

hitl-single-title =  ⚠ 工具审批 (1 项)
hitl-batch-title =  ⚠ 批量工具审批
hitl-choice-once = [同意本次]
hitl-choice-session = [本次会话同意]
hitl-choice-reject = [拒绝]
hitl-approved = [批准]
hitl-rejected = [拒绝]
hitl-key-hint = ↑↓ 选择 · Tab 切换工具 · Enter 确认 · Esc 全部拒绝
hitl-summary = 已选: { $approved } 批准 / { $rejected } 拒绝
hitl-run-command = 运行命令
hitl-edit-file = 编辑文件
hitl-read-file = 读取文件
hitl-write-file = 写入文件
hitl-search = 搜索
hitl-call-tool = 调用工具
hitl-working-directory = 执行目录
hitl-description-once = 仅允许这次调用
hitl-description-session-command = 本次会话内，相同命令不再询问
hitl-description-session-file = 本次会话内，此工具操作该文件不再询问
hitl-description-session-tool = 本次会话内，此工具相同参数不再询问
hitl-description-reject = 阻止这次调用
hitl-diff-title = 修改对比 · − 删除 · + 新增
hitl-selected-choice = { $choice }
hitl-selection-target = 审批第 { $current }/{ $count } 项 · { $tool }
hitl-summary-three = 本次同意 { $once } · 会话同意 { $session } · 拒绝 { $rejected }
hitl-hidden-lines = … 还有 { $count } 行未显示 · 请扩大终端

# ---- AskUser Popup ----

ask-user-placeholder = 输入自定义内容...

# ---- App Messages ----

app-provider-ready = { $name } ({ $model }) 已就绪
app-not-configured = 未配置
app-empty = 无
app-no-api-key-warning = 警告: 未设置任何 API Key（ANTHROPIC_API_KEY 或 OPENAI_API_KEY）
app-interrupted-resumed = 已强制中断
app-interrupt-done = 已中断
app-interrupted-background = 已强制中断
app-config-saved = 配置已保存
app-config-save-failed = 配置保存失败: { $error }
app-provider-activated = 已激活 Provider: { $name }
app-provider-created = 已新建并激活 Provider: { $name }
app-provider-saved = 已保存并激活 Provider: { $name }
app-provider-deleted = 已删除 Provider: { $name }
app-provider-name-empty = 保存失败：Provider 名称不能为空
app-agent-reset = Agent 已重置（未设置 agent_id）
app-agent-switched = Agent 已切换为: { $name } ({ $id })
app-agent-disconnected = Agent 连接异常断开，请重试发送消息
app-compact-no-context = 无可压缩的上下文（历史消息为空）
app-compact-started = 正在压缩上下文…
app-compact-no-provider = 压缩失败: 未配置 LLM Provider（请设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY）
app-compact-compressing = 压缩上下文
app-compact-done = 上下文已压缩
app-compact-failed = 压缩失败: { $error }
app-compact-auto-cleared = 自动清理：释放了 { $count } 个工具调用结果
app-compact-limit-reached = 上下文压缩后仍超出限制，已停止自动继续。请使用 /compact 手动压缩或 /clear 清空历史。
app-recap-hint = （可在 /config 中关闭会话回顾）
app-recap-failed = 回顾生成失败: { $error }
app-model-switched = 模型已切换为: { $alias } ({ $effort } effort)
app-1m-context-enabled = 已启用 1M 上下文模式（context window: 1,000,000 tokens）
app-prompt-cache-low = Prompt cache 命中率 { $rate }% < 80% (req: { $req })
app-no-mcp-configured = 无 MCP 服务器配置（请在 .mcp.json 或 settings.json 中添加）
app-no-cron-tasks = 无定时任务
app-cron-deleted = 已删除定时任务: { $preview }
app-submit-attachments = { $input } [ { $count } 张图片 ]
attachment-title = 待发送附件
attachment-del-hint = Del: 删除最后一张
app-no-provider-submit = 未配置 API Key，请输入 /login 配置 Provider
app-bg-task-done = [后台任务 { $id } 已完成] Agent: { $agent } | 工具调用: { $tools } | 耗时: { $duration }ms
app-bg-task-done-with-result = [后台任务 { $id } 已完成] Agent: { $agent } | 工具调用: { $tools } | 耗时: { $duration }ms\n结果:\n{ $result }
app-bg-task-failed = [后台任务 { $id } 执行失败] Agent: { $agent } | { $error }
app-bg-task-failed-with-error = [后台任务 { $id } 执行失败] Agent: { $agent }\n错误:\n{ $error }
app-bg-continuation = 正在回顾 { $count } 个后台 Agent 结果...
app-bg-empty = 暂无后台任务（前台命令运行时 Ctrl+B 可转入后台）
app-bg-foreground-tag = [前台] { $command }
app-bg-task-missing = 任务不存在

# ---- Panel Status Bar Hints ----

# Login panel
hint-login-browse = :导航
hint-login-activate = :激活
hint-login-edit = :编辑
hint-login-new = :新建
hint-login-delete = :删除
hint-login-close = :关闭
hint-login-field = :字段
hint-login-save = :保存
hint-login-paste = :粘贴
hint-login-toggle = :切换
hint-login-back = :返回

# Config panel
hint-config-field = :字段
hint-config-toggle = :切换
hint-config-save = :保存并关闭

# Model panel
hint-model-navigate = :导航
hint-model-confirm = :确认
hint-model-effort = :Effort
hint-model-close = :关闭

# Agent panel
hint-agent-select = :选择
hint-agent-confirm = :确认
hint-agent-cancel = :取消

# MCP panel
hint-mcp-navigate = :导航
hint-mcp-detail = :详情
hint-mcp-reconnect = :重连
hint-mcp-delete = :删除
hint-mcp-execute = :执行
hint-mcp-back = :返回
hint-mcp-close = :关闭

# ---- MCP Panel Content ----

mcp-server-count = { $count } 个服务器
mcp-section-project = 项目 MCP
mcp-section-project-path = 项目 MCP ({ $path })
mcp-section-user = 用户 MCP
mcp-section-user-path = 用户 MCP ({ $path })
mcp-section-plugin = 插件 MCP
mcp-no-servers = 未配置 MCP 服务器。编辑 .mcp.json 或 settings.json
mcp-panel-title = 管理 MCP 服务器
# Status
mcp-status-connected = 已连接
mcp-status-needs-auth = 需要认证
mcp-status-error = 错误
mcp-status-disabled = 已禁用
mcp-status-uninitialized = 未初始化
mcp-status-offline = 离线
# Auth
mcp-auth-authenticated = 已认证
mcp-auth-none = 无
# Labels
mcp-label-status = 状态:
mcp-label-auth = 认证:
mcp-label-url = URL:
mcp-label-config-location = 配置位置:
mcp-label-plugin = 插件
mcp-label-plugin-source = 插件 - { $source }
mcp-label-capabilities = 能力:
mcp-label-tools = 工具:
mcp-label-tools-count = { $count } 个工具
# Capabilities
mcp-capability-tools = 工具
mcp-capability-resources = 资源
# Actions
mcp-action-hide-tools = 隐藏工具
mcp-action-view-tools = 查看工具
mcp-action-reauthenticate = 重新认证
mcp-action-clear-auth = 清除认证
mcp-action-reconnect = 重新连接
mcp-action-disable = 禁用
mcp-action-enable = 启用
# OAuth Messages
mcp-oauth-completed = [i] OAuth 授权完成: { $server }
mcp-oauth-failed = [i] OAuth 授权失败: { $server } - { $error }
mcp-clear-auth-ok = [i] OAuth 凭证已清除: { $server }
mcp-clear-auth-failed = [i] 清除 OAuth 凭证失败: { $server }
mcp-action-ok = [i] 操作完成: { $server }
mcp-action-failed = [i] 操作失败: { $server }

# Plugin panel
hint-plugin-uninstall = :确认卸载
hint-plugin-cancel = :取消
hint-plugin-delete = :确认删除
hint-plugin-add = :添加
hint-plugin-exit-search = :退出搜索
hint-plugin-tab = :Tab
hint-plugin-install = :安装
hint-plugin-remove = :Remove
hint-plugin-navigate = :导航
hint-plugin-execute = :执行
hint-plugin-back = :返回列表
hint-plugin-select = :选择
hint-plugin-search = :搜索

# Cron panel
hint-cron-confirm-delete = :确认删除
hint-cron-navigate = :导航
hint-cron-toggle = :切换
hint-cron-delete = :删除
hint-cron-close = :关闭

# Status panel
hint-status-tab = :切换Tab
hint-status-close = :关闭

# History panel
hint-history-confirm-delete = :确认删除
hint-history-exit-search = :退出搜索
hint-history-close = :关闭

# Hooks panel
hint-hooks-navigate = :导航
hint-hooks-close = :关闭

# Memory panel
hint-memory-select = :选择
hint-memory-edit = :编辑
hint-memory-close = :关闭

# ---- Plugin Panel Messages ----

app-plugin-updating = 正在更新 marketplace: { $name }
app-plugin-delete-failed = 删除失败: { $error }
app-plugin-add-failed = 添加失败: { $error }
app-plugin-added = Marketplace 已添加: { $name } (正在获取内容...)

# 后台 Agent 管理栏
bg-bar-focus-hint = 按 Esc 退出聚焦

# 退出提示
app-exit-resume = 恢复会话: cc-code -r { $id }

# ---- Spinner / Widget ----

spinner-thinking = 思考中…
spinner-tool-use = 执行工具…
spinner-responding = 正在生成回复…
spinner-thinking-header = 💭 思考 ({ $count } chars)

# 执行中的消息队列
queue-label = 排队
queue-steer = 立即补充
queue-keys-tip = Ctrl+Enter 立即补充 · Ctrl+X 删除
queue-sending = 插入中…
queue-attachments = { $count } 张图片
queue-unavailable = 当前执行暂不支持补充，消息仍保留在队列中。
queue-steer-failed = 本轮未接收补充信息，消息已保留，将按队列顺序发送。

# ---- 后台 shell 通知（展示文本） ----
shell-notify-failed = 后台 shell 失败
shell-notify-timed-out = 后台 shell 已超时终止
shell-notify-cancelled = 后台 shell 已取消
shell-notify-terminated = 后台 shell 已终止
shell-notify-completed = 后台 shell 已完成
shell-notify-waiting-input = 后台 shell 等待输入

# 命令、shell 输入与状态面板
command-ambiguous = 命令 '{ $command }' 匹配多个: { $matches } （请输入完整命令名）
command-unknown = 未知命令或 Skill: { $command } （输入 /help 查看可用命令）
shell-enter-command = 请输入 shell 命令
shell-task-running = 已有任务正在运行，暂不能启动 shell 命令
shell-stdin-closed = shell stdin 已关闭
shell-session-failed = 创建 shell 会话失败: { $error }
rewind-no-turns = 没有可回退的对话轮次
status-title = 状态
status-tab-cost = 费用
status-tab-context = 上下文
status-duration = 会话时长
status-input-tokens = 输入 Tokens
status-output-tokens = 输出 Tokens
status-cache-creation = Cache 创建
status-cache-read = Cache 读取
status-model-calls = LLM 调用次数
status-estimated-cost = 估算费用
status-current-model = 当前模型
status-context = 上下文
status-used = 已用
status-messages = 消息
status-tools = 工具
status-no-requests = 暂无请求数据
status-uncached = 未缓存
status-cache-hit-rate = 缓存命中率
status-hit = 命中
# /gc diagnostics
cmd-gc-desc = 手动触发内存回收并显示 RSS 变化和数据结构诊断
cmd-gc-rss = RSS: { $before } → { $after } ({ $delta })
cmd-gc-allocated-rss = { $allocator } allocated: { $allocated } (allocated-RSS = { $delta }；{ $hint })
cmd-gc-os-rss = OS RSS: { $before } → { $after } ({ $delta })
cmd-gc-origin-messages = origin_messages:  { $count } 条, ~{ $bytes }
cmd-gc-completed-messages = pipeline.completed: { $count } 条, ~{ $bytes }
cmd-gc-view-messages = view_messages:     { $count } 条 VM, ~{ $bytes }
cmd-gc-message-comparison = origin vs completed: { $overlap } ({ $origin }/{ $completed } 条)
cmd-gc-markdown-cache = markdown_cache: { $entries }/{ $capacity } 条, 数据堆估算 ~{ $bytes }
cmd-gc-markdown-cache-details = { "  " }平均条目 ~{ $average } | 最大条目 ~{ $largest } | { $lines } 行 / { $spans } Span
cmd-gc-render-cache = render_cache: { $lines } 行 + { $wraps } wrap 映射, 数据堆估算 ~{ $bytes }（整段对话渲染结果 + 逐行纯文本副本）
cmd-gc-diff-cache = diff_cache:  { $entries }/{ $capacity } 条, ~{ $bytes }（字节预算 { $budget }）
cmd-gc-diff-render-cache = diff_render_cache: { $entries }/{ $capacity } 条, ~{ $bytes }（字节预算 { $budget }）
cmd-gc-allocator-heading = ── { $allocator } 明细 ──
cmd-gc-allocated = allocated: { $bytes } (分配器跟踪的在用分配，不保证覆盖原生库)
cmd-gc-active-mimalloc = active:    { $bytes } (mimalloc page_committed＝历史触及高水位，非当前占用，勿用于诊断)
cmd-gc-active = active:    { $bytes } (活跃页)
cmd-gc-resident-mimalloc = resident:  { $bytes } (全进程 RSS/WorkingSet，非 mimalloc 独占内存)
cmd-gc-resident = resident:  { $bytes } (分配器物理驻留估算)
cmd-gc-metadata = metadata:  { $bytes } (分配器元数据)
cmd-gc-mapped-mimalloc = mapped:    { $bytes } (reserved 虚拟地址保留量，非物理驻留量)
cmd-gc-retained-mimalloc = retained:  { $bytes } (reserved-committed 保留未提交量，非物理驻留量)
cmd-gc-mapped = mapped:    { $bytes } (映射)
cmd-gc-retained = retained:  { $bytes } (保留未归还 OS)
cmd-gc-fragmentation-mimalloc = 碎片: active-allocated={ $fragmentation } （⚠ 基于 mimalloc touched 高水位，非真实碎片，忽略）
cmd-gc-fragmentation = 碎片: active-allocated={ $fragmentation } | resident-active={ $waste }
cmd-gc-os-gap = OS RSS({ $rss }) - { $allocator } resident({ $resident }) = { $gap }
cmd-gc-full-stats-heading = ── { $allocator } 全量统计（见日志）──
cmd-gc-known-estimate = 已知合计估算: { $known } (消息 { $messages } + VM { $views } + Markdown 缓存 { $markdown } + 渲染缓存 { $render } + diff 缓存 { $diff }) | allocated 内未识别估算: { $gap }
cmd-gc-allocated-rss-hint-positive = allocated 大于当时 RSS（采样时刻/记账口径差，仅供参考）
cmd-gc-allocated-rss-hint-negative = RSS 差额可能含分配器空闲页/页舍入/原生库分配/栈/映射文件等，不能据此归因
cmd-gc-rss-unavailable = RSS: 不可用（分配器 stats 读取失败）
cmd-gc-structures-heading = ── 数据结构诊断 ──
cmd-gc-message-counts-equal = 条数相同（未比较内容；origin 为 agent 权威历史，completed 为渲染管线基线）
cmd-gc-message-counts-different = 条数不同（未比较内容）
cmd-gc-caches-heading = ── 渲染缓存 ──
cmd-gc-cache-estimate-note = { "  " }口径：按容器/自有字符串 capacity 估算，不含 LRU/分配器开销，非 RSS。
cmd-gc-metadata-mimalloc = metadata:  不可用（committed 与 page_committed 口径不同，差值不能估算元数据）
cmd-gc-estimate-note = 注：估算仍未覆盖语法高亮引擎(SyntaxSet/ThemeSet)/编译正则/ACP 缓冲/tokio/tracing/分配器元数据等；余量来源待定位，不能据此判断是否泄漏。
cmd-gc-collection-scope = 回收范围：当前线程立即回收；TUI 工作线程在下次空闲时响应请求，不保证本次采样已完成。不会清空应用缓存或释放存活对象。

# CLI
cli-command-cc-code = CC Code AI 智能助手
cli-command-acp = 以 ACP Agent 模式运行（stdin/stdout JSON-RPC）
cli-command-update = 从 GitHub 下载并安装最新版本
cli-command-sync = 在设备间同步 settings、skills、MCP 和插件
cli-command-plugin = 插件管理
cli-command-sender = 发送本地配置到远端设备
cli-command-receiver = 从远端设备接收配置
cli-command-list = 列出已安装的插件
cli-command-install = 安装插件
cli-command-uninstall = 卸载插件
cli-help-yolo = 免审批模式（默认关闭）
cli-help-approve = 启用审批（等同 --permission-mode auto）
cli-help-print = 非交互模式：输出响应后退出
cli-help-output-format = 输出格式：text / json / stream-json（需 -p）
cli-help-max-turns = 最大 agentic 轮数（需 -p）
cli-help-bare = 跳过 hooks、LSP、插件和 MCP 初始化（需 -p）
cli-help-permission-mode = 权限模式：auto / bypass
cli-help-skip-permissions = 绕过所有权限检查
cli-help-model = 模型别名（opus / sonnet / haiku / fable）或全名
cli-help-effort = 推理强度：low / medium / high / max
cli-help-cont = 继续当前目录最近的对话
cli-help-resume = 按 session ID 恢复对话
cli-help-session-id = 指定会话 ID（必须是有效 UUID）
cli-help-session-name = 设置会话显示名称
cli-help-no-session-persistence = 禁用会话持久化（需 -p）
cli-help-allowed-tools = 允许的工具列表，例如 Bash(git:*)、Edit
cli-help-disallowed-tools = 禁止的工具列表
cli-help-settings = 加载额外 settings 文件或 JSON 字符串
cli-help-cwd = 工作目录
cli-help-agent = Agent 类型（从 .claude/agents/ 中选择）
cli-help-server = Relay 服务器地址
cli-help-json = JSON 输出
cli-help-plugin = 插件名称或 ID（name@marketplace）
cli-help-scope = 插件范围：user / project / local
cli-help-uninstall-scope = 卸载范围（不指定则从所有范围移除）
cli-settings-read-warning = 警告：无法读取 settings 文件 '{ $path }'：{ $error }
cli-settings-json-warning = 警告：--settings 内容不是有效的 JSON
cli-output-format-error = 未知的输出格式：'{ $value }'（可选值：text、json、stream-json）
cli-plugin-scope-error = 未知的插件范围：'{ $value }'（可选值：user、project、local）
cli-update-success = 已更新至 { $tag }
cli-update-failed = 更新失败：{ $error }
cli-no-session-id = 无可用的会话 ID
cli-error = 错误：{ $error }
cli-empty-prompt = 无输入 prompt。用法：cc-code -p "你的问题" 或 echo "问题" | cc-code -p
cli-settings-invalid = --settings 不是有效文件路径或 JSON：{ $error }
cli-no-provider = 未配置 LLM provider。请设置 ANTHROPIC_API_KEY 或 OPENAI_API_KEY
cli-plugin-empty = 未安装任何插件。
cli-plugin-version = 版本
cli-plugin-marketplace = 市场
cli-plugin-status = 状态
cli-plugin-enabled = 已启用
cli-plugin-disabled = 已禁用
cli-plugin-install-failed = 安装失败：{ $error }
cli-plugin-installed = 已安装：{ $id } v{ $version }（范围：{ $scope }）
cli-plugin-uninstall-failed = 卸载失败：{ $error }
cli-plugin-uninstalled = 已卸载：{ $id }

# 文件提及弹窗
at-mention-searching = 搜索中…
at-mention-empty-directory = 当前目录无候选，请输入关键词搜索。
at-mention-no-matches = 没有匹配的文件。
