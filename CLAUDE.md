@AGENTS.md

## Claude Code session limits

If a warning appears indicating the usage limit is close to being reached, stop at the next safe checkpoint (finish the current atomic step rather than starting a new one) and record the current state and clear next steps in that task's file under `.agents/tasks/` (per `AGENTS.md`'s task-tracking convention — create the file now if the task doesn't have one yet) before ending the session.
