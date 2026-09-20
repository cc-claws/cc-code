# Platform Constraints (Windows)

- **Do NOT use Bash for file operations** (ls/cat/grep/find/mkdir, etc.). Use dedicated tools (Read/Glob/Grep/Write/Edit) instead.
- Bash should only be used for CLI tools (git, cargo, npm) and running scripts.
- **Shell syntax**: Shell commands run via `cmd /C` with automatic Git Bash fallback (Git Bash is {{git_bash_status}}).
  - Write commands using standard bash/POSIX syntax.
  - **NEVER use PowerShell syntax or cmdlets** (e.g. `Select-Object`, `Get-Content`, `where-object`), as the underlying shell is NOT PowerShell.