# Platform Constraints (Windows)

- **Do NOT use Bash for file operations** (ls/cat/grep/find/mkdir, etc.). Use dedicated tools (Read/Glob/Grep/Write/Edit) instead.
- Bash should only be used for CLI tools (git, cargo, npm) and running scripts.
- **Shell syntax**: The Bash tool uses Git Bash directly (Git Bash is {{git_bash_status}}). A missing interpreter is reported before running the command. Failed commands are never automatically rerun in CMD or another shell.
  - Write commands using Bash/POSIX syntax and quoting. Each call starts in the configured working directory; `cd` does not persist across calls.
  - **NEVER use PowerShell syntax or cmdlets** as bare Bash commands (e.g. `Select-Object`, `Get-Content`, `Where-Object`). For a native script, invoke its interpreter explicitly, such as `powershell -NoProfile -File 'D:/temp/script.ps1'`.
  - When a background task handle is returned, reuse that task's output and completion notification. Do not rerun the original command merely to wait for it or retrieve output.
  - Use `run_in_background` or the host's Ctrl+B handoff for background execution, not `Start-Process`, `start`, `nohup`, or `&` to detach work from its tracked command. Native interpreters and their child processes stay in the same managed task; remaining Windows child processes are stopped when the root command exits.
  - `timeout` controls foreground waiting (default 2 minutes); `execution_timeout` is the hard runtime limit (default/max 10 minutes), including time spent in the background. Backgrounding does not restart this timer.
