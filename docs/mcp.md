# Lancet MCP Server & Official Agent Skill

Lancet ships a cross-agent Skill package ([`skills/lancet/SKILL.md`](../skills/lancet/SKILL.md)) for agents that have a shell, and a built-in Model Context Protocol (MCP) stdio server (`lancet mcp`) for clients that don't. The server has one tool, `lancet`, that takes the same arguments as the CLI.

---

## 1. Quick Setup: `lancet mcp`

`lancet mcp` speaks newline-delimited JSON-RPC 2.0 over `stdin`/`stdout` (MCP revisions `2025-03-26` and `2024-11-05`) and runs each tool call as the `lancet` CLI in the `--root` directory.

### Flags

| Flag | Default | Description |
|---|---|---|
| `--root <DIR>` | `.` | Directory commands run in, so relative paths resolve against it (not a chroot/sandbox; absolute or `..` paths work like the CLI) |
| `--read-only` | `false` | Refuses commands that write: `edit`, `patch`, `rename`, `index`, and anything with `--apply` |
| `--trust-index` | `false` | Adds `--trust-index` to searches (`rg`, `fd`, `ast`, `refs`, `batch`, queries) that don't pick an index mode themselves |
| `-j`, `--threads <NUM>` | CPU count | Adds `-j NUM` to those searches unless they set it |

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

## 3. The `lancet` Tool

One tool. Its arguments are the command line, so every CLI feature works over MCP as soon as it ships and the schema stays small (about 2.4 KB; the earlier 11-tool server was 12.5 KB).

| Field | Type | Meaning |
|---|---|---|
| `args` | string array (required) | Arguments after `lancet`, one per element. No shell: no quoting, globbing, pipes or `&&`. |
| `stdin` | string | Text for a `-` argument: hunks for `edit --edits -`, a diff for `patch -`, text for `--with-file -`. |
| `max_output_bytes` | integer | Output cap (default `131072`, clamped to `256..=4194304`). |

```json
{"name": "lancet", "arguments": {"args": ["rg", "-w", "spin_lock", "src"]}}
{"name": "lancet", "arguments": {"args": ["read", "src/x.rs", "--symbol", "handle"]}}
{"name": "lancet", "arguments": {"args": ["edit", "--edits", "-", "--apply"],
  "stdin": "[{\"path\": \"src/a.rs\", \"rev\": \"7b47…\", \"find\": \"old(\", \"with\": \"new(\"}]"}}
```

The result text is the command's stdout, then stderr, then `[exit N]` for a non-zero exit. `isError` is `true` only for exit `2` (refused or invalid); exit `1` means nothing was found. Writes are previews until `--apply`, exactly as on the command line.

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
