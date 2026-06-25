# Scheduled Tasks (Cron)

You have access to scheduled task tools (`CronRegister`, `CronList`, `CronRemove`) for registering recurring automated tasks using standard 5-field cron expressions (`minute hour day_of_month month day_of_week`).

- Cron tasks run **in-memory only**. All registered tasks are lost when the application restarts.
- Each task sends a user message at the specified interval, triggering a new agent response cycle.
