//! `lancet edit`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_edit(EditCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet edit",
    about = "Replace, delete, or insert lines addressed like `lancet read`, or exact text with --find (dry-run diff unless --apply)",
    group(ArgGroup::new("target").required(true).args(["lines", "symbol", "find", "edits"])),
    group(ArgGroup::new("op").args(["with", "with_file", "delete", "insert_before", "insert_after"]).conflicts_with("edits"))
)]
pub(crate) struct EditCli {
    /// File to edit (plain-text files; documents and archives are read-only).
    /// Optional with --edits when every hunk names its own "path"
    path: Option<PathBuf>,
    /// 1-based line range, as in `lancet read` (`10:50`, `42`, `100:`)
    #[arg(short = 'l', long)]
    lines: Option<String>,
    /// The lines of an AST symbol, exactly as `lancet read --symbol` shows them
    /// (doc comments and attributes above it included: replace them too, or they go)
    #[arg(short = 's', long)]
    symbol: Option<String>,
    /// Exact TEXT to change (not whole lines; may span lines). Refused unless it
    /// occurs exactly once, or pass --replace-all to change every occurrence
    #[arg(
        short = 'f',
        long = "old-string",
        visible_alias = "find",
        allow_hyphen_values = true
    )]
    find: Option<String>,
    /// With --old-string: change every occurrence instead of requiring exactly one
    #[arg(long = "replace-all", visible_alias = "all", requires = "find")]
    all: bool,
    /// With --old-string: require exactly N occurrences and change all of them
    #[arg(long, value_name = "N", requires = "find")]
    expected_replacements: Option<usize>,
    /// Replace the addressed lines (or the --old-string text) with TEXT
    #[arg(
        long = "new-string",
        visible_alias = "with",
        allow_hyphen_values = true
    )]
    with: Option<String>,
    /// Replace the addressed lines with the contents of FILE (`-` reads stdin)
    #[arg(long)]
    with_file: Option<PathBuf>,
    /// Delete the addressed lines
    #[arg(long)]
    delete: bool,
    /// Insert TEXT before the addressed lines
    #[arg(long, allow_hyphen_values = true)]
    insert_before: Option<String>,
    /// Insert TEXT after the addressed lines
    #[arg(long, allow_hyphen_values = true)]
    insert_after: Option<String>,
    /// Several hunks in one atomic call, from a JSON file (`-` reads stdin). Hunks may
    /// name their file (`"path"`, with that file's `"rev"`) to change several files at once:
    /// `[{"lines": "10:12", "with": "…"}, {"symbol": "f", "delete": true}]`.
    /// All hunks use the original file's line numbers.
    #[arg(long)]
    edits: Option<PathBuf>,
    /// Refuse unless the file's content revision equals REV (from read/ast/edit JSON)
    #[arg(long)]
    rev: Option<String>,
    /// Write the change atomically (default: print a dry-run diff)
    #[arg(long)]
    apply: bool,
    /// Write even if the edit adds Tree-sitter syntax errors
    #[arg(long)]
    allow_syntax_errors: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

