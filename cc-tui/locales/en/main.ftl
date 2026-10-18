# ============================================================
# Peri TUI — English (en) Translation File
# This is the fallback language; keys missing in other
# languages resolve to the values defined here.
# ============================================================

# ---- i18n infrastructure test keys ----
test-hello = Hello, World!
test-greeting = Hello, { $name }!

# ---- Command Descriptions ----

command-help-description = List all available commands
command-clear-description = Clear message list
command-exit-description = Exit the application
command-model-description = Open model selection panel (Provider + Level + Thinking); with args, switch alias directly (opus/sonnet/haiku/fable)
command-theme-description = Open theme selection panel; with args, switch theme directly (default/dark/light/light-daltonized/dark-daltonized/light-ansi/dark-ansi)
command-login-description = Manage Provider configuration (create/edit/delete)
command-cost-description = View current session cost and token usage
command-context-description = View context usage and session statistics
command-agents-description = Open Agent selection panel
command-mcp-description = Manage MCP server connections
command-memory-description = Edit user/project-level CLAUDE.md memory files
command-history-description = Browse input history (list view of Ctrl+↑/↓)
command-resume-description = Open conversation history browser
command-loop-description = Register scheduled loop task (natural language description, e.g. /loop remind me to drink water every 5 minutes)
command-cron-description = View and manage scheduled tasks
command-plugin-description = Manage plugins (browse, install, uninstall)
command-config-description = Global configuration (autocompact, language, system prompt overrides)
command-hooks-description = View Hook configuration
command-effort-description = View or set reasoning effort (low/medium/high/xhigh/max)
command-doctor-description = Diagnose configuration completeness
command-rename-description = View or modify current session title
command-rewind-description = Open the rewind picker and roll back to an earlier message (alias undo)
command-lang-description = Switch interface language (e.g. /lang zh-CN)
command-setup-description = Open setup wizard to configure providers
command-init-description = Generate or optimize project CLAUDE.md knowledge base
command-agent-description = Set Agent definition, switch different Agent roles
command-export-description = Export conversation to a file or the clipboard
command-channel-description = Manage MCP channel connections: open <source> / close / status

# ---- Command Execution Messages ----

# help command
help-available-commands = Available commands:
help-alias-prefix = (aliases: /{ $aliases })
help-skills-count = Skills ({ $count } available): type # prefix to view
help-skills-empty = Skills: place .md files in .claude/skills/ directory to add
help-shortcuts = Shortcuts: Shift+Tab toggle permission mode | { $palette } command palette (select Model / Provider) | Shift+Enter newline | Esc quit | Ctrl+C interrupt

# compact command

# history command

# model command
config-save-failed = Configuration save failed: { $error }

# effort command
effort-set = Reasoning effort set to { $effort }
effort-current = Current reasoning effort: { $effort }
effort-usage = Usage: /effort low|medium|high|xhigh|max

# loop command
loop-usage = Usage: /loop <natural language time description> <prompt>
loop-example = Example: /loop remind me to drink water every 5 minutes

# rename command
rename-no-session = No active session, cannot rename
rename-current-title = Current title: { $title }
rename-updated = Session title updated to: { $name }
rename-failed = Rename failed: { $error }
rename-untitled = (untitled)

# doctor command
doctor-header = Doctor check results:
doctor-settings-missing = Missing  Cannot get home directory
doctor-no-api-key = Missing  No API Key set (ANTHROPIC_API_KEY or OPENAI_API_KEY)
doctor-no-provider = No Provider  No provider configured
doctor-provider-not-found = No Provider  active_provider_id '{ $id }' not found
doctor-no-mcp = None  No MCP servers configured
doctor-mcp-not-init = None  .mcp.json exists but MCP not initialized
doctor-no-alias = No Alias  No model aliases configured
doctor-no-alias-no-provider = No Alias  No active Provider
doctor-no-alias-not-configured = No Alias  Not configured
doctor-ok = OK  { $detail }
doctor-missing-path = Missing  { $path }
doctor-mcp-ok = OK  MCP pool initialized
doctor-table-header = | Check | Status | Detail |

# lang command
lang-switched = Language switched to { $lang }
lang-available = Available languages: { $langs }
lang-unsupported = Unsupported language: { $lang }

