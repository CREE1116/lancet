# Lancet Documentation

**Official Documentation Website:** [English Docs](https://cree1116.github.io/lancet/docs/) · [한국어 공식 문서](https://cree1116.github.io/lancet/ko/docs/)

This directory contains the source Markdown guides and SVG diagrams for **Lancet**. For the full structured documentation with interactive search, side-by-side migration tables, recipes, JSON schemas, and troubleshooting, visit the live documentation site above.

## Guides & References

| Document | Topic |
|---|---|
| [`grammar.md`](./grammar.md) | Lancet Query Language (LQL v0.1) formal grammar, semantics, and full CLI surface |
| [`rg-parity.md`](./rg-parity.md) | `lancet rg` flag compatibility matrix, intentional differences, and `--docs` / `--passthru` / `--type-add` |
| [`mcp.md`](./mcp.md) | Built-in `lancet mcp` stdio server setup, 11 tools, client configs, and skill installation |
| [`agent-instructions.md`](./agent-instructions.md) | Drop-in agent workflow rules (`fd`, `outline`, `read`, `refs`, `ast`, `edit`, `patch`, `rename`, `mcp`) |
| [`why-one-tool.md`](./why-one-tool.md) ([한국어](./why-one-tool.ko.md)) | Design rationale, failure-mode prevention, and honest tradeoffs vs existing tools |

## Agent Skill Package & Benchmarks

- Official Cross-Agent Skill: [`../skills/lancet/SKILL.md`](../skills/lancet/SKILL.md) (with [`references/`](../skills/lancet/references/))
- 8-Codebase Core Suite: [`../bench/results/SUITE.md`](../bench/results/SUITE.md)
- Indexed-Search & Agent Competitor Benchmark: [`../bench/results/COMPETITORS.md`](../bench/results/COMPETITORS.md)
- Reproducible Failure-Mode Harness: [`../bench/failure_modes.py`](../bench/failure_modes.py)
