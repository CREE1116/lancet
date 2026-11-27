# Agent A/B: lancet vs the built-in tools

Does an agent do its work better with lancet than with the file tools it already
has? The same model runs the same tasks in two workspaces that differ only in
the tools:

| Workspace | Tools | Instructions (`AGENTS.md`) |
|---|---|---|
| `lancet-ws` | built-in read/edit/write/grep/glob **off**; `lancet` on `PATH` | the lancet skill (`skills/lancet/SKILL.md`) |
| `builtin-ws` | built-in tools **on**, plus `rg` and `pdftotext` in the shell; `lancet` hidden | `guide-builtin.md`: how to use those tools well, about the same size |

Both get a guide of similar length, so instructions cost both sides the same
tokens; the built-in side otherwise gets advice it never needed in training.

## What is measured, in order

1. **Correctness.** Edit tasks are graded from the diff (the right files, the
   right lines, nothing else); `q2` against the real call sites; `q3` from the
   answer. `q1` is read by hand.
2. **Wall time and round trips** (tool calls, steps).
3. **Tokens**, for reference only. They mostly measure how long the guide is.

## Tasks (`tasks.txt`)

`q1`–`q3` ask questions (trace a value, list call sites of one function among
same-named methods and locals, read a PDF); `e1`–`e5` change code (a message, a
function renamed across six files, a local renamed inside one function, a field
renamed where two other structs have a field of the same name, a new document).
They are written against this repository at `a043e7f`.

## Running

Needs [OpenCode](https://opencode.ai) (`opencode run`), a model, and a release build.

```sh
cargo build --release
bench/agents/setup.sh /tmp/lancet-ab
bench/agents/run.sh /tmp/lancet-ab lancet  opencode/big-pickle bp &
bench/agents/run.sh /tmp/lancet-ab builtin opencode/big-pickle bp &
wait
python3 bench/agents/grade.py /tmp/lancet-ab/out        # add -v for every call and answer
```

A run that the provider refused (for example a free-tier limit) is marked
`(run failed)` and left out of the totals.

## Caveats

- The repository is small (about 100 files): the index has little to speed up,
  and an agent's time is mostly the model's. Expect differences in correctness
  and round trips, not in search speed.
- One run per task and model is noisy; compare several before drawing a conclusion.