# export command
export-cwd-error = Cannot get cwd: { $error }
export-clipboard-error = Clipboard error: { $error }
export-written = Exported to: { $path }
export-write-error = Write error: { $error }

# channel command
channel-usage = Usage: /channel open <source> | /channel close | /channel status
channel-not-initialized = Channel system is not initialized
channel-unsupported = Server { $server } does not support channels or is not connected
channel-opened = Channel opened: { $source }
channel-closed = Channel closed: { $server }
channel-all-closed = All channels closed
channel-none-open = No open channels. Use /channel open <source> to open one
channel-open-list = Open channels:

# session switching
thread-busy = This session still has running tasks, queued messages or pending approvals; finish them before switching
thread-cwd-unavailable = Project directory unavailable, cannot enter session: { $cwd }
thread-config-unavailable = Project config unavailable, cannot enter session: { $error }
thread-load-failed = Failed to load session: { $error }
thread-enter-failed = Failed to enter session, kept the current session: { $error }
thread-new-failed = Failed to create session, kept the current session: { $error }

# ---- Status Bar ----

statusbar-permission-auto = Auto Mode
statusbar-permission-bypass = Bypass
statusbar-permission-cycle-hint = (Shift+Tab to cycle)
statusbar-copied = Copied { $count } chars to clipboard
scroll-back-to-bottom = ↓ Back to bottom · End
scroll-back-to-bottom-new = ↓ New activity · End
statusbar-retrying = Retry { $attempt }/{ $max } ({ $delay }s): { $error }
statusbar-notice = Notice: { $message }
statusbar-mcp-connecting =  MCP ({ $connected }/{ $total })...
statusbar-mcp-ready =  MCP ready ({ $total } servers)
statusbar-mcp-failed =  MCP failed: { $msg }
statusbar-lsp-diag = diag: { $errors }E/{ $warnings }W

# ---- Status Bar Shortcut Hints (main view) ----

key-close = :Close
key-cancel = :Cancel
key-open-browser = :Open browser
key-submit = :Submit
key-switch = :Switch
key-move = :Move
key-select = :Select
key-fill-input = :Fill input
key-confirm = :Confirm
key-delete = :Delete
key-reconnect = :Reconnect
key-exit-detail = :Exit Verbose
key-detail = :Detail
key-execute = :Execute
key-back = :Back
key-install = :Install
key-tab = :Tab
key-effort = :Effort
key-settings = :Settings
key-nav = :Navigate
key-edit = :Edit
key-kill = :Stop
key-bg-command = :Background command

# ---- Welcome Page ----

welcome-title = CC Code Agent Framework
welcome-divider = ────── What can I do? ──────
welcome-feature-code = Ask me to code, debug, or refactor
welcome-feature-files = Manage files and run terminal commands
welcome-feature-agents = Delegate tasks to specialized sub-agents
welcome-login-hint-1 = Please type
welcome-login-hint-2 = to configure API Key to get started
welcome-shortcut-quit = :Quit
welcome-shortcut-stop = :Stop
welcome-shortcut-newline = :NewLine
welcome-shortcut-mode = :Mode
welcome-shortcut-model = :Model
welcome-skills-available = { $count } skills available

# ---- Tips (18 items) ----

tip-0 = Type / to enter commands, Tab to autocomplete
tip-1 = Ctrl+C interrupts Agent, Shift+Tab toggles permission mode
tip-2 = Ctrl+P / Alt+P opens command palette (Provider / Model / Effort)
tip-3 = Shift+Enter for newline in input box
tip-4 = Drag images to attach, drag files to insert path
tip-5 = Alt+V / Ctrl+V pastes clipboard image (Alt+V avoids terminal intercept)
tip-6 = PageUp/Down scrolls messages (when input empty), Home/End jumps top/bottom, Ctrl+Up/Down browses history at boundary
tip-7 = Ctrl+B backgrounds running task, /resume browses and switches sessions
tip-8 = Esc closes popup or panel, Enter confirms selection
tip-9 = /compact compresses context to save tokens
tip-10 = /clear clears current conversation
tip-11 = /model switches LLM model
tip-12 = /resume browses conversation history
tip-13 = /loop creates scheduled loop tasks
tip-14 = /plugin manages Claude Code plugins
tip-15 = Add custom Skills in .claude/skills/
tip-16 = Define SubAgents in .claude/agents/
tip-17 = For complex tasks, have Agent plan first before executing
tip-clear-override = Use /clear to start fresh when switching topics and free up context

