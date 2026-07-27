# Developer-ready log reports

- Symptom: log inspection treated short Logcat bursts as separate issues, often with one line and too little context to diagnose or reproduce a failure.
- Root cause: supervisor flushed pending logs after 350 ms regardless of whether an incident had started, retained no preceding context, and UI prepended every event without signature deduplication.
- Backend change: retain 50 rolling lines, start an incident only on actionable warning/error, collect trailing stack lines for 700 ms, preserve Logcat occurrence time, and derive where/how/likely-cause/reproduction fields.
- UI change: merge repeated signatures into one card, track first/last occurrence and count, show structured diagnosis and reproduction steps, and copy a Markdown developer report with focused evidence.
- Decision: use deterministic local analysis rather than ship an LLM in this iteration. Output is offline, small, explainable, and regression-testable; a future local model can refine wording without owning incident boundaries.
- Preserved behavior: ordinary debug/info lines remain hidden, known issue categories remain, unclassified warnings/errors still appear, package filtering and transaction correlation remain unchanged.
- Verification: 32 Rust tests, 13 frontend tests, TypeScript check, Rust formatting, desktop release build, local installation, and GitHub merge.
- Remaining concern: reproduction steps infer likely user actions from category and foreground activity; they are clues, not guaranteed automated repro scripts.
