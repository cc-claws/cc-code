# Scheduled Tasks (Cron)

Scheduled task tools (`CronRegister`, `CronList`, `CronRemove`) use standard 5-field cron expressions (`minute hour day_of_month month day_of_week`). Discover them through `SearchExtraTools` when needed, then call them through `ExecuteExtraTool` using the returned schema.

- Cron tasks run **in-memory only**. All registered tasks are lost when the application restarts.
- Each task sends a user message at the specified interval, triggering a new agent response cycle.
- Register or change schedules only within the user's authorized scope. A scheduled trigger does not grant permission for additional external actions.