# ---- Setup Wizard ----

setup-welcome-title =  ── CC Code Setup ── Welcome
setup-choose-provider =  Choose how to configure your provider:
setup-source-custom-api = Custom API
setup-source-migrate = Migrate from Claude Code
setup-source-custom-desc = Manually enter provider details
setup-source-migrate-desc = Import config from ~/.claude/
setup-key-confirm = :Confirm
setup-key-select = :Select
setup-key-quit = :Quit
setup-configure-title =  ── CC Code Setup ── Configure Providers
setup-submit = Submit
setup-key-edit-submit = :Edit/Submit
setup-key-check = :Check
setup-key-back = :Back
setup-edit-title =  ── Setup ── Edit: { $type } ({ $id })
setup-field-type = Type
setup-field-id = ID
setup-field-base-url = Base URL
setup-hint-base-url-v1 = OpenAI base URL needs /v1 suffix
setup-field-api-key = API Key
setup-field-opus = Opus
setup-field-sonnet = Sonnet
setup-field-haiku = Haiku
setup-field-fable = Fable
setup-model-label = Model
setup-label-key = Key:
setup-provider-anthropic = Anthropic
setup-provider-openai = OpenAI Compatible
setup-confirm = Confirm
setup-test-connectivity = [ Test Connectivity ]
setup-key-switch-type = :Switch type
setup-key-back-list = :Back to list
setup-complete-title =  ── Setup Complete ✓
setup-press-enter = Press
setup-to-start = to start using
setup-no-key = (no key)
setup-no-providers = No providers configured. Add one by selecting "Custom API" or importing from Claude Code.

setup-language-title = ── CC Code Setup ── Language
setup-language-prompt = Choose your interface language:

# ---- Config Panel ----

config-panel-title =  /config — Configuration
config-field-autocompact = Autocompact
config-field-compact-threshold = Compact Threshold
config-field-language = Language
config-field-persona = Persona
config-field-tone = Tone
config-field-proactiveness = Proactiveness
config-field-diff = Inline Diff
config-field-recap = Auto Recap
config-value-on = ON
config-value-off = OFF

# Config panel groups
config-group-general = General
config-group-prompt-overrides = Prompt Overrides

# Config field descriptions
config-desc-autocompact = (ON/OFF — auto-compact context when full)
config-desc-threshold = 50-99% — trigger threshold for auto-compact
config-desc-recap = (ON/OFF — auto-generate recap after terminal unfocused for 60s)
config-desc-language = en, zh-CN, or leave empty for auto
config-desc-persona = Override system prompt persona (empty = default)
config-desc-tone = Override system prompt tone (empty = default)
config-desc-proactiveness = low / medium / high — agent initiative level
config-desc-diff = (ON/OFF — show inline diff for Write/Edit tools)
config-field-streaming = Streaming Mode
config-desc-streaming = streaming / block / none — render granularity for LLM output

# ---- Login Panel ----

login-panel-title-browse =  /login — Provider Management
login-panel-title-edit =  /login — Edit Provider
login-panel-title-new =  /login — New Provider
login-panel-title-confirm-delete =  /login — Confirm Delete
login-no-model = (not set)
login-empty-hint =   (no provider, press Ctrl+N to create)
login-confirm-delete-label =  Confirm delete
login-confirm-delete-question =  ?
login-confirm-delete = :Confirm delete

# ---- HITL Popup ----

hitl-single-title =  ⚠ Tool Approval (1 item)
hitl-batch-title =  ⚠ Batch Tool Approval
hitl-choice-once = [Allow once]
hitl-choice-session = [Allow for this session]
hitl-choice-reject = [Reject]
hitl-key-hint = ↑↓ choose · Tab switch tool · Enter confirm · Esc reject all
hitl-run-command = Run command
hitl-edit-file = Edit file
hitl-read-file = Read file
hitl-write-file = Write file
hitl-search = Search
hitl-call-tool = Call tool
hitl-working-directory = Working directory
hitl-description-once = Allow only this invocation
hitl-description-session-command = Don’t ask again for this command in this session
hitl-description-session-file = Don’t ask again for this tool on this file this session
hitl-description-session-tool = Don’t ask again for this tool with these parameters this session
hitl-description-reject = Block this invocation
hitl-diff-title = Changes · − Removed · + Added
hitl-selected-choice = { $choice }
hitl-selection-target = Deciding tool { $current }/{ $count } · { $tool }
hitl-summary-three = Once { $once } · Session { $session } · Reject { $rejected }
hitl-hidden-lines = … { $count } lines not shown · Enlarge terminal

