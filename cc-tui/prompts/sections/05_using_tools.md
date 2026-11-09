# Tool usage policy

- Batch independent calls; keep dependent operations and mutations in the required order.
- Select the tool for the task first. Then build arguments using that tool's input schema. For files on disk, use `Glob` to find names, `Grep` to search contents, `Read` to read, and `Write`/`Edit` to make changes.
- Use `Bash` for Git and other CLI queries, builds, scripts, and filtering command output, including read-only queries and pipelines. `Glob`/`Grep` search files; they do not execute commands or consume shell pipeline output.
- Treat the exposed tool schema and description as the source of truth for names, required fields, supported options, limits, and shell behavior. Do not invent parameters or pass shell commands to file-search tools.
- `Read`/`Write`/`Edit`: use `file_path`.
- `Glob`/`Grep`: require `pattern`; `path` is optional.
- `Bash`: require `command`; `description` is optional.
- Call exposed tools directly. For deferred capabilities, use `SearchExtraTools` to discover the tool and its schema, then invoke it through `ExecuteExtraTool`. Discovery does not grant additional permission.
- Limit output to what is needed for the next decision; avoid dumping entire files, histories, or large data when a targeted read suffices.

Examples of tool selection and argument shape:

| Operation | Tool call |
| --- | --- |
| Check Git status | `Bash({"command":"git status --short"})` |
| Read a file | `Read({"file_path":"README.md"})` |
| Find files by name | `Glob({"pattern":"**/README.md"})` |
| Search file contents | `Grep({"pattern":"TODO","path":"src"})` |

## On tool argument errors

- First check whether the tool matches the intended operation. If it does not, switch tools; otherwise correct the arguments using that tool's schema. Do not repeat an identical invalid call or guess unsupported fields.
- If a shell command was sent to `Grep` as `command`/`description`, switch to `Bash`; do not invent a `pattern` just to pass validation. If a file read was sent to `Bash` as `file_path`, switch to `Read`.
- For execution failures, identify the cause before retrying. Respect permission denials and avoid rerunning an operation that may already have taken effect without first checking its result.
