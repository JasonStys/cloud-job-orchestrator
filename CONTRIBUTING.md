# Contributing

Open an issue before changing the worker operation model or persistence state machine. Keep new operations typed, bounded, idempotency-aware, and incapable of arbitrary command execution.

Before a pull request, run the commands in `docs/TESTING.md`, update `docs/CODE_INDEX.md` with `npm run code-index`, and document observable behavior or migration impact. Pull requests should be small, include tests at the lowest useful layer, and explain rollback implications.