# ---- AskUser Popup ----

ask-user-placeholder = Type something.

# ---- App Messages ----

app-provider-ready = { $name } ({ $model }) ready
app-not-configured = Not configured
app-empty = None
app-no-api-key-warning = Warning: No API Key set (ANTHROPIC_API_KEY or OPENAI_API_KEY)
app-interrupted-resumed = Force interrupted
app-interrupt-done = Interrupted
app-interrupted-background = Force interrupted
app-config-saved = Configuration saved
app-config-save-failed = Configuration save failed: { $error }
app-provider-activated = Provider activated: { $name }
app-provider-created = Provider created and activated: { $name }
app-provider-saved = Provider saved and activated: { $name }
app-provider-deleted = Provider deleted: { $name }
app-provider-name-empty = Save failed: Provider name cannot be empty
app-agent-reset = Agent reset (no agent_id set)
app-agent-switched = Agent switched to: { $name } ({ $id })
app-compact-started = Compacting context…
app-compact-done = Context compressed
app-compact-failed = Compact failed: { $error }
app-compact-auto-cleared = Auto cleanup: freed { $count } tool call results
app-recap-hint = (disable recaps in /config)
app-recap-failed = Recap failed: { $error }
app-model-switched = Model switched to: { $alias } ({ $effort } effort)
app-theme-set = Theme set to { $name }
app-theme-unknown = Unknown theme: { $name }. Available: default, dark, light, light-daltonized, dark-daltonized, light-ansi, dark-ansi
app-1m-context-enabled = 1M context mode enabled (context window: 1,000,000 tokens)
app-no-mcp-configured = No MCP servers configured (add in .mcp.json or settings.json)
app-no-cron-tasks = No cron tasks
app-cron-deleted = Cron task deleted: { $preview }
app-submit-attachments = { $input } [{ $count } image(s)]
attachment-title = Pending Attachments
attachment-del-hint = Del: remove last one
app-no-provider-submit = No API Key configured, type /login to configure Provider
app-bg-task-done = [Background task { $id } completed] Agent: { $agent } | Tool calls: { $tools } | Duration: { $duration }ms
app-bg-task-done-with-result = [Background task { $id } completed] Agent: { $agent } | Tool calls: { $tools } | Duration: { $duration }ms\nResult:\n{ $result }
app-bg-task-failed = [Background task { $id } failed] Agent: { $agent } | { $error }
app-bg-task-failed-with-error = [Background task { $id } failed] Agent: { $agent }\nError:\n{ $error }
app-bg-continuation = Reviewing { $count } background agent result(s)...
app-bg-empty = No background tasks (press Ctrl+B while a foreground command runs to background it)
app-bg-foreground-tag = [FG] { $command }
app-bg-task-missing = Task not found

# ---- Panel Status Bar Hints ----

# Login panel
hint-login-browse = :Navigate
hint-login-activate = :Activate
hint-login-edit = :Edit
hint-login-new = :New
hint-login-delete = :Delete
hint-login-close = :Close
hint-login-field = :Field
hint-login-save = :Save
hint-login-paste = :Paste
hint-login-toggle = :Toggle
hint-login-back = :Back

# Config panel
hint-config-field = :Field
hint-config-toggle = :Toggle

# Model panel

# Agent panel

# MCP panel

# ---- MCP Panel Content ----

