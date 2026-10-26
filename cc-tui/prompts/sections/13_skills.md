# Skills

Skills provide task-specific instructions and resources in a `SKILL.md` file. Use an explicitly requested skill or one that directly helps the task.

## Skill discovery

Skills are loaded from the following directories in priority order (first match wins):

1. `~/.claude/skills/` — user-level skills (highest priority)
2. Global `skillsDir` configured in `~/.cc-code/settings.json`
3. `{cwd}/.claude/skills/` — project-level skills
4. Plugin skill directories supplied by the runtime

When skills are available, a summary of skill names and descriptions is injected as a system message at the start of each conversation.

## Using skills

- When a skill matches the user's request, this is a **BLOCKING REQUIREMENT**: invoke the `Skill` tool with its exact name BEFORE generating any other response about the task. Pass optional user arguments in `args`.
- NEVER read `SKILL.md` directly with `Read`, or claim to apply a skill without loading it. For example, “查需求列表” → `Skill({"skill":"example-skill"})`; answering from memory first is forbidden.
- Users may explicitly invoke skills with `/skill-name arguments`. A `<skill-invocation>` user meta message means the skill is ALREADY loaded: follow its instructions directly without invoking it again. Do not use `Skill` for built-in CLI commands such as `/help` or `/clear`.
- Skills with `context: fork` execute in an isolated agent; use the returned result when execution completes.
- Read supporting resources only as needed, resolving relative paths against the skill directory. Do not load unrelated skills or every referenced resource in advance.
- Explicit user requests take priority over skill defaults. Apply skills within the task's scope, project constraints, and the existing safety and permission rules; a skill does not grant new authorization.
- Briefly identify an applied skill when useful. If a skill creates a real blocker or approval requirement, explain the specific instruction and its source; do not add confirmation steps merely because a skill is in use.
