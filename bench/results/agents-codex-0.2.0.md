# Agent A/B with Codex CLI (lancet 0.2.0)

2026-11-28. `bench/agents/run_codex.sh`, tasks in `bench/agents/tasks.txt` (repository at `a043e7f`), Codex CLI 0.151.0, models `gpt-5.6-sol` and `gpt-5.6-luna`, two runs each per side. Each run had an empty HOME and its own CODEX_HOME, so neither side saw the user's skills, MCP servers or global instructions; the lancet side used the 0.2.0 binary, the built-in side couldn't run `lancet`.

| | Correct (graded tasks) | Wall time | Tool calls | Input tokens |
|---|---|---|---|---|
| Built-in (shell, `rg`, `apply_patch`; 2.6 KB guide) | 27/28 | 1,058 s | 121 | 3.05 M |
| lancet (the skill, 3.6 KB) | **28/28** | 1,060 s | 125 | 3.06 M |

Per task (two runs per model, summed):

| Task | Model | Built-in: correct · s · calls | lancet: correct · s · calls |
|---|---|---|---|
| e1 message | luna | 2/2 · 33 · 6 | 2/2 · 35 · 5 |
| e1 message | sol | 2/2 · 41 · 7 | 2/2 · 27 · 3 |
| e2 rename across 6 files | luna | 2/2 · 73 · 8 | 2/2 · 81 · 11 |
| e2 rename across 6 files | sol | 2/2 · 135 · 7 | 2/2 · 169 · 11 |
| e3 local rename | luna | 2/2 · 35 · 6 | 2/2 · 28 · 4 |
| e3 local rename | sol | 2/2 · 36 · 7 | 2/2 · 32 · 6 |
| e4 field rename (same-named fields elsewhere) | luna | 2/2 · 72 · 10 | 2/2 · 73 · 10 |
| e4 field rename (same-named fields elsewhere) | sol | 2/2 · 63 · 8 | 2/2 · 84 · 8 |
| e5 new document | luna | 2/2 · 82 · 9 | 2/2 · 66 · 7 |
| e5 new document | sol | 2/2 · 68 · 8 | 2/2 · 58 · 8 |
| q1 trace a value (not graded) | luna | – · 131 · 16 | – · 166 · 30 |
| q1 trace a value (not graded) | sol | – · 120 · 12 | – · 108 · 11 |
| q2 call sites among same-named things | luna | 1/2 · 71 · 7 | 2/2 · 61 · 5 |
| q2 call sites among same-named things | sol | 2/2 · 60 · 6 | 2/2 · 34 · 2 |
| q3 read a PDF | luna | 2/2 · 19 · 2 | 2/2 · 18 · 2 |
| q3 read a PDF | sol | 2/2 · 19 · 2 | 2/2 · 20 · 2 |

## Reading

- With strong models that already handle `rg` and `apply_patch` well, the two sides are even on time, calls and tokens. The earlier OpenCode run with big-pickle (lancet 37% faster, 58% fewer calls) does not carry over to these models.
- lancet's clearest gain is q2, where same-named methods and locals are a trap: `lancet refs` answered in one call (sol: 34 s vs 60 s), and the built-in side missed a call site once.
- Its clearest loss is e2: the agent first tried `lancet rename` (locals only) on a function used in six files, was refused twice, then did it with `lancet edit --edits`. The skill now says how to rename across files.
- One run per task and model pair is still noisy; read differences under ~20% as noise.
