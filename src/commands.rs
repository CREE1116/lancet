//! The one list of lancet's commands (docs/ARCHITECTURE.md, pattern ⑦).
//!
//! The top-level help, the MCP tool (which commands exist, which write, which
//! search), and the skill-table test all read it, so they cannot drift apart.

pub struct CommandSpec {
    pub name: &'static str,
    /// What follows `lancet` in the help's command list.
    pub usage: &'static str,
    pub summary: &'static str,
    /// Changes files or the on-disk index (refused on a read-only MCP server).
    pub writes: bool,
    /// Accepts `--trust-index` and `-j`.
    pub searches: bool,
}

const fn cmd(
    name: &'static str,
    usage: &'static str,
    summary: &'static str,
    writes: bool,
    searches: bool,
) -> CommandSpec {
    CommandSpec {
        name,
        usage,
        summary,
        writes,
        searches,
    }
}

// Columns: name, usage, summary, writes, searches.
#[rustfmt::skip]
pub static COMMANDS: &[CommandSpec] = &[
    cmd("fd", "fd [PATTERN] [PATH...]", "find files and folders (`--tree`, globs like '**/*.ts')", false, true),
    cmd("rg", "rg [FLAGS] PATTERN [PATH...]", "ripgrep: same flags, output and exit codes", false, true),
    cmd("read", "read TARGET...", "lines, symbols, pages, sheets, archive entries (`--offset/--limit`)", false, false),
    cmd("edit", "edit PATH", "replace / delete / insert by lines, symbol or exact text (`--old-string`)", true, false),
    cmd("write", "write PATH", "create a file, or replace one you have read (`--rev`)", true, false),
    cmd("patch", "patch [DIFF|-]", "apply a unified diff or a Codex patch, all files or none", true, false),
    cmd("refs", "refs SYMBOL [PATH...]", "definitions and references (`--rank` for the most used)", false, true),
    cmd("outline", "outline [PATH...]", "declarations, headings, sheets, archive contents", false, false),
    cmd("ast", "ast PATTERN [PATH...]", "structural search and rewrite (24 languages)", false, true),
    cmd("rename", "rename FILE:LINE OLD NEW", "rename a local variable and its uses", true, false),
    cmd("index", "index [PATH]", "build or update the index (optional; results are always fresh; `--status`)", true, false),
    cmd("mcp", "mcp", "MCP server with one `lancet` tool taking these arguments", false, false),
];

pub fn get(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// The `Commands:` block of `lancet --help`.
pub fn help_list() -> String {
    let width = COMMANDS.iter().map(|c| c.usage.len()).max().unwrap_or(0) + 2;
    COMMANDS
        .iter()
        .map(|c| format!("  lancet {:width$} {}", c.usage, c.summary))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_shows_every_command_an_agent_runs() {
        let skill = include_str!("../skills/lancet/SKILL.md");
        for c in COMMANDS.iter().filter(|c| !matches!(c.name, "mcp")) {
            assert!(
                skill.contains(&format!("lancet {} ", c.name)),
                "skills/lancet/SKILL.md never shows `lancet {}`",
                c.name
            );
        }
    }

    #[test]
    fn names_are_unique_and_usage_starts_with_the_name() {
        for (i, c) in COMMANDS.iter().enumerate() {
            assert!(c.usage == c.name || c.usage.starts_with(&format!("{} ", c.name)));
            assert!(COMMANDS[..i].iter().all(|d| d.name != c.name));
        }
    }
}
