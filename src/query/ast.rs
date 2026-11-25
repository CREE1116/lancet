//! Query IR. The engine executes this tree; surface syntaxes (the readable
//! grammar in `parser.rs`, a future special-token grammar for the SFT model)
//! are just serializations of it.

use serde::Serialize;
use std::fmt;
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
    /// A word token (`[\p{Alphabetic}\p{N}_]+`) within Levenshtein distance `k`
    /// (in chars, case-insensitive) of `text`.
    Fuzzy { text: String, k: u32 },
    /// Code symbol definition matching `name` (case-insensitive, tree-sitter AST).
    Symbol { name: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Expr {
    Match {
        m: Matcher,
    },
    And {
        args: Vec<Expr>,
    },
    Or {
        args: Vec<Expr>,
    },
    Not {
        arg: Box<Expr>,
    },
    /// `a` and `b` both occur in the unit with at most `dist` word tokens
    /// between their starts. `ordered`: `a` must start before `b`.
    Near {
        a: Matcher,
        b: Matcher,
        dist: u32,
        ordered: bool,
    },
}

/// The unit over which the boolean expression is evaluated and reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Line,
    /// Maximal run of non-blank lines.
    Block,
    /// Not implemented yet (needs a real code parser).
    Function,
    #[default]
    File,
}

impl Unit {
    pub fn name(self) -> &'static str {
        match self {
            Unit::Line => "line",
            Unit::Block => "block",
            Unit::Function => "function",
            Unit::File => "file",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "line" => Some(Unit::Line),
            "block" | "paragraph" => Some(Unit::Block),
            "function" => Some(Unit::Function),
            "file" => Some(Unit::File),
            _ => None,
        }
    }
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
            "rs" | "py" | "pyi" | "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "go" | "java"
            | "kt" | "kts" | "scala" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx"
            | "cs" | "swift" | "m" | "mm" | "rb" | "php" | "pl" | "pm" | "lua" | "sh" | "bash"
            | "zsh" | "fish" | "ps1" | "r" | "jl" | "dart" | "ex" | "exs" | "erl" | "hs" | "ml"
            | "mli" | "clj" | "sql" | "vue" | "svelte" | "zig" | "nim" | "cu" | "proto" => {
                FileType::Code
            }
            "md" | "markdown" | "txt" | "rst" | "adoc" | "org" | "tex" | "html" | "htm" | "pdf"
            | "docx" | "pptx" | "odt" | "hwp" | "hwpx" | "epub" => FileType::Document,
            "csv" | "tsv" | "xlsx" | "xls" | "xlsm" | "xlsb" | "ods" => FileType::Spreadsheet,
            "json" | "jsonc" | "yaml" | "yml" | "toml" | "xml" | "ini" | "cfg" | "conf" | "env"
            | "properties" => FileType::Config,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Path,
    CountDesc,
    CountAsc,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Query {
    pub expr: Expr,
    pub unit: Unit,
    pub filters: Filters,
    /// Lines of context around each matching line.
    pub context: u32,
    /// Return the full text of the matched unit.
    pub expand_unit: bool,
    /// Merge all hits of a file into one result.
    pub group_file: bool,
    /// Return only matching file paths.
    pub files_only: bool,
    /// Return per-file hit counts only.
    pub count: bool,
    /// Extract top co-occurring terms across matched units.
    pub terms_top: Option<usize>,
    /// Result ordering.
    pub sort: SortOrder,
    /// Token budget for returned results.
    pub budget_tokens: Option<usize>,
    pub limit: Option<usize>,
}

impl Query {
    pub fn new(expr: Expr) -> Self {
        Query {
            expr,
            unit: Unit::File,
            filters: Filters::default(),
            context: 0,
            expand_unit: false,
            group_file: false,
            files_only: false,
            count: false,
            terms_top: None,
            sort: SortOrder::Path,
            budget_tokens: None,
            limit: None,
        }
    }

    /// Bring the query into canonical form: flattened, sorted, deduplicated.
    /// `parse(q.to_string()) == q` holds for every normalized query.
    pub fn normalize(&mut self) {
        let e = std::mem::replace(&mut self.expr, Expr::And { args: vec![] });
        self.expr = e.normalize();
        self.filters.types.sort();
        self.filters.types.dedup();
        self.filters.exts.sort();
        self.filters.exts.dedup();
        self.filters.paths.sort();
        self.filters.paths.dedup();
        if self.unit == Unit::File {
            self.group_file = false; // no-op on file units
        }
    }
}

impl Expr {
    pub fn lit(s: impl Into<String>) -> Expr {
        Expr::Match {
            m: Matcher::Lit { text: s.into() },
        }
    }

