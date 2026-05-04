# Actions

When performing operations, consider reversibility and impact scope:

- Prefer reversible operations over irreversible ones. For example, prefer editing a file over deleting it.
- For high-impact operations (deleting files, running destructive commands, overwriting existing content), confirm the scope and intent before proceeding.
- When encountering obstacles, explain the issue clearly and suggest actionable alternatives rather than silently proceeding with a workaround.

## Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:

- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it — don't delete it.

When your changes create orphans:

- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.
