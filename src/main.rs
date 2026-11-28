mod cli;

use clap::{CommandFactory, FromArgMatches, Parser};
use std::ffi::OsString;
use std::process::ExitCode;

#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

const ABOUT_EXAMPLES: &str =
    "Lancet: surgical code, AST, and multi-format document search, inspection, and refactoring CLI.

Examples:
  lancet fd --tree --dirs -d 2 .
  lancet rg -w spin_lock src
  lancet read src/rg.rs --symbol run
  lancet read report.pdf --page 2
  lancet edit src/net.rs --old-string 'retries = 3' --new-string 'retries = 5' --apply
  lancet refs --rank '*Service' --kind class .
  lancet ast '$X.unwrap()' src --lang rs --rewrite '$X?'";

#[derive(Parser)]
#[command(name = "lancet", version, arg_required_else_help = true)]
struct TopCli {
    /// One of the commands above
    command: String,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = true)]
    rest: Vec<OsString>,
}

type Runner = fn(&[OsString]) -> ExitCode;

/// Each command's entry point; `lancet::commands::COMMANDS` lists the same names.
const RUNNERS: &[(&str, Runner)] = &[
    ("fd", cli::fd::main),
    ("rg", cli::rg::main),
    ("read", cli::read::main),
    ("edit", cli::edit::main),
    ("write", cli::write::main),
    ("patch", cli::patch::main),
    ("refs", cli::refs::main),
    ("outline", cli::outline::main),
    ("ast", cli::ast::main),
    ("rename", cli::rename::main),
    ("index", cli::index::main),
    ("mcp", cli::mcp::main),
];

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let name = args.get(1).and_then(|a| a.to_str());
    if let Some((_, run)) = RUNNERS.iter().find(|(n, _)| Some(*n) == name) {
        return run(&args[1..]);
    }
    // Only --help / --version / no arguments get here without an error.
    let about = format!(
        "{ABOUT_EXAMPLES}\n\nCommands:\n{}",
        lancet::commands::help_list()
    );
    let matches = TopCli::command().about(about).get_matches_from(&args);
    let cli = TopCli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    eprintln!(
        "error: unknown command `{}` (run `lancet --help` for the list)",
        cli.command
    );
    ExitCode::from(2)
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_listed_command_runs_and_every_runner_is_listed() {
        let listed: Vec<&str> = lancet::commands::COMMANDS.iter().map(|c| c.name).collect();
        let run: Vec<&str> = super::RUNNERS.iter().map(|(n, _)| *n).collect();
        assert_eq!(listed, run);
    }
}