mcp-server-count = { $count } servers
mcp-section-project = Project MCPs
mcp-section-project-path = Project MCPs ({ $path })
mcp-section-user = User MCPs
mcp-section-user-path = User MCPs ({ $path })
mcp-section-plugin = Plugin MCPs
mcp-no-servers = No MCP servers configured. Edit .mcp.json or settings.json
mcp-panel-title = Manage MCP servers
# Status
mcp-status-connected = connected
mcp-status-needs-auth = needs authentication
mcp-status-error = error
mcp-status-disabled = disabled
mcp-status-uninitialized = not initialized
mcp-status-offline = offline
# Auth
mcp-auth-authenticated = authenticated
mcp-auth-none = none
# Labels
mcp-label-status = Status:
mcp-label-auth = Auth:
mcp-label-url = URL:
mcp-label-config-location = Config location:
mcp-label-plugin = Plugin
mcp-label-plugin-source = Plugin - { $source }
mcp-label-capabilities = Capabilities:
mcp-label-tools = Tools:
mcp-label-tools-count = { $count } tools
# Capabilities
mcp-capability-tools = tools
mcp-capability-resources = resources
# Actions
mcp-action-hide-tools = Hide tools
mcp-action-view-tools = View tools
mcp-action-reauthenticate = Re-authenticate
mcp-action-clear-auth = Clear authentication
mcp-action-reconnect = Reconnect
mcp-action-disable = Disable
mcp-action-enable = Enable
# OAuth Messages
mcp-oauth-completed = [i] OAuth authorization completed: { $server }
mcp-oauth-failed = [i] OAuth authorization failed: { $server } - { $error }
mcp-clear-auth-ok = [i] OAuth credentials cleared: { $server }
mcp-clear-auth-failed = [i] Failed to clear OAuth credentials: { $server }
mcp-action-ok = [i] Action completed: { $server }
mcp-action-failed = [i] Action failed: { $server }

# Plugin panel
hint-plugin-uninstall = :Confirm uninstall
hint-plugin-delete = :Confirm delete
hint-plugin-add = :Add
hint-plugin-exit-search = :Exit search
hint-plugin-remove = :Remove
hint-plugin-search = :Search

# Cron panel
hint-cron-confirm-delete = :Confirm delete

# Status panel

# History panel
hint-history-confirm-delete = :Confirm delete

# Hooks panel

# Memory panel

# ---- Plugin Panel Messages ----

app-plugin-updating = Updating marketplace: { $name }
app-plugin-delete-failed = Delete failed: { $error }
app-plugin-add-failed = Add failed: { $error }
app-plugin-added = Marketplace added: { $name } (fetching content...)

# Background Agent Bar

# Exit prompt
app-exit-resume = Resume session: cc-code -r { $id }

# ---- Spinner / Widget ----


# Messages queued during execution
queue-label = Queued
queue-steer = Send now
queue-keys-tip = Ctrl+Enter send now · Ctrl+X delete
queue-keys-tip-macos = ⌥S send now · Ctrl+X delete
queue-keys-tip-linux = Alt+S send now · Ctrl+X delete
queue-sending = Sending…
queue-attachments = { $count } images
queue-unavailable = This execution cannot accept updates yet. Your message remains queued.
queue-steer-failed = This turn did not accept the update. Your message remains queued for the next turn.

# ---- Background shell notifications (display text) ----
shell-notify-failed = Background shell failed
shell-notify-timed-out = Background shell timed out
shell-notify-cancelled = Background shell cancelled
shell-notify-terminated = Background shell terminated
shell-notify-completed = Background shell completed
shell-notify-waiting-input = Background shell waiting for input

