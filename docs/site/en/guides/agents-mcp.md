---
nav_title: "AI Agents, MCP & Skill"
title: "AI Agents, Built-in MCP Server & Official Skill"
description: "Configure lancet mcp across Claude Code, Cursor, Codex, Antigravity, and Claude Desktop, plus install skills/lancet."
lede: "11 tools over newline-delimited stdio JSON-RPC with --read-only, --root, and the cross-agent Official Skill."
keywords: "mcp server root read-only claude cursor codex antigravity skill agents.md max_output_bytes prompts"
source: "https://github.com/CREE1116/lancet/blob/main/docs/mcp.md"
---
AI coding agents can drive Lancet via shell CLI or its zero-dependency stdio **Model Context Protocol (MCP)** server — paired with the cross-agent **Official Skill** package (`skills/lancet/`).

## Starting lancet mcp {#mcp-flags}

`lancet mcp` speaks newline-delimited JSON-RPC 2.0 over `stdin`/`stdout` supporting protocol revisions `2025-03-26` and `2024-11-05`, invoking Lancet's Rust engines directly in-process.

| Flag | Default | Exact Behavior |
| --- | --- | --- |
| `--root <DIR>` | `.` | Base directory for relative paths in tool arguments (**not** a chroot/sandbox; absolute or `..` paths still resolve like the CLI) |
| `--read-only` | `false` | Removes `lancet_edit`, `lancet_patch`, and `lancet_rename` from `tools/list` and refuses writes (`lancet_ast` `apply`, mutating `lancet_index` actions) |
| `--trust-index` | `false` | Sets default `index_mode` to `trust` instead of `auto` unless overridden per tool call |
| `-j, --threads <NUM>` | CPU count | Rayon worker threads |

## Ready-to-Copy Client Configurations {#client-configs}

Full template bundle: [`skills/lancet/references/mcp-config.json`](https://github.com/CREE1116/lancet/blob/main/skills/lancet/references/mcp-config.json).

### Claude Code {#claude-code}

```bash title="Terminal"
# Read + CAS write tools
claude mcp add --transport stdio lancet -- lancet mcp --root .

# Safe read-only research server
claude mcp add --transport stdio lancet -- lancet mcp --root . --read-only
```

### Cursor (.cursor/mcp.json or ~/.cursor/mcp.json) {#cursor}

```json title="JSON"
{
  "mcpServers": {
    "lancet": {
      "command": "lancet",
      "args": ["mcp", "--root", "."]
    }
  }
}
```

### Claude Desktop / Windsurf / Antigravity {#desktop-agy}

Pass an absolute `--root` path when the desktop client launches without your project as CWD:

```json title="JSON"
{
  "mcpServers": {
    "lancet": {
      "command": "lancet",
      "args": ["mcp", "--root", "/abs/path/to/your/repo"]
    }
  }
}
```

### OpenAI Codex (~/.codex/config.toml) {#codex}

```toml title="TOML"
[mcp_servers.lancet]
command = "lancet"
args = ["mcp", "--root", "."]
```

## Installing the Official Agent Skill {#skill-install}

The [`skills/lancet/`](https://github.com/CREE1116/lancet/tree/main/skills/lancet) directory bundles `SKILL.md` plus detailed `references/` for LQL, 24 AST languages, edit/patch/rename CAS rules, and MCP configs.

```bash title="Terminal"
# Project-local (.agents standard)
mkdir -p .agents/skills && ln -s "$(pwd)/skills/lancet" .agents/skills/lancet

# Personal install (Claude Code / Codex / Antigravity)
mkdir -p ~/.claude/skills && cp -R skills/lancet ~/.claude/skills/lancet
mkdir -p ~/.codex/skills  && cp -R skills/lancet ~/.codex/skills/lancet
mkdir -p ~/.gemini/config/skills && cp -R skills/lancet ~/.gemini/config/skills/lancet
```

## Built-in MCP Prompts & AGENTS.md Rule {#prompts-notes}

- **`prompts/list`:** ships `lancet-explore` (`goal` argument) and `lancet-safe-edit` (`target` argument).
- **Empty-match semantics:** read/search tools return `isError: false` on `status: "empty"` (0 hits is normal, not an exception).
- **Line-boundary byte cap:** `lancet_rg` caps stdout cleanly on a newline boundary via `max_output_bytes` (default `131072` B = 128 KiB, hard clamp `256..=4194304`).

```bash title="AGENTS.md / CLAUDE.md snippet"
Prefer lancet (lancet_* MCP tools or lancet CLI) instead of rg/fd/ast-grep/cat/sed.
Orient with fd/outline, locate with rg/search/refs/ast, capture rev via read, apply changes via edit/patch/rename --rev --apply.
```
