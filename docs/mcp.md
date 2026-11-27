# Lancet MCP Server & Official Agent Skill

Lancet ships a built-in Model Context Protocol (MCP) stdio server (`lancet mcp`) and a cross-agent Skill package ([`skills/lancet/SKILL.md`](../skills/lancet/SKILL.md)) so AI coding agents can use Lancet without shell-escaping friction or external daemons.

---

## 1. Quick Setup: `lancet mcp`

`lancet mcp` speaks newline-delimited JSON-RPC 2.0 over `stdin`/`stdout` (MCP revisions `2025-03-26` and `2024-11-05`) and invokes Lancet's Rust engines directly in-process.

### Flags

| Flag | Default | Description |
|---|---|---|
| `--root <DIR>` | `.` | Default working directory for resolving relative paths in tool calls (not a chroot/sandbox; absolute or `..` paths work like the CLI) |
| `--read-only` | `false` | Omits `lancet_edit`, `lancet_patch`, and `lancet_rename` from `tools/list` and refuses writes (`lancet_ast` `apply`, `lancet_index` mutating actions) |
| `--trust-index` | `false` | Default `index_mode` becomes `trust` instead of `auto` (fastest; skips filesystem change checks unless overridden per call) |
| `-j`, `--threads <NUM>` | CPU count | Rayon worker thread count |

---

## 2. Client Configurations

Ready-to-copy JSON snippets are also in [`skills/lancet/references/mcp-config.json`](../skills/lancet/references/mcp-config.json).

### Claude Code

```bash
claude mcp add --transport stdio lancet -- lancet mcp --root .
```

Or read-only:

```bash
claude mcp add --transport stdio lancet -- lancet mcp --root . --read-only
```

### Cursor (`.cursor/mcp.json` or `~/.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "lancet": {
      "command": "lancet",
      "args": ["mcp", "--root", "."]
    }
  }
}
```

### Claude Desktop / Windsurf / Antigravity (`mcp_config.json`)

Use an absolute `--root` path when the desktop client launches without your repository as its working directory:

```json
{
  "mcpServers": {
    "lancet": {
      "command": "lancet",
      "args": ["mcp", "--root", "/path/to/your/repo"]
    }
  }
}
```

### OpenAI Codex (`~/.codex/config.toml`)

```toml
[mcp_servers.lancet]
command = "lancet"
args = ["mcp", "--root", "."]
```

---

## 3. Exposed MCP Tools

Every read/search tool treats `0` matches as a normal `empty` result (`isError: false`). Write operations default to `apply: false` (dry-run unified diff) and require `apply: true` to touch disk.

| Tool | Role | Notes |
|---|---|---|
| `lancet_fd` | `.gitignore`-aware file/dir discovery & `--tree` | Supports `dirs: true`, `pattern_mode`, `exts`, `type_classes`, `changed_within`, `min_size`/`max_size`, `index_mode` |
| `lancet_search` | Lancet DSL query (`AND`/`OR`/`NOT`, `near`, `fuzzy`, `regex`, pipes) | Supports `--budget` pipes, `max_lines`, `max_cols`, `ocr` |
| `lancet_rg` | `ripgrep`-compatible regex/fixed search | Always returns deterministic path-sorted lines with line numbers; `docs: true` enables PDF/Office/HWP/archives; supports `passthru` & `type_add`; output capped cleanly at line boundaries via `max_output_bytes` |
| `lancet_outline` | Declaration skeletons, Markdown headings, sheets, archive manifests | Supports `max_depth`, `exts`, `limit`, `ocr` |
| `lancet_read` | Exact `lines`, `symbol`, `page`, `sheet`/`rows`/`cols`, `archive.zip!entry` | Returns `rev` (XXH3-64) on plain-text files and `next_line` with `budget_tokens` |
| `lancet_refs` | Index-accelerated `[def]` / `[ref]` lexical AST lookup & `--rank` | Excludes comments & string literals across 24 languages; `rank: true` + `kinds` ranks declarations matching `symbol` glob by reference count |
| `lancet_ast` | Tree-sitter structural search & optional `rewrite` | 24 languages; `apply: false` (dry-run) by default; `apply: true` refused under `--read-only` |
| `lancet_edit` | Locked CAS line/symbol/`find`/multi-hunk `edits` | Omitted under `--read-only`; verifies `rev` and Tree-sitter syntax gate |
| `lancet_patch` | Atomic multi-file unified diff apply (`git diff` / `diff -u`) | Omitted under `--read-only`; exact hunk matching + race rollback |
| `lancet_rename` | Tier-1 local-scope binding rename in one file (`path`, `line`, `col?`, `old`, `new`) | Omitted under `--read-only`; Rust/Python/JS/TS/TSX/Go scope resolution + shadowing check + CAS |
| `lancet_index` | `.lancet` index `status`, `sync`, `rebuild`, `compact` | Mutating actions (`sync`/`rebuild`/`compact`) refused under `--read-only` |

Built-in MCP prompts (`prompts/list` / `prompts/get`): `lancet-explore` and `lancet-safe-edit`.

---

## 4. Installing the Official Agent Skill

Install [`skills/lancet`](../skills/lancet) (including `SKILL.md` and `references/` covering LQL, 24 AST languages, `edit`/`patch`/`rename` schemas, and MCP configs) into whichever agent harness you use:

```bash
# Project-local (universal .agents convention)
mkdir -p .agents/skills && ln -s "$(pwd)/skills/lancet" .agents/skills/lancet

# Claude Code (personal)
mkdir -p ~/.claude/skills && cp -R skills/lancet ~/.claude/skills/lancet

# Codex (personal)
mkdir -p ~/.codex/skills && cp -R skills/lancet ~/.codex/skills/lancet

# Antigravity (personal)
mkdir -p ~/.gemini/config/skills && cp -R skills/lancet ~/.gemini/config/skills/lancet
```