# Commands, shell input and status panel
command-ambiguous = Command '{ $command }' matches multiple commands: { $matches } (enter the full command name)
command-unknown = Unknown command or Skill: { $command } (type /help to view available commands)
command-busy-rejected = Agent is running; cannot run { $command } now. Wait for the current turn to finish (your input was kept)
queue-steer-command-rejected = Commands cannot be inserted into a running turn; it will run after the current turn ends
shell-enter-command = Please enter a shell command
shell-task-running = A task is already running; cannot start a shell command yet
shell-stdin-closed = Shell stdin is closed
shell-session-failed = Failed to create shell session: { $error }
rewind-no-turns = No conversation turns available to rewind
status-title = Status
input-history-title = Input history
input-history-empty = No input history yet
status-tab-cost = Cost
status-tab-context = Context
status-duration = Session duration
status-input-tokens = Input Tokens
status-output-tokens = Output Tokens
status-cache-creation = Cache creation
status-cache-read = Cache read
status-model-calls = Model calls
status-estimated-cost = Estimated cost
status-current-model = Current model
status-context = Context
status-used = Used
status-messages = Messages
status-tools = Tools
status-no-requests = No request data yet
status-uncached = Uncached
status-cache-hit-rate = Cache hit rate
status-hit = Hit
# /gc diagnostics
cmd-gc-desc = Manually collect memory and display RSS changes and data structure diagnostics
cmd-gc-rss = RSS: { $before } → { $after } ({ $delta })
cmd-gc-allocated-rss = { $allocator } allocated: { $allocated } (allocated-RSS = { $delta }; { $hint })
cmd-gc-os-rss = OS RSS: { $before } → { $after } ({ $delta })
cmd-gc-origin-messages = origin_messages: { $count } messages, ~{ $bytes }
cmd-gc-completed-messages = pipeline.completed: { $count } messages, ~{ $bytes }
cmd-gc-view-messages = view_messages: { $count } VMs, ~{ $bytes }
cmd-gc-message-comparison = origin vs completed: { $overlap } ({ $origin }/{ $completed } messages)
cmd-gc-markdown-cache = markdown_cache: { $entries }/{ $capacity } entries, estimated data heap ~{ $bytes }
cmd-gc-markdown-cache-details = { "  " }Average entry ~{ $average } | largest entry ~{ $largest } | { $lines } lines / { $spans } spans
cmd-gc-render-cache = render_cache: { $lines } lines + { $wraps } wrap mappings, estimated data heap ~{ $bytes } (rendered conversation + plain-text copies of each line)
cmd-gc-diff-cache = diff_cache: { $entries }/{ $capacity } entries, ~{ $bytes } (byte budget { $budget })
cmd-gc-diff-render-cache = diff_render_cache: { $entries }/{ $capacity } entries, ~{ $bytes } (byte budget { $budget })
cmd-gc-allocator-heading = ── { $allocator } breakdown ──
cmd-gc-allocated = allocated: { $bytes } (in-use allocations tracked by the allocator; native libraries may not be covered)
cmd-gc-active-mimalloc = active: { $bytes } (mimalloc page_committed = historical touched high-water mark, not current usage; do not use for diagnosis)
cmd-gc-active = active: { $bytes } (active pages)
cmd-gc-resident-mimalloc = resident: { $bytes } (whole-process RSS/WorkingSet, not memory exclusive to mimalloc)
cmd-gc-resident = resident: { $bytes } (estimated physical memory resident in the allocator)
cmd-gc-metadata = metadata: { $bytes } (allocator metadata)
cmd-gc-mapped-mimalloc = mapped: { $bytes } (reserved virtual address space, not physical resident memory)
cmd-gc-retained-mimalloc = retained: { $bytes } (reserved-committed: reserved but uncommitted memory, not physical resident memory)
cmd-gc-mapped = mapped: { $bytes } (mapped memory)
cmd-gc-retained = retained: { $bytes } (retained memory not returned to the OS)
cmd-gc-fragmentation-mimalloc = Fragmentation: active-allocated={ $fragmentation } (⚠ based on the mimalloc touched high-water mark, not actual fragmentation; ignore)
cmd-gc-fragmentation = Fragmentation: active-allocated={ $fragmentation } | resident-active={ $waste }
cmd-gc-os-gap = OS RSS({ $rss }) - { $allocator } resident({ $resident }) = { $gap }
cmd-gc-full-stats-heading = ── { $allocator } full statistics (see logs) ──
cmd-gc-known-estimate = Estimated known total: { $known } (messages { $messages } + VMs { $views } + Markdown cache { $markdown } + render cache { $render } + diff caches { $diff }) | estimated unidentified memory within allocated: { $gap }
cmd-gc-allocated-rss-hint-positive = allocated exceeds the RSS sampled at that time (sampling time/accounting differences; for reference only)
cmd-gc-allocated-rss-hint-negative = The RSS difference may include allocator idle pages, page rounding, native library allocations, stacks, mapped files, etc.; the difference alone does not identify the cause
cmd-gc-rss-unavailable = RSS: unavailable (failed to read allocator statistics)
cmd-gc-structures-heading = ── Data structure diagnostics ──
cmd-gc-message-counts-equal = Equal counts (contents not compared; origin is the authoritative agent history, completed is the rendering pipeline baseline)
cmd-gc-message-counts-different = Different counts (contents not compared)
cmd-gc-caches-heading = ── Rendering caches ──
cmd-gc-cache-estimate-note = { "  " }Basis: estimated from container/owned string capacity; excludes LRU/allocator overhead and is not RSS.
cmd-gc-metadata-mimalloc = metadata: unavailable (committed and page_committed use different accounting bases; their difference cannot estimate metadata)
cmd-gc-estimate-note = Note: estimates still exclude syntax highlighting (SyntaxSet/ThemeSet), compiled regular expressions, ACP buffers, tokio, tracing, allocator metadata, etc.; the remaining memory needs investigation and cannot establish whether a leak exists.
cmd-gc-collection-scope = Collection scope: the current thread collects immediately; TUI worker threads respond when next idle and may not have completed collection before this sample. Application caches are not cleared and live objects are not freed.

