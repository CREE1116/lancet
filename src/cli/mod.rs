//! The command-line interface: one module per command, each with its clap
//! arguments, its `main(args)` and its output; the shared helpers are here.

pub(crate) mod ast;
pub(crate) mod edit;
pub(crate) mod fd;
pub(crate) mod index;
pub(crate) mod mcp;
pub(crate) mod outline;
pub(crate) mod patch;
pub(crate) mod read;
pub(crate) mod refs;
pub(crate) mod rename;
pub(crate) mod rg;
pub(crate) mod write;

pub(crate) use clap::{ArgAction, ArgGroup, Parser};
pub(crate) use lancet::ast_grep::{print_ast_human, search_ast, AstOptions};
pub(crate) use lancet::edit_cmd::{edit_file, EditOptions, Op, Target};
pub(crate) use lancet::index::Index;
pub(crate) use lancet::outline::{
    build_outline, print_outline_human, OutlineOptions, OutlineResponse,
};
pub(crate) use lancet::output::Status;
pub(crate) use lancet::query::FileType;
pub(crate) use lancet::read_cmd::{
    parse_col_spec, parse_line_spec, print_read_human, read_file_range, ReadOptions,
};
pub(crate) use lancet::refs::{find_refs, print_refs_human, RefsOptions};
pub(crate) use lancet::scope::IndexMode;
pub(crate) use lancet::walk::WalkOpts;
pub(crate) use std::ffi::OsString;
pub(crate) use std::io::{self, IsTerminal, Write};
pub(crate) use std::path::{Path, PathBuf};
pub(crate) use std::process::ExitCode;

pub(crate) fn init_threads(n: Option<usize>) {
    if let Some(n) = n {
        let _ = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global();
    }
}

/// 0 done, 1 nothing found, 2 refused or invalid (as grep and ripgrep).
pub(crate) fn exit(status: Status) -> ExitCode {
    ExitCode::from(status.exit_code())
}

pub(crate) fn index_mode(no_index: bool, trust_index: bool) -> IndexMode {
    if no_index {
        IndexMode::Off
    } else if trust_index {
        IndexMode::Trust
    } else {
        IndexMode::Auto
    }
}

pub(crate) fn cli_invalid(json: bool, msg: &str) -> ExitCode {
    if json {
        println!(
            "{}",
            serde_json::json!({ "status": "invalid_query", "error": { "message": msg } })
        );
    } else {
        eprintln!("error: {msg}");
    }
    ExitCode::from(2)
}

/// Which revision to pass next: before writing, the file's current rev (what
/// `--rev` must match); after writing, the new one (for the next edit).
pub(crate) fn rev_note(applied: bool, rev: Option<&str>, rev_after: Option<&str>) -> String {
    let (r, a) = (rev.unwrap_or("-"), rev_after.unwrap_or("-"));
    if applied {
        format!("rev {r} -> {a} (pass --rev {a} to the next edit)")
    } else {
        format!("file is at rev {r}: re-run with --rev {r} --apply to write (afterwards it will be at rev {a})")
    }
}

pub(crate) fn read_text_arg(f: &Path) -> io::Result<String> {
    if f.as_os_str() == "-" {
        let mut s = String::new();
        io::Read::read_to_string(&mut io::stdin(), &mut s).map(|_| s)
    } else {
        std::fs::read_to_string(f)
    }
}