    pub fn normalize(self) -> Expr {
        match self {
            Expr::And { args } => {
                let mut out = Vec::new();
                for a in args {
                    match a.normalize() {
                        Expr::And { args: inner } => out.extend(inner),
                        x => out.push(x),
                    }
                }
                finish(out, true)
            }
            Expr::Or { args } => {
                let mut out = Vec::new();
                for a in args {
                    match a.normalize() {
                        Expr::Or { args: inner } => out.extend(inner),
                        x => out.push(x),
                    }
                }
                finish(out, false)
            }
            Expr::Not { arg } => match arg.normalize() {
                Expr::Not { arg: inner } => *inner,
                x => Expr::Not { arg: Box::new(x) },
            },
            x => x,
        }
    }
}

fn finish(mut v: Vec<Expr>, and: bool) -> Expr {
    v.sort_by_cached_key(|e| e.to_string());
    v.dedup();
    if v.len() == 1 {
        v.pop().unwrap()
    } else if and {
        Expr::And { args: v }
    } else {
        Expr::Or { args: v }
    }
}

/// Quote a string so that the lexer reads it back verbatim. Backslashes are
/// only escaped where needed, so regexes stay readable (`"\s+"`).
pub fn quote(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => {
                let next = chars.get(i + 1);
                if matches!(next, None | Some('\\') | Some('"')) {
                    out.push_str("\\\\");
                } else {
                    out.push('\\');
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

impl fmt::Display for Matcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Matcher::Lit { text } => f.write_str(&quote(text)),
            Matcher::Regex { pattern } => write!(f, "regex({})", quote(pattern)),
            Matcher::Fuzzy { text, k } => write!(f, "fuzzy({}, {k})", quote(text)),
            Matcher::Symbol { name } => write!(f, "symbol({})", quote(name)),
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_expr(self, f, 0)
    }
}

// Precedence: OR = 1, AND = 2, NOT/atoms = 3.
fn write_expr(e: &Expr, f: &mut fmt::Formatter<'_>, prec: u8) -> fmt::Result {
    match e {
        Expr::Match { m } => write!(f, "{m}"),
        Expr::Near {
            a,
            b,
            dist,
            ordered,
        } => {
            write!(
                f,
                "{}({a}, {b}, {dist})",
                if *ordered { "before" } else { "near" }
            )
        }
        Expr::Not { arg } => {
            f.write_str("NOT ")?;
            write_expr(arg, f, 3)
        }
        Expr::And { args } => write_join(args, " AND ", 2, prec, f),
        Expr::Or { args } => write_join(args, " OR ", 1, prec, f),
    }
}

fn write_join(
    args: &[Expr],
    sep: &str,
    my: u8,
    prec: u8,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let paren = prec > my;
    if paren {
        f.write_str("(")?;
    }
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            f.write_str(sep)?;
        }
        write_expr(a, f, my + 1)?;
    }
    if paren {
        f.write_str(")")?;
    }
    Ok(())
}

fn write_list(f: &mut fmt::Formatter<'_>, key: &str, vals: &[String]) -> fmt::Result {
    match vals.len() {
        0 => Ok(()),
        1 => write!(f, " in {key}:{}", vals[0]),
        _ => write!(f, " in {key}:({})", vals.join(", ")),
    }
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "find {}", self.expr)?;
        if self.unit != Unit::File {
            write!(f, " in {}", self.unit.name())?;
        }
        let types: Vec<String> = self
            .filters
            .types
            .iter()
            .map(|t| t.name().to_string())
            .collect();
        write_list(f, "type", &types)?;
        write_list(f, "ext", &self.filters.exts)?;
        let paths: Vec<String> = self.filters.paths.iter().map(|p| quote(p)).collect();
        write_list(f, "path", &paths)?;
        if self.expand_unit {
            f.write_str(" | expand parent")?;
        }
        if self.context > 0 {
            write!(f, " | expand context:{}", self.context)?;
        }
        if self.group_file {
            f.write_str(" | group file")?;
        }
        if self.files_only {
            f.write_str(" | files")?;
        }
        if self.count {
            f.write_str(" | count")?;
        }
        if let Some(top) = self.terms_top {
            write!(f, " | terms top:{top}")?;
        }
        match self.sort {
            SortOrder::Path => {}
            SortOrder::CountDesc => f.write_str(" | sort count")?,
            SortOrder::CountAsc => f.write_str(" | sort count asc")?,
        }
        if let Some(b) = self.budget_tokens {
            write!(f, " | budget tokens:{b}")?;
        }
        if let Some(n) = self.limit {
            write!(f, " | limit {n}")?;
        }
        Ok(())
    }
}