# CLI
cli-command-cc-code = CC Code AI Agent
cli-command-acp = Run as an ACP agent over stdin/stdout JSON-RPC
cli-command-update = Download and install the latest GitHub release
cli-command-sync = Sync settings, skills, MCP and plugins between devices
cli-command-plugin = Manage plugins
cli-command-sender = Send local configuration to another device
cli-command-receiver = Receive configuration from another device
cli-command-list = List installed plugins
cli-command-install = Install a plugin
cli-command-uninstall = Uninstall a plugin
cli-help-yolo = Bypass approval (disabled by default)
cli-help-approve = Enable approval (same as --permission-mode auto)
cli-help-print = Print the response and exit
cli-help-output-format = Output format: text, json or stream-json (requires -p)
cli-help-max-turns = Maximum agentic turns (requires -p)
cli-help-bare = Skip hooks, LSP, plugins and MCP initialization (requires -p)
cli-help-permission-mode = Permission mode: auto or bypass
cli-help-skip-permissions = Bypass all permission checks
cli-help-model = Model alias (opus, sonnet, haiku, fable) or full name
cli-help-effort = Reasoning effort: low, medium, high or max
cli-help-cont = Continue the latest conversation in this directory
cli-help-resume = Resume a conversation by session ID
cli-help-session-id = Session ID (valid UUID required)
cli-help-session-name = Session display name
cli-help-no-session-persistence = Disable session persistence (requires -p)
cli-help-allowed-tools = Allowed tools, for example Bash(git:*) and Edit
cli-help-disallowed-tools = Disallowed tools
cli-help-settings = Additional settings file or JSON string
cli-help-cwd = Working directory
cli-help-agent = Agent type from .claude/agents/
cli-help-server = Relay server URL
cli-help-json = Output JSON
cli-help-plugin = Plugin name or ID (name@marketplace)
cli-help-scope = Plugin scope: user, project or local
cli-help-uninstall-scope = Uninstall scope (omit to remove from all scopes)
cli-settings-read-warning = Warning: cannot read settings file '{ $path }': { $error }
cli-settings-json-warning = Warning: --settings is not valid JSON
cli-output-format-error = Unknown output format: '{ $value }' (expected text, json or stream-json)
cli-plugin-scope-error = Unknown plugin scope: '{ $value }' (expected user, project or local)
cli-update-success = Updated to { $tag }
cli-update-failed = Update failed: { $error }
cli-no-session-id = No session ID available
cli-error = Error: { $error }
cli-empty-prompt = No prompt provided. Use cc-code -p "your question" or echo "question" | cc-code -p
cli-settings-invalid = --settings is not a valid file path or JSON: { $error }
cli-no-provider = No LLM provider configured. Set ANTHROPIC_API_KEY or OPENAI_API_KEY
cli-plugin-empty = No plugins installed.
cli-plugin-version = Version
cli-plugin-marketplace = Marketplace
cli-plugin-status = Status
cli-plugin-enabled = Enabled
cli-plugin-disabled = Disabled
cli-plugin-install-failed = Installation failed: { $error }
cli-plugin-installed = Installed: { $id } v{ $version } (scope: { $scope })
cli-plugin-uninstall-failed = Uninstall failed: { $error }
cli-plugin-uninstalled = Uninstalled: { $id }

