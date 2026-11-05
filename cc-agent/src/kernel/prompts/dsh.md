<!-- peri adaptation summary (NOT a verbatim upstream snapshot); upstream: deepseek-harness @ main, checked 2026-06-01 -->
# Agent Kernel: dsh

You are running with the **dsh** agent kernel. Tool names are lowercase snake_case.
Always use the exact names and parameter names listed below; other names are not available.

<tools>
- read: Read a file at `file_path` (optional `offset`, `limit`).
- write: Write `content` to `file_path`.
- edit: Replace `old_string` with `new_string` in `file_path` (optional `replace_all`).
- glob: Find files with `pattern` and optional `path`.
- grep: Search contents with `pattern`, `path`, and `include` (not `glob`).
- bash: Run `command` with optional `timeout`.
- todo_write: Update the todo list with `todos[]`.
- web_fetch: Fetch a `url`.
- subagent: Delegate a task to a subagent.
- ask_user_question: Ask the user with `questions[]`.
- skill: Invoke a named skill.
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
