//! `lancet rename`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_rename(RenameCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet rename",
    about = "Rename a local variable, parameter or closure/loop variable and every use of it (dry-run diff unless --apply)",
    long_about = "Rename a local binding by position. A per-language scope resolver (Rust, Python, JavaScript, TypeScript, Go) links every use to its declaration, and the file is resolved again after renaming: if any name would refer to something else (shadowing, capture), nothing is written. Names declared outside a function, fields, methods, Python parameters, and names used inside macros or eval are refused rather than guessed."
)]
pub(crate) struct RenameCli {
    /// Where the name occurs: FILE:LINE or FILE:LINE:COL (1-based byte column, as `lancet rg --column` prints)
    target: String,
    /// The current name
    old: String,
    /// The new name
    new: String,
    /// Refuse unless the file's content revision equals REV (from read/edit JSON)
    #[arg(long)]
    rev: Option<String>,
    /// Write the change atomically (default: print a dry-run diff)
    #[arg(long)]
    apply: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

pub(crate) fn run_rename(cli: RenameCli) -> ExitCode {
    // FILE:LINE[:COL]; the file name itself may contain ':'.
    let mut parts = cli.target.rsplitn(3, ':').collect::<Vec<_>>();
    parts.reverse();
    let nums: Vec<Option<usize>> = parts.iter().map(|p| p.parse().ok()).collect();
    let (file, line, col) = match (parts.as_slice(), nums.as_slice()) {
        ([f, _, _], [_, Some(l), Some(c)]) => (f.to_string(), *l, Some(*c)),
        ([f, rest, _], [_, None, Some(l)]) => (format!("{f}:{rest}"), *l, None),
        ([f, _], [_, Some(l)]) => (f.to_string(), *l, None),
        _ => {
            return cli_invalid(
                cli.json,
                &format!("expected FILE:LINE or FILE:LINE:COL, got `{}`", cli.target),
            )
        }
    };
    let o = lancet::rename::RenameOptions {
        path: PathBuf::from(file),
        line,
        col,
        old: cli.old,
        new: cli.new,
        rev: cli.rev,
        apply: cli.apply,
    };
    let resp = lancet::rename::rename(&o);
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
        } else {
            let comments = if resp.comment_mentions > 0 {
                format!(
                    "; {} comment mention(s) of `{}` left unchanged",
                    resp.comment_mentions, resp.old
                )
            } else {
                String::new()
            };
            eprintln!(
                "[lancet rename] {}: `{}` -> `{}`, {} site(s) in {}{comments}; {}",
                resp.mode,
                resp.old,
                resp.new,
                resp.sites.len(),
                resp.path,
                rev_note(resp.applied, resp.rev.as_deref(), resp.rev_after.as_deref()),
            );
        }
    }
    let _ = out.flush();
    exit(resp.status)
}
