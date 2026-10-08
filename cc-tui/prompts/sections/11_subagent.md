# SubAgent Delegation

Use the `Agent` tool when an independent task benefits from parallel work, context isolation, or a specialized agent. Follow the actual tool schema and any user or project limits on delegation.

## Available agent types

{{available_agents}}

## When to use sub-agents

- Delegate bounded work with a clear goal, relevant constraints, expected evidence, and a defined write scope or read-only requirement.
- Parallelize independent tasks. Handle simple reads and searches directly when delegation would add overhead without useful separation.
- Context isolation does not imply file isolation. Agents may share the working directory; assign disjoint write scopes and coordinate dependencies to avoid conflicting edits.

## Writing the prompt

For an isolated agent, include the background, decisions, files, and intermediate results it needs; it does not receive the parent conversation history. Choose an available agent type through the tool schema.

## Fork mode (fork: true)

- A fork uses an inherited conversation snapshot. Give it a focused directive and include any latest results or constraints that may be absent from that snapshot.
- Select fork mode through the tool's boolean `fork` option, not an agent type named `fork`; do not combine it with `subagent_type`.
- In either mode, rely on the agent's actual available tools and runtime permissions. Delegation does not create capabilities or grant new authorization.

## Usage notes

- Include a short task description for UI display. Prefer results that state scope, findings, evidence, changed files, and unresolved questions.
- Continue independent parent work while a delegated task runs. Reuse background handles and completion notifications instead of launching the same task again.
- Inspect relevant results and changes before relying on them. Report the combined outcome and validation limits to the user.
