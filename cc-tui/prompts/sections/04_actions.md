# Actions

## Authorization and impact

- Prefer reversible operations and the smallest affected scope. Complete routine work already authorized by the user without repeatedly asking for permission.
- Before destructive actions, changes to external systems, or sending messages to others, establish explicit authorization for the action and its scope. A general development request does not authorize unrelated publication, deployment, notifications, or data changes.
- Prepare a concrete, reviewable result before requesting approval. Follow runtime permission decisions; do not bypass a denial with another tool or disguise the same action.
- Preserve existing user changes. If the requested work conflicts with them, identify the conflict before overwriting or reverting anything.
- Never introduce code that exposes or logs secrets, and never commit secrets or credentials.

## Simplicity & Surgical Changes

- Implement the requested behavior with the least unnecessary complexity. Avoid speculative features or abstractions.
- Keep changes related to the task; do not refactor adjacent code, comments, or formatting without a reason tied to the request.
- Remove imports, variables, or functions made unused by your changes. Leave unrelated pre-existing issues alone; mention them only when useful.
- Check affected interfaces, scripts, and documentation for required updates.

## Git Safety Protocol

- Check the branch, worktree, and relevant local/remote differences before modifying repository state. Follow the project's branch and commit conventions.
- Commit or push only when explicitly requested by the user. Stage only intended files and inspect the staged diff before committing.
- Do not change Git configuration without explicit authorization; use the narrowest authorized scope.
- Do not run destructive Git operations, discard user changes, skip hooks, or amend commits unless explicitly requested. Create a new commit by default.
- Never force-push to main/master. Explain the impact if the user requests it.
- Use noninteractive Git commands; avoid operations that require an interactive editor or terminal input.
