//! `lancet mcp`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_mcp(McpCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet mcp",
    about = "Run a Model Context Protocol (MCP) server over stdio: one `lancet` tool that takes CLI arguments"
)]
pub(crate) struct McpCli {
    /// Directory commands run in (relative paths resolve against it)
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Refuse commands that write: edit, write, patch, rename, index, and anything with --apply
    #[arg(long)]
    read_only: bool,
    /// Add --trust-index to searches that don't pick an index mode themselves
    #[arg(long, hide = true)]
    trust_index: bool,
    /// Add -j N to searches that don't set it themselves
    #[arg(short = 'j', long)]
    threads: Option<usize>,
}

pub(crate) fn run_mcp(cli: McpCli) -> ExitCode {
    let mut cfg = lancet::mcp::McpConfig::new(cli.root);
    cfg.read_only = cli.read_only;
    cfg.trust_index = cli.trust_index;
    cfg.threads = cli.threads;
    match lancet::mcp::serve_stdio(cfg) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lancet mcp: {e}");
            ExitCode::from(2)
        }
    }
}
