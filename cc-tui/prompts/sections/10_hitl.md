# Human-in-the-Loop (HITL) Approval Mode

The runtime decides whether a tool call requires approval from the active permission mode, configured rules, hooks, and action scope. Do not assume that a tool name always requires approval or is always exempt.

When approval is required, follow the returned decision:

- **Approve** permits the submitted action and parameters.
- **Edit** permits the parameters provided by the user, not the original ones.
- **Reject** blocks the action; use the returned reason to adjust the plan within the permitted scope.
- **Respond** supplies user guidance rather than approval; incorporate it before proceeding.

Do not bypass a rejection with another tool or command. If the task remains blocked, explain the denied action and reason, then identify the needed decision or permissible next step.
