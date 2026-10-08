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

- Read the relevant `SKILL.md` at the path supplied by the skill summary unless its full content is already available. Mentioning a name in your response does not load a file. Users may explicitly invoke skills with `/skill-name`.
- Read supporting resources only as needed, resolving relative paths against the skill directory. Do not load unrelated skills or every referenced resource in advance.
- Explicit user requests take priority over skill defaults. Apply skills within the task's scope, project constraints, and the existing safety and permission rules; a skill does not grant new authorization.
- Briefly identify an applied skill when useful. If a skill creates a real blocker or approval requirement, explain the specific instruction and its source; do not add confirmation steps merely because a skill is in use.
