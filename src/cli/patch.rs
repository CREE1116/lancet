//! `lancet patch`: command-line arguments and output.

use super::*;

pub(crate) fn main(args: &[OsString]) -> ExitCode {
    run_patch(PatchCli::parse_from(args))
}

#[derive(Parser)]
#[command(
    name = "lancet patch",
    about = "Apply a unified diff (`git diff`, `diff -u`) with lancet edit's guarantees (dry-run unless --apply)"
)]
pub(crate) struct PatchCli {
    /// Patch file (`-` or omitted: stdin)
    patch: Option<PathBuf>,
    /// Directory the patch's paths are relative to
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Strip N leading path components, like `patch -pN` (1 removes `a/` and `b/`)
    #[arg(short = 'p', long = "strip", default_value_t = 1)]
    strip: usize,
    /// Write the changes (default: verify and print the resulting diff)
    #[arg(long)]
    apply: bool,
    /// Write even if a file gains Tree-sitter syntax errors
    #[arg(long)]
    allow_syntax_errors: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

pub(crate) fn run_patch(cli: PatchCli) -> ExitCode {
    let src = cli.patch.as_deref().unwrap_or(Path::new("-"));
    let text = match read_text_arg(src) {
        Ok(t) => t,
        Err(e) => return cli_invalid(cli.json, &format!("cannot read {}: {e}", src.display())),
    };
    let mut o = lancet::patch::PatchOptions::new(text, &cli.root);
    o.strip = cli.strip;
    o.apply = cli.apply;
    o.allow_syntax_errors = cli.allow_syntax_errors;
    let resp = lancet::patch::apply_patch(&o);
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    if cli.json {
        let _ = serde_json::to_writer(&mut out, &resp)
            .map_err(io::Error::other)
            .and_then(|_| writeln!(out));
    } else {
        for f in &resp.files {
            let _ = write!(out, "{}", f.diff);
        }
        let _ = out.flush();
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        } else {
            let moved = resp
                .files
                .iter()
                .flat_map(|f| &f.offsets)
                .filter(|o| **o != 0)
                .count();
            let note = if moved > 0 {
                format!(", {moved} hunk(s) found at an offset")
            } else {
                String::new()
            };
            let hint = if resp.applied {
                ""
            } else {
                " (re-run with --apply to write)"
            };
            eprintln!(
                "[lancet patch] {}: {} file(s){note}{hint}",
                resp.mode,
                resp.files.len()
            );
        }
    }
    let _ = out.flush();
    exit(resp.status)
}
