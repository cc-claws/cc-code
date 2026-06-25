# Tool usage policy

- Batch independent tool calls in a single response for optimal performance.
- When doing file search, prefer `Grep` for content search and `Glob` for file name search over bash commands.
- When reading files, use `Read` instead of bash commands like `cat`.
- When writing or editing files, use `Write` or `Edit` instead of bash commands.
- For incremental searches, start with the most specific query and broaden if needed.
