//! `lancet write`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_write(WriteCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet write",
    about = "Create a file, or replace a whole file you have read (--rev); preview unless --apply"
)]
pub(crate) struct WriteCli {
    /// File to create or replace
    path: PathBuf,
    /// The new content
    #[arg(long, allow_hyphen_values = true, conflicts_with = "with_file")]
    content: Option<String>,
    /// Read the new content from FILE (`-` reads stdin)
    #[arg(long, value_name = "FILE")]
    with_file: Option<PathBuf>,
    /// Required to replace an existing file: the rev you read it at
    #[arg(long)]
    rev: Option<String>,
    /// Write the file (default: print a preview diff)
    #[arg(long)]
    apply: bool,
    /// Write even if the content has more Tree-sitter syntax errors than the file it replaces
    #[arg(long)]
    allow_syntax_errors: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

pub(crate) fn run_write(cli: WriteCli) -> ExitCode {
    let content = match (cli.content, &cli.with_file) {
        (Some(c), _) => c,
        (None, Some(f)) => match read_text_arg(f) {
            Ok(t) => t,
            Err(e) => return cli_invalid(cli.json, &format!("cannot read {}: {e}", f.display())),
        },
        (None, None) => {
            return cli_invalid(
                cli.json,
                "give the content with --content TEXT or --with-file FILE (`-` = stdin)",
            )
        }
    };
    let o = lancet::edit_cmd::WriteOptions {
        path: cli.path,
        content,
        rev: cli.rev,
        apply: cli.apply,
        allow_syntax_errors: cli.allow_syntax_errors,
    };
    let resp = lancet::edit_cmd::write_file(&o);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if cli.json {
        let _ = serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        let _ = write!(out, "{}", resp.diff);
        let _ = out.flush();
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        } else if resp.status == Status::Empty {
            eprintln!(
                "[lancet write] no change: {} already has that content",
                resp.path
            );
        } else {
            let after = resp.rev_after.as_deref().unwrap_or("-");
            match (resp.applied, resp.rev.as_deref()) {
                (false, None) => eprintln!(
                    "[lancet write] dry-run: would create {} ({} lines); re-run with --apply",
                    resp.path, resp.end_line
                ),
                (false, Some(r)) => eprintln!(
                    "[lancet write] dry-run: {}; {}",
                    resp.path,
                    rev_note(false, Some(r), Some(after))
                ),
                (true, None) => eprintln!("[lancet write] created {} (rev {after})", resp.path),
                (true, Some(r)) => eprintln!(
                    "[lancet write] applied: {}; {}",
                    resp.path,
                    rev_note(true, Some(r), Some(after))
                ),
            }
        }
    }
    let _ = out.flush();
    exit(resp.status)
}