pub(crate) fn run_edit(cli: EditCli) -> ExitCode {
    if let Some(f) = &cli.edits {
        let files = match read_text_arg(f)
            .map_err(|e| format!("cannot read {}: {e}", f.display()))
            .and_then(|j| {
                lancet::edit_cmd::parse_file_hunks(&j, cli.path.as_deref(), cli.rev.as_deref())
            }) {
            Ok(fs) => fs,
            Err(msg) => return cli_invalid(cli.json, &msg),
        };
        if let ([one], Some(p)) = (files.as_slice(), &cli.path) {
            if one.path == *p {
                // Every hunk is for PATH: the single-file edit, same output as before.
                let mut opts = EditOptions::multi(p.clone(), one.hunks.clone());
                opts.rev = one.rev.clone();
                opts.apply = cli.apply;
                opts.allow_syntax_errors = cli.allow_syntax_errors;
                return report_edit(&edit_file(&opts), cli.json);
            }
        }
        let resp = lancet::edit_cmd::edit_files(&files, cli.apply, cli.allow_syntax_errors);
        return report_multi_edit(&resp, cli.json);
    }
    let Some(path) = cli.path.clone() else {
        return cli_invalid(
            cli.json,
            "give the file to edit (or --edits with a \"path\" in every hunk)",
        );
    };
    let target = match (&cli.lines, cli.symbol, cli.find) {
        (Some(l), _, _) => match parse_line_spec(l) {
            Ok((a, b)) => Target::Lines(a, b),
            Err(msg) => return cli_invalid(cli.json, &msg),
        },
        (None, Some(s), _) => Target::Symbol(s),
        (None, None, Some(text)) => Target::Find {
            text,
            all: cli.all,
            count: cli.expected_replacements,
        },
        (None, None, None) => unreachable!("clap requires --lines, --symbol, --find or --edits"),
    };
    let op = if cli.delete {
        Op::Delete
    } else if let Some(t) = cli.with {
        Op::Replace(t)
    } else if let Some(f) = cli.with_file {
        match read_text_arg(&f) {
            Ok(t) => Op::Replace(t),
            Err(e) => return cli_invalid(cli.json, &format!("cannot read {}: {e}", f.display())),
        }
    } else if let Some(t) = cli.insert_before {
        Op::InsertBefore(t)
    } else if let Some(t) = cli.insert_after {
        Op::InsertAfter(t)
    } else {
        return cli_invalid(
            cli.json,
            "give one of --with, --with-file, --delete, --insert-before, --insert-after (or --edits)",
        );
    };

    let mut opts = EditOptions::new(path, target, op);
    opts.rev = cli.rev;
    opts.apply = cli.apply;
    opts.allow_syntax_errors = cli.allow_syntax_errors;
    report_edit(&edit_file(&opts), cli.json)
}

pub(crate) fn report_multi_edit(
    resp: &lancet::edit_cmd::MultiEditResponse,
    json: bool,
) -> ExitCode {
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if json {
        let _ = serde_json::to_writer(&mut out, resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        for f in &resp.files {
            let _ = write!(out, "{}", f.diff);
        }
        let _ = out.flush();
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        } else if resp.status == Status::Empty {
            eprintln!("[lancet edit] no change");
        } else {
            eprintln!(
                "[lancet edit] {}: {} file(s){}",
                resp.mode,
                resp.files.len(),
                if resp.applied {
                    " (pass each file's new rev to its next edit)"
                } else {
                    " — re-run with --apply; put each file's rev in its hunks' \"rev\" field"
                }
            );
            for f in &resp.files {
                let (r, a) = (
                    f.rev.as_deref().unwrap_or("-"),
                    f.rev_after.as_deref().unwrap_or("-"),
                );
                if resp.applied {
                    eprintln!("  {}: rev {r} -> {a}", f.path);
                } else {
                    eprintln!("  {}: at rev {r} (afterwards {a})", f.path);
                }
            }
        }
    }
    let _ = out.flush();
    exit(resp.status)
}

pub(crate) fn report_edit(resp: &lancet::edit_cmd::EditResponse, json: bool) -> ExitCode {
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if json {
        let _ = serde_json::to_writer(&mut out, resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        if !resp.diff.is_empty() {
            let _ = write!(out, "{}", resp.diff);
        }
        let _ = out.flush();
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        } else if resp.status == Status::Empty {
            eprintln!("[lancet edit] no change");
        } else {
            let what = if resp.hunks.len() > 1 {
                format!("{} hunks", resp.hunks.len())
            } else {
                format!("lines {}-{}", resp.start_line, resp.end_line)
            };
            eprintln!(
                "[lancet edit] {}: {what} of {}; {}",
                resp.mode,
                resp.path,
                rev_note(resp.applied, resp.rev.as_deref(), resp.rev_after.as_deref()),
            );
        }
    }
    let _ = out.flush();
    exit(resp.status)
}