# File mention popup
at-mention-searching = Searching…
at-mention-empty-directory = No files here. Type to search.
at-mention-no-matches = No matching files.
command-agent-center-description = Open the cross-project agent command center
agent-center-loading = (Loading…)
agent-center-none = (none)
agent-center-status-note = Other processes: saved status only
agent-center-empty = No matching loaded sessions. Use Show more to load older sessions.
agent-center-show-more = Show more
agent-center-search = Search titles in loaded sessions; / or Esc clears
agent-center-rename = Rename:
agent-center-busy = Cannot switch or delete while an agent, shell or permission request is active, or the project is unavailable.
agent-center-other-projects = Other projects: { $count } sessions · g to group · counts include all sessions
agent-center-delete-question = Permanently delete this session and its messages?
agent-center-delete-confirm = y Delete permanently (cannot be undone) · Esc Cancel
agent-center-open-failed = Could not open the session. Check the project directory and session data; Esc returns to the detailed error.
agent-center-help = Navigate
    ↑/↓ move · Home/End first/last · Enter open / load more
    Esc close overlay, rename, search, then center
    Tasks
    n new session in selected project · o current-project history
    r rename · Backspace delete (confirmation required)
    View
    Tab/Shift+Tab filter · g Project/Status/Model · / title search
    Search and filters apply to loaded pages; counts cover all projects.
    Working/Needs you are live only in this process. Other processes use saved status.
    ? or Esc close help

# ── i18n wiring fix: commands ──
streaming-current = Current render mode: { $mode } (options: streaming / block / none)
streaming-usage = Usage: /streaming [streaming|block|none]
streaming-switched = Render mode switched to: { $mode }

# ── i18n wiring fix: panels & popups ──
agent-panel-title = Agent Selection
agent-panel-title-empty = Agent Selection (none)
agent-panel-none = ○ No Agent (default)
agent-panel-empty-hint = Add Agent definition files under .claude/agents/
cron-panel-title = Scheduled Tasks
cron-status-enabled = ✓enabled
cron-status-disabled = ✗disabled
cron-empty = (no scheduled tasks; create one with /loop)
memory-panel-title = Memory files
memory-panel-create-hint = Press Enter to create and edit
key-rewind-file = :Switch rewind file
rewind-mode-messages = 1. Rewind to this prompt
rewind-mode-messages-files = 2. Rewind to this prompt + restore files
rewind-mode-confirm = ⚠ Confirm: restore files?
rewind-files-to-restore = Files to restore:
rewind-action-write = Write → delete + git restore
rewind-action-edit = Edit → restore
rewind-key-hint = Enter confirm, Esc cancel
memory-label-project = Project instructions
memory-label-global = User global
oauth-popup-title = OAuth Authorization — { $server }
oauth-popup-instruction = Press Ctrl+O to open the link in a browser, then paste the callback URL:
oauth-popup-input-prompt = Callback URL >
oauth-callback-parse-failed = Cannot parse callback URL: { $error }

# ── i18n wiring fix: plugin / subagent / background bar ──
plugin-parse-failed = Parse failed: { $error }
plugin-marketplace-exists = Marketplace already exists
plugin-marketplace-fetched = Marketplace '{ $name }' content fetched
plugin-content-fetch-failed = Failed to fetch content: { $error }
plugin-marketplace-removed = Marketplace removed: { $name }
subagent-approval-pending = Approval pending: { $tool }
subagent-awaiting-answer = Waiting for user input
bg-agent-approval-hint = y:approve / n:reject / Enter:details
bg-agent-question-hint = Enter:view question
bg-agent-approval-request = [Approval request] Subagent "{ $agent }": { $action } [{ $hint }]
bg-agent-nav-hint = ↑/↓ · Enter to view · x to stop
bg-agent-bar-expand = [{ $agents }] (↓ expand)
compact-context-summarized = 📋 Context summarized
bg-moved-to-background = Moved to background (task { $task })
bg-output-file = Output file: { $path }

# ── i18n wiring fix: misc ──
key-any-other = Other key
todo-count = { $count } tasks
agent-center-read-ended = Session read task ended; reopen the center
subagent-tool-uses = { $count } tool uses
bg-drawer-hint-short = ↑/↓ to select · ctrl+x ctrl+k to stop all agents
bg-drawer-hint-full = Enter to view · x to stop · ctrl+x ctrl+k to stop all agents
