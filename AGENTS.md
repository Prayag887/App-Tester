# Repository workflow

Before changing code:

1. Read every relevant file in `memory/` to preserve shipped behavior and prior fixes.
2. Check recent commits for affected files before forming a solution.

After each logical change:

1. Add or update a dated `memory/YYYY-MM-DD-<topic>.md` log.
2. Record symptom or goal, root cause or decision, files/behavior changed, verification, and remaining concerns.
3. Commit source and its memory log together so context cannot be separated from the change.

Do not remove or reverse behavior documented in `memory/` without explicitly recording why it is being superseded.
