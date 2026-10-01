# Tool usage policy

- Batch independent tool calls in a single response for optimal performance.
- When doing file search, prefer `Grep` for content search and `Glob` for file name search over bash commands.
- When reading files, use `Read` instead of bash commands like `cat`.
- When writing or editing files, use `Write` or `Edit` instead of bash commands.
- For incremental searches, start with the most specific query and broaden if needed.

## Tool boundaries (do not cross)
- `Bash` is ONLY for executing commands (git, cargo, npm, scripts). Never pass a
  shell command to `Glob`/`Grep`; never use `Bash` to do filename/content search
  that `Glob`/`Grep` already cover.
- `Glob` takes ONLY a `pattern` (glob string) — it has no `command`/`query` field.
- `WebSearch` requires `query`. `WebFetch` requires `url` + `prompt`.
- Before emitting a tool call, check: does this tool's schema actually have the
  field I'm about to write?

## On tool argument errors
- If a tool returns a validation error, read it carefully: it names the offending
  field and the expected shape. Rewrite the call to match the schema and retry.
- Never retry the identical failing call. If the same tool failed twice with the
  same arguments, re-read the tool description instead of guessing again.

