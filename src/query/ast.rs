//! What a command asks the index for, and which files it looks at.
//!
//! [`Expr`] is the requirement a search puts on a file's content; the index turns
//! it into a trigram filter ([`crate::index::Req`]) that can only drop files that
//! cannot match. [`Filters`] / [`FileType`] narrow the files by name.

use serde::Serialize;
use std::path::Path;

/// A primitive that produces match positions inside a file.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Matcher {
    /// Case-insensitive (Unicode simple case folding) substring.
    Lit { text: String },
    /// Rust `regex` syntax, multi-line mode (`^`/`$` match at line boundaries).
    /// Case-sensitive unless the pattern uses `(?i)`.
    Regex { pattern: String },
}

/// A requirement on a file's content: every matching file satisfies it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Expr {
    Match { m: Matcher },
    And { args: Vec<Expr> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileType {
    Code,
    Document,
    Spreadsheet,
    Config,
    Log,
    Data,
    Archive,
    Image,
    Other,
}

impl FileType {
    pub const ALL: [FileType; 9] = [
        FileType::Code,
        FileType::Document,
        FileType::Spreadsheet,
        FileType::Config,
        FileType::Log,
        FileType::Data,
        FileType::Archive,
        FileType::Image,
        FileType::Other,
    ];

    pub fn name(self) -> &'static str {
        match self {
            FileType::Code => "code",
            FileType::Document => "document",
            FileType::Spreadsheet => "spreadsheet",
            FileType::Config => "config",
            FileType::Log => "log",
            FileType::Data => "data",
            FileType::Archive => "archive",
            FileType::Image => "image",
            FileType::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "code" => Some(FileType::Code),
            "document" | "doc" | "docs" => Some(FileType::Document),
            "spreadsheet" | "table" => Some(FileType::Spreadsheet),
            "config" => Some(FileType::Config),
            "log" | "logs" => Some(FileType::Log),
            "data" => Some(FileType::Data),
            "archive" | "archives" => Some(FileType::Archive),
            "image" | "images" => Some(FileType::Image),
            "other" => Some(FileType::Other),
            _ => None,
        }
    }

    pub fn from_path(p: &Path) -> Self {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(
            name,
            "Makefile"
                | "makefile"
                | "Dockerfile"
                | "CMakeLists.txt"
                | "Rakefile"
                | "Gemfile"
                | "Justfile"
                | "BUILD"
                | "WORKSPACE"
        ) {
            return FileType::Code;
        }
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        Self::from_ext(&ext.to_ascii_lowercase())
    }

    pub fn from_ext(ext: &str) -> Self {
        match ext {
            "rs" | "py" | "pyi" | "js" | "mjs" | "cjs" | "ts" | "mts" | "cts" | "tsx" | "jsx"
            | "go" | "java" | "kt" | "kts" | "scala" | "sc" | "c" | "h" | "cc" | "cpp" | "cxx"
            | "hpp" | "hh" | "hxx" | "cs" | "swift" | "m" | "mm" | "rb" | "php" | "pl" | "pm"
            | "lua" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "r" | "jl" | "dart" | "ex"
            | "exs" | "erl" | "hs" | "ml" | "mli" | "clj" | "sql" | "vue" | "svelte" | "css"
            | "scss" | "zig" | "nim" | "cu" | "proto" => FileType::Code,
            "md" | "markdown" | "txt" | "rst" | "adoc" | "org" | "tex" | "html" | "htm" | "pdf"
            | "docx" | "pptx" | "odt" | "hwp" | "hwpx" | "epub" => FileType::Document,
            "csv" | "tsv" | "xlsx" | "xls" | "xlsm" | "xlsb" | "ods" => FileType::Spreadsheet,
            "json" | "jsonc" | "yaml" | "yml" | "toml" | "xml" | "ini" | "cfg" | "conf" | "env"
            | "properties" | "tf" | "tfvars" | "hcl" => FileType::Config,
            "log" => FileType::Log,
            "jsonl" | "ndjson" => FileType::Data,
            "zip" | "tar" | "gz" | "tgz" => FileType::Archive,
            "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp" => FileType::Image,
            _ => FileType::Other,
        }
    }
}

/// File-level filters. Within a category values are OR-ed; categories are AND-ed.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Filters {
    pub types: Vec<FileType>,
    pub exts: Vec<String>,
    /// Globs on the path relative to the search root. A glob without
    /// metacharacters also matches everything below it (`src` == `src/**`).
    pub paths: Vec<String>,
}
