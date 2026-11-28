---
nav_title: "AI Agents, MCP & Skill"
title: "AI Agents, Built-in MCP Server & Official Skill"
description: "Configure lancet mcp across Claude Code, Cursor, Codex, Antigravity, and Claude Desktop, plus install skills/lancet."
lede: "One `lancet` tool that takes CLI arguments, over newline-delimited stdio JSON-RPC, with --read-only and --root, plus the cross-agent Official Skill."
keywords: "mcp server root read-only claude cursor codex antigravity skill agents.md max_output_bytes args stdin"
source: "https://github.com/CREE1116/lancet/blob/main/docs/mcp.md"
---
AI coding agents can drive Lancet via shell CLI or its zero-dependency stdio **Model Context Protocol (MCP)** server — paired with the cross-agent **Official Skill** package (`skills/lancet/`).

## Starting lancet mcp {#mcp-flags}

`lancet mcp` speaks newline-delimited JSON-RPC 2.0 over `stdin`/`stdout` (protocol revisions `2025-03-26` and `2024-11-05`) and runs each tool call as the `lancet` CLI in the `--root` directory.

| Flag | Default | Exact Behavior |
| --- | --- | --- |
| `--root <DIR>` | `.` | Directory commands run in, so relative paths resolve against it (**not** a chroot/sandbox; absolute or `..` paths still resolve like the CLI) |
| `--read-only` | `false` | Refuses commands that write: `edit`, `patch`, `rename`, `index`, and anything with `--apply` |
| `--trust-index` | `false` | Adds `--trust-index` to searches (`rg`, `fd`, `ast`, `refs`) that don't pick an index mode themselves |
| `-j, --threads <NUM>` | CPU count | Adds `-j NUM` to those searches unless they set it |

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

The [`skills/lancet/`](https://github.com/CREE1116/lancet/tree/main/skills/lancet) directory bundles `SKILL.md` plus detailed `references/` for `rg` and AST patterns, 24 AST languages, edit/patch/rename CAS rules, and MCP configs.

```bash title="Terminal"
# Project-local (.agents standard)
mkdir -p .agents/skills && ln -s "$(pwd)/skills/lancet" .agents/skills/lancet

# Personal install (Claude Code / Codex / Antigravity)
mkdir -p ~/.claude/skills && cp -R skills/lancet ~/.claude/skills/lancet
mkdir -p ~/.codex/skills  && cp -R skills/lancet ~/.codex/skills/lancet
mkdir -p ~/.gemini/config/skills && cp -R skills/lancet ~/.gemini/config/skills/lancet
```

## The `lancet` tool {#mcp-tool}

The server exposes **one tool**, `lancet`. Its arguments are the command line: `{"args": ["rg", "-w", "spin_lock", "src"]}` runs exactly what `lancet rg -w spin_lock src` runs — same binary, flags, output and exit codes — in the `--root` directory. Every CLI feature is available over MCP as soon as it ships, and the whole schema is about 2.4 KB (the earlier 11-tool server was 12.5 KB).

| Field | Type | Meaning |
| --- | --- | --- |
| `args` | string array (required) | Arguments after `lancet`, one per element. No shell: no quoting, globbing, pipes or `&&` (`["\"OOM\" in line", "src"]`, not `["'\"OOM\" in line'"]`). |
| `stdin` | string | Text for a `-` argument: the hunks for `edit --edits -`, the diff for `patch -`, the text for `--with-file -`. |
| `max_output_bytes` | integer | Output cap, default `131072`, clamped to `256..=4194304`; longer output is cut and marked `truncated`. |

The result is the command's stdout, then its stderr, then `[exit N]` when it is not `0`. `isError` is `true` only for exit `2` (refused or invalid); exit `1` means nothing was found and is not an error.

```json title="tools/call"
{"name": "lancet", "arguments": {"args": ["edit", "--edits", "-", "--apply"],
  "stdin": "[{\"path\": \"src/a.rs\", \"rev\": \"7b47…\", \"find\": \"old(\", \"with\": \"new(\"}]"}}
```

```bash title="AGENTS.md / CLAUDE.md snippet"
Prefer lancet (the lancet CLI, or the `lancet` MCP tool with the same arguments) to rg/fd/ast-grep/cat/sed.
Orient with fd/outline/refs --rank, locate with rg/refs/ast, note the rev from read, change with edit/patch/rename --rev --apply.
```
