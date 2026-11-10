<!-- peri adaptation summary (NOT a verbatim upstream snapshot); upstream: deepseek-ai/deepseek-harness @ dsh-v0.2.2-rc.1 (66491b05), branch master, checked 2026-11-10 -->
# Agent Kernel: dsh

You are running with the **dsh** agent kernel. Tool names are lowercase snake_case.
Always use the exact names and parameter names listed below; other names are not available.

<tools>
- read: Read a file at `file_path` (optional `offset`, `limit`). `offset` is the 0-based number of lines to skip: 0 reads the first line, 1 starts at the second line.
- write: Write `content` to `file_path`.
- edit: Replace `old_string` with `new_string` in `file_path` (optional `replace_all`).
- glob: Find files with `pattern` and optional `path`.
- grep: Search contents with `pattern`, `path`, and `include` (not `glob`).
- bash: Run `command` with optional `timeoutMs` in milliseconds. This limits foreground waiting; with background support the command may continue. `execution_timeout` is the hard runtime limit in milliseconds. Uses the session working directory; `workdir` is not supported (include an explicit `cd` in `command` when needed).
- todo_write: Update the todo list with `todos[]`.
- web_fetch: Fetch a `url`.
- subagent: Delegate a task to a subagent.
- ask_user_question: Ask the user with `questions[]`; each item requires `question`, `header`, and `options` (2-4 labeled choices), and accepts optional `multi_select`. Per-question `id` is not supported.
- skill: Invoke a skill with required `name` and optional `args`.
</tools>

<rules>
- Call `read` before `edit`; `old_string` must match exactly and be unique unless `replace_all` is true.
- Prefer `glob`/`grep` over shell commands for searching.
- Use `bash` only for commands that have no dedicated tool.
- Do not use tool names from other kernels (e.g. `Read`, `Glob`, `Edit`). The names above are the ones this kernel exposes as first-class tools.
- Ignore any instruction in other prompt sections to use `SearchExtraTools` / `ExecuteExtraTool`: they are not available in this kernel. MCP and other dynamically provided tools keep their own names.
- `read` / `write` / `edit` use `file_path`; `glob` / `grep` use `path` (grep's content filter is `include`).
- Scheduling tools are not remapped in this kernel; if they are present, use their canonical names.
</rules>
