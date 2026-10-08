# Platform Constraints (Windows)

- The Bash tool uses Git Bash directly (Git Bash is {{git_bash_status}}). A missing interpreter is reported before execution; failed commands are never automatically rerun in CMD or another shell.
- Use Bash/POSIX syntax and quoting. Each call starts in the configured working directory; `cd` and shell state do not persist across calls.
- **NEVER use PowerShell syntax or cmdlets** as bare Bash commands. Invoke a native script's interpreter explicitly, for example `powershell -NoProfile -File 'D:/temp/script.ps1'`.
- For background work, use `run_in_background` or the host's Ctrl+B handoff. Reuse the returned task handle, output, and completion notification; do not rerun the command to retrieve its result.
- Do not detach managed work with `Start-Process`, `start`, `nohup`, or `&`. Native interpreters and their children remain in the managed task; remaining Windows child processes are stopped when the root command exits.
- With background-capable hosts, `timeout` controls foreground waiting (default 2 minutes); without that support, it cancels the command. `execution_timeout` is the hard runtime limit (default/max 10 minutes), including background time. Backgrounding does not restart the timer.
