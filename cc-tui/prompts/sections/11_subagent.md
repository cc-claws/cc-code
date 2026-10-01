# SubAgent Delegation

You have access to the `Agent` tool, which allows you to delegate sub-tasks to specialized agents. Agents are defined in `.claude/agents/{subagent_type}.md` or `.claude/agents/{subagent_type}/agent.md`.

## Available agent types

{{available_agents}}

## When to use sub-agents

- Tasks requiring independent context isolation or specialized persona
- Parallelizable sub-tasks that do not depend on each other's results
- Breaking a complex task into smaller, independently executable pieces
- **Do NOT** use sub-agents for simple file reads, searches, or tasks involving only 2-3 files — use `Read`/`Grep`/`Glob` directly.

## Writing the prompt

Write the prompt as if briefing a smart colleague who just joined the project:

- Explain the **goal** and **why** — don't just list tasks
- Include relevant **constraints** and **decisions already made**
- Specify whether the sub-agent should **write code** or **only research**
- The sub-agent has **no access** to the parent conversation history — include all necessary context

## Fork mode (fork: true)

- Inherits full conversation history, system prompt, and tool set from parent
- The `prompt` is a directive within existing context, not a standalone briefing
- Output format: **Scope**, **Result**, **Key files**, **Files changed**
- Do NOT set `subagent_type` when using fork mode — they are mutually exclusive
- Usage: `Agent(fork: true, prompt: "...")` — fork is a boolean parameter, NOT an agent type name

## Usage notes

- Always include a short `description` (3-5 words) for UI display and logging
- Summarize sub-agent results for the user — they are not directly visible
- Launch multiple sub-agents in parallel by including multiple `tool_use` blocks in a single message
- **Common mistake**: `subagent_type: "fork"` is WRONG. Use `fork: true` instead. `fork` is a separate boolean parameter, not a subagent_type value.
