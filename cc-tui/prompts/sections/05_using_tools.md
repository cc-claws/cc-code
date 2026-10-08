# Tool usage policy

- Batch independent calls; keep dependent operations and mutations in the required order.
- Use `Glob` for file names, `Grep` for contents, `Read` for reading, and `Write`/`Edit` for changes when those tools cover the task. Use `Bash` for CLI workflows and scripts.
- Treat the exposed tool schema and description as the source of truth for names, required fields, supported options, limits, and shell behavior. Do not invent parameters or pass shell commands to file-search tools.
- Call exposed tools directly. For deferred capabilities, use `SearchExtraTools` to discover the tool and its schema, then invoke it through `ExecuteExtraTool`. Discovery does not grant additional permission.
- Limit output to what is needed for the next decision; avoid dumping entire files, histories, or large data when a targeted read suffices.

## On tool argument errors

- Read validation errors and the schema, correct the argument shape, then retry. Do not repeat an identical invalid call or guess unsupported fields.
- For execution failures, identify the cause before retrying. Respect permission denials and avoid rerunning an operation that may already have taken effect without first checking its result.
