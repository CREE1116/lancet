//! `lancet rename` — tier 1: rename a *local* binding (a variable, parameter,
//! closure or loop variable declared inside a function) and every use of it, in
//! one file, without a type checker and without guessing.
//!
//! The binding is picked by position (`FILE:LINE[:COL] OLD`), never by name, and a
//! per-language scope resolver links each use to its declaration. Anything the
//! resolver cannot prove is refused rather than approximated:
//!
//! * the binding must be local (module-level names, fields, methods and exported
//!   symbols can be used from other files: that needs a compiler-backed tier);
//! * every occurrence of the old *and* the new name inside the binding's scope must
//!   be understood — one inside a macro, a JSX tag name, a Go composite-literal key,
//!   `eval`/`with`/`locals()`, or a `global`/`nonlocal` statement stops the rename;
//! * after renaming, the file is resolved again and every use of every name must
//!   point at the same declaration as before. That is what catches shadowing and
//!   capture (renaming `x` to a `y` that is already visible somewhere it is used).
//!
//! Writes go through the same locked compare-and-swap as `lancet edit`.

use crate::output::{ErrorInfo, Status};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct RenameOptions {
    pub path: PathBuf,
    /// 1-based line of an occurrence (the declaration or any use).
    pub line: usize,
    /// 1-based byte column, as `lancet rg --column` prints it. Optional when the
    /// line has only one binding named `old`.
    pub col: Option<usize>,
    pub old: String,
    pub new: String,
    pub rev: Option<String>,
    pub apply: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Site {
    pub line: usize,
    pub col: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct RenameResponse {
    pub status: Status,
    pub mode: &'static str,
    pub path: String,
    pub old: String,
    pub new: String,
    /// How the binding was resolved (`local-scope` for this tier).
    pub tier: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub declaration: Option<Site>,
    /// Every renamed occurrence (declarations and uses), in file order.
    pub sites: Vec<Site>,
    /// Comments in the binding's scope that mention the old name (left as is).
    pub comment_mentions: usize,
    pub applied: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub diff: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub elapsed_ms: f64,
}

pub fn rename(o: &RenameOptions) -> RenameResponse {
    let t0 = Instant::now();
    let mut r = RenameResponse {
        status: Status::Complete,
        mode: if o.apply { "applied" } else { "dry-run" },
        path: o.path.to_string_lossy().into_owned(),
        old: o.old.clone(),
        new: o.new.clone(),
        tier: "local-scope",
        rev: None,
        rev_after: None,
        declaration: None,
        sites: Vec::new(),
        comment_mentions: 0,
        applied: false,
        diff: String::new(),
        error: None,
        elapsed_ms: 0.0,
    };
    if let Err((status, msg)) = imp::run(o, &mut r) {
        r.status = status;
        r.rev_after = None;
        r.error = Some(ErrorInfo {
            message: msg,
            pos: None,
        });
    }
    r.elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    r
}

pub use imp::{local_name_starts, local_name_starts_in};

#[cfg(not(feature = "ast"))]
mod imp {
    use super::*;
    pub fn local_name_starts(_path: &std::path::Path, _src: &str) -> Option<Vec<usize>> {
        None
    }
    pub fn local_name_starts_in(_syn: &crate::code::Syntax, _src: &str) -> Option<Vec<usize>> {
        None
    }
    pub fn run(_o: &RenameOptions, _r: &mut RenameResponse) -> Result<(), (Status, String)> {
        Err((
            Status::Unsupported,
            "lancet was built without the `ast` feature".into(),
        ))
    }
}

#[cfg(feature = "ast")]
mod imp {
    use super::*;
    use crate::code::Lang;
    use crate::edit::{self, Edit};
    use std::collections::HashMap;
    use tree_sitter::Node;

    type Fail = (Status, String);
    type Span = (usize, usize);

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum L {
        Rust,
        Python,
        Js,
        Go,
    }

    fn family(lang: Lang) -> Option<L> {
        match lang {
            Lang::Rust => Some(L::Rust),
            Lang::Python => Some(L::Python),
            Lang::Js | Lang::Ts | Lang::Tsx => Some(L::Js),
            Lang::Go => Some(L::Go),
            _ => None,
        }
    }

    /// A binding: where it is written and where uses of its name see it.
    #[derive(Clone, Debug)]
    struct Decl {
        name: String,
        /// Identifier spans that bind it (Python may bind one local several times).
        sites: Vec<Span>,
        /// Which of `sites` are shorthand (`S { x }`, `{x}`): renamed as `x: new`.
        shorthand: Vec<bool>,
        /// Uses at byte `p` see this binding when `from <= p < to`, outside `holes`.
        from: usize,
        to: usize,
        holes: Vec<Span>,
        /// Declared inside a function (so no other file can use it).
        local: bool,
        /// Why renaming this binding is refused even though it is local.
        refuse: Option<&'static str>,
    }

    #[derive(Clone, Debug)]
    struct Use {
        name: String,
        span: Span,
        shorthand: bool,
    }

    #[derive(Default, Debug)]
    struct Analysis {
        decls: Vec<Decl>,
        uses: Vec<Use>,
        /// Identifier start → why its meaning can't be determined (macro, JSX tag…).
        opaque: HashMap<usize, &'static str>,
        /// Identifier starts that are names in another namespace (fields, types…).
        other: Vec<usize>,
        /// Byte ranges where names are resolved at run time (`eval`, `with`, `locals()`).
        dynamic: Vec<(Span, &'static str)>,
        /// Names declared `global` / `nonlocal` somewhere in the file.
        rebound: Vec<String>,
        /// String literals inside std formatting macros lancet resolved: plain text
        /// (the format string's `{name}` captures are in `uses`).
        literal: Vec<Span>,
        by_name: HashMap<String, Vec<usize>>,
    }

    impl Analysis {
        fn add_decl(&mut self, d: Decl) -> usize {
            let i = self.decls.len();
            self.by_name.entry(d.name.clone()).or_default().push(i);
            self.decls.push(d);
            i
        }

        /// The binding a use of `name` at byte `p` refers to: the innermost one in
        /// view (the latest `from`), or `None` for a free name (global, import, builtin).
        fn resolve(&self, name: &str, p: usize) -> Option<usize> {
            self.by_name
                .get(name)?
                .iter()
                .copied()
                .filter(|&i| {
                    let d = &self.decls[i];
                    d.from <= p && p < d.to && !d.holes.iter().any(|h| h.0 <= p && p < h.1)
                })
                .max_by_key(|&i| (self.decls[i].from, i))
        }
    }

    pub fn run(o: &RenameOptions, r: &mut RenameResponse) -> Result<(), Fail> {
        let invalid = |m: String| (Status::InvalidQuery, m);
        let unsupported = |m: String| (Status::Unsupported, m);
        let lang = Lang::from_path(&o.path)
            .ok_or_else(|| unsupported(format!("{}: not a supported source file", r.path)))?;
        let fam = family(lang).ok_or_else(|| {
            unsupported(format!(
                "rename supports Rust, Python, JavaScript, TypeScript and Go so far (not {})",
                lang.name()
            ))
        })?;
        if o.old == o.new {
            return Err(invalid("the old and new names are the same".into()));
        }
        check_new_name(fam, &o.new).map_err(invalid)?;
        if fam == L::Rust && [&o.old, &o.new].iter().any(|n| starts_upper(n)) {
            return Err(unsupported(
                "Rust names starting with an uppercase letter can be constants or enum variants in patterns; rename only covers snake_case locals".into(),
            ));
        }

        let bytes = edit::read_shared(&o.path).map_err(|e| invalid(format!("read error: {e}")))?;
        let src =
            String::from_utf8(bytes).map_err(|_| unsupported("file is not valid UTF-8".into()))?;
        let rev = edit::rev(src.as_bytes());
        r.rev = Some(rev.clone());
        if let Some(want) = &o.rev {
            if *want != rev {
                return Err((
                    Status::Conflict,
                    format!(
                        "file changed since rev {want} (now {rev}); read it again before renaming"
                    ),
                ));
            }
        }
        let before = analyze(lang, fam, &src)?;
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(memchr::memchr_iter(b'\n', src.as_bytes()).map(|i| i + 1))
            .collect();
        let site_of = |p: usize| {
            let l = line_starts.partition_point(|&s| s <= p);
            Site {
                line: l,
                col: p - line_starts[l - 1] + 1,
            }
        };

        // 1. The binding at the given position.
        let target = pick_binding(&before, &src, &line_starts, o)?;
        let d = &before.decls[target];
        let decl_site = site_of(d.sites[0].0);
        if !d.local {
            return Err(unsupported(format!(
                "`{}` is declared at {}:{} outside any function, so other files may use it; renaming it needs the compiler-backed tier (not available yet)",
                o.old, decl_site.line, decl_site.col
            )));
        }
        if let Some(why) = d.refuse {
            return Err(unsupported(format!(
                "`{}` ({}:{}): {why}",
                o.old, decl_site.line, decl_site.col
            )));
        }
        for name in [&o.old, &o.new] {
            if before.rebound.iter().any(|n| n == name) {
                return Err(unsupported(format!(
                    "`{name}` appears in a `global`/`nonlocal` statement in this file; not renamed"
                )));
            }
        }
        let region = (d.sites.iter().map(|s| s.0).min().unwrap().min(d.from), d.to);
        if let Some(((a, _), why)) = before
            .dynamic
            .iter()
            .find(|((a, b), _)| *a < region.1 && region.0 < *b)
        {
            let s = site_of(*a);
            return Err(unsupported(format!(
                "the scope of `{}` contains {why} at {}:{}, which can look names up at run time; not renamed",
                o.old, s.line, s.col
            )));
        }

        // 2. Every mention of either name in that scope must be understood.
        let mentions = scan_mentions(lang, &src, region, &[&o.old, &o.new], &before)?;
        if let Some((p, name, why)) = mentions.blocked.first() {
            let s = site_of(*p);
            return Err(unsupported(format!(
                "`{name}` at {}:{} is {why}; lancet can't tell what it refers to, so nothing was renamed",
                s.line, s.col
            )));
        }
        r.comment_mentions = mentions.comments;

        // 3. Edits: every site of the binding and every use that resolves to it.
        let mut edits: Vec<(Span, bool)> = d
            .sites
            .iter()
            .zip(&d.shorthand)
            .map(|(s, sh)| (*s, *sh))
            .collect();
        for u in &before.uses {
            if u.name == o.old && before.resolve(&u.name, u.span.0) == Some(target) {
                edits.push((u.span, u.shorthand));
            }
        }
        edits.sort();
        edits.dedup();
        let as_edit = |(s, sh): &(Span, bool)| Edit {
            start: s.0,
            end: s.1,
            replacement: if *sh {
                format!("{}: {}", o.old, o.new)
            } else {
                o.new.clone()
            },
        };
        let new_src = edit::plan(&src, &edits.iter().map(as_edit).collect::<Vec<_>>()).new_src;

        // 4. Resolve the result again: every use must mean what it meant before.
        let after = analyze(lang, fam, &new_src)?;
        verify(&before, &after, target, &edits, o, &site_of)?;
        let (e0, e1) = (
            crate::code::syntax_errors(&o.path, src.as_bytes()),
            crate::code::syntax_errors(&o.path, new_src.as_bytes()),
        );
        if let (Some(a), Some(b)) = (e0, e1) {
            if b > a {
                return Err(invalid(format!(
                    "renaming would add {} syntax error(s); nothing written",
                    b - a
                )));
            }
        }

        r.declaration = Some(decl_site);
        r.sites = edits.iter().map(|(s, _)| site_of(s.0)).collect();
        r.diff = edit::unified_diff(&r.path, &src, &new_src);
        r.rev_after = Some(edit::rev(new_src.as_bytes()));
        if o.apply {
            match edit::write_if_rev(&o.path, &rev, new_src.as_bytes()) {
                Ok(()) => r.applied = true,
                Err(edit::WriteError::Conflict { now }) => {
                    return Err((
                        Status::Conflict,
                        format!("file changed while renaming (now rev {now}); nothing written"),
                    ))
                }
                Err(edit::WriteError::Io(e)) => return Err(invalid(format!("write failed: {e}"))),
            }
        }
        Ok(())
    }

    fn starts_upper(s: &str) -> bool {
        s.chars().next().is_some_and(char::is_uppercase)
    }

    fn is_ident_char(fam: L, c: char) -> bool {
        c.is_alphanumeric() || c == '_' || (fam == L::Js && c == '$')
    }

    fn check_new_name(fam: L, n: &str) -> Result<(), String> {
        let mut cs = n.chars();
        let ok_first = cs
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || (fam == L::Js && c == '$'));
        if !ok_first || !cs.all(|c| is_ident_char(fam, c)) || n == "_" {
            return Err(format!("`{n}` is not a valid identifier"));
        }
        let kw: &[&str] = match fam {
            L::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
                "enum", "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop",
                "match", "mod", "move", "mut", "pub", "ref", "return", "self", "static", "struct",
                "super", "trait", "true", "try", "type", "union", "unsafe", "use", "where",
                "while", "yield", "abstract", "become", "box", "do", "final", "macro", "override",
                "priv", "typeof", "unsized", "virtual",
            ],
            L::Python => &[
                "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
                "continue", "def", "del", "elif", "else", "except", "finally", "for", "from",
                "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass",
                "raise", "return", "try", "while", "with", "yield",
            ],
            L::Js => &[
                "arguments",
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "debugger",
                "default",
                "delete",
                "do",
                "else",
                "enum",
                "eval",
                "export",
                "extends",
                "false",
                "finally",
                "for",
                "function",
                "if",
                "implements",
                "import",
                "in",
                "instanceof",
                "interface",
                "let",
                "new",
                "null",
                "package",
                "private",
                "protected",
                "public",
                "return",
                "static",
                "super",
                "switch",
                "this",
                "throw",
                "true",
                "try",
                "typeof",
                "undefined",
                "var",
                "void",
                "while",
                "with",
                "yield",
            ],
            L::Go => &[
                "break",
                "case",
                "chan",
                "const",
                "continue",
                "default",
                "defer",
                "else",
                "fallthrough",
                "for",
                "func",
                "go",
                "goto",
                "if",
                "import",
                "interface",
                "map",
                "package",
                "range",
                "return",
                "select",
                "struct",
                "switch",
                "type",
                "var",
            ],
        };
        if kw.contains(&n) {
            return Err(format!("`{n}` is a reserved word"));
        }
        Ok(())
    }

    /// The binding the user pointed at: the identifier at `line:col`, or the only
    /// binding named `old` on `line`.
    fn pick_binding(
        a: &Analysis,
        src: &str,
        line_starts: &[usize],
        o: &RenameOptions,
    ) -> Result<usize, Fail> {
        let invalid = |m: String| (Status::InvalidQuery, m);
        let ls = *line_starts
            .get(o.line.wrapping_sub(1))
            .ok_or_else(|| invalid(format!("line {} is outside the file", o.line)))?;
        let le = line_starts.get(o.line).copied().unwrap_or(src.len());
        // (span, binding or None for a free name) of every occurrence named `old` on the line.
        let mut found: Vec<(Span, Option<usize>)> = Vec::new();
        for (i, d) in a.decls.iter().enumerate() {
            for s in &d.sites {
                if d.name == o.old && ls <= s.0 && s.0 < le {
                    found.push((*s, Some(i)));
                }
            }
        }
        for u in &a.uses {
            if u.name == o.old && ls <= u.span.0 && u.span.0 < le {
                found.push((u.span, a.resolve(&u.name, u.span.0)));
            }
        }
        if let Some(col) = o.col {
            let p = ls + col.saturating_sub(1);
            found.retain(|(s, _)| s.0 <= p && p < s.1);
            if found.is_empty() {
                let what = if a
                    .opaque
                    .keys()
                    .any(|&k| ls <= k && k < le && src[k..].starts_with(&o.old))
                {
                    " (it is inside a macro or another construct lancet can't resolve)"
                } else {
                    ""
                };
                return Err(invalid(format!(
                    "no variable `{}` at {}:{col}{what}",
                    o.old, o.line
                )));
            }
        }
        let mut targets: Vec<Option<usize>> = found.iter().map(|f| f.1).collect();
        targets.sort();
        targets.dedup();
        match targets.as_slice() {
            [] => Err(invalid(format!(
                "no variable `{}` on line {} (fields, types, methods and names in comments or strings are not renamed by this tier)",
                o.old, o.line
            ))),
            [None] => Err((
                Status::Unsupported,
                format!(
                    "`{}` on line {} is not declared in this file (a global, import or builtin); renaming it needs the compiler-backed tier (not available yet)",
                    o.old, o.line
                ),
            )),
            [Some(t)] => Ok(*t),
            _ => {
                let cols: Vec<String> = found
                    .iter()
                    .map(|(s, _)| (s.0 - ls + 1).to_string())
                    .collect();
                Err(invalid(format!(
                    "line {} has more than one `{}` binding (columns {}); add the column: FILE:{}:COL",
                    o.line,
                    o.old,
                    cols.join(", "),
                    o.line
                )))
            }
        }
    }

    struct Mentions {
        blocked: Vec<(usize, String, &'static str)>,
        comments: usize,
    }

    /// Every word-boundary occurrence of `names` in `region` must be an identifier the
    /// analysis classified, or sit in a comment or a plain string literal.
    fn scan_mentions(
        lang: Lang,
        src: &str,
        region: Span,
        names: &[&String],
        a: &Analysis,
    ) -> Result<Mentions, Fail> {
        let fam = family(lang).unwrap();
        let tree = parse(lang, src)?;
        let root = tree.root_node();
        let known: std::collections::HashSet<usize> = a
            .decls
            .iter()
            .flat_map(|d| d.sites.iter().map(|s| s.0))
            .chain(a.uses.iter().map(|u| u.span.0))
            .chain(a.other.iter().copied())
            .collect();
        let mut m = Mentions {
            blocked: Vec::new(),
            comments: 0,
        };
        let hay = &src[region.0..region.1.min(src.len())];
        for name in names {
            for (off, _) in hay.match_indices(name.as_str()) {
                let p = region.0 + off;
                let q = p + name.len();
                let before_ok = src[..p]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !is_ident_char(fam, c));
                let after_ok = src[q..]
                    .chars()
                    .next()
                    .is_none_or(|c| !is_ident_char(fam, c));
                if !before_ok || !after_ok {
                    continue;
                }
                if let Some(why) = a.opaque.get(&p) {
                    m.blocked.push((p, name.to_string(), why));
                    continue;
                }
                if known.contains(&p) {
                    continue;
                }
                let Some(n) = root.descendant_for_byte_range(p, q) else {
                    continue;
                };
                let in_kind = |pred: &dyn Fn(&str) -> bool| {
                    let mut cur = Some(n);
                    while let Some(c) = cur {
                        if pred(c.kind()) {
                            return true;
                        }
                        cur = c.parent();
                    }
                    false
                };
                if in_kind(&|k| k.contains("comment")) {
                    if **name == *names[0] {
                        m.comments += 1;
                    }
                } else if a.literal.iter().any(|s| s.0 <= p && p < s.1) {
                    // Literal text inside `format!`-style arguments.
                } else if fam == L::Rust
                    && in_kind(&|k| k == "token_tree" || k == "macro_definition")
                {
                    m.blocked.push((p, name.to_string(), "inside a macro"));
                } else if in_kind(&|k| {
                    k.contains("string") || k == "char_literal" || k == "rune_literal"
                }) {
                    // Literal text, not code (f-string / template interpolations are
                    // parsed as expressions and already classified).
                } else {
                    m.blocked.push((
                        p,
                        name.to_string(),
                        "in a construct this tier does not model",
                    ));
                }
            }
        }
        m.blocked.sort();
        Ok(m)
    }

    fn verify(
        before: &Analysis,
        after: &Analysis,
        target: usize,
        edits: &[(Span, bool)],
        o: &RenameOptions,
        site_of: &dyn Fn(usize) -> Site,
    ) -> Result<(), Fail> {
        // Old byte → new byte, for positions outside the edited identifiers.
        let shift = |p: usize| -> usize {
            let mut q = p as isize;
            for ((s, e), sh) in edits {
                if *e <= p {
                    let rep = if *sh {
                        o.old.len() + 2 + o.new.len()
                    } else {
                        o.new.len()
                    };
                    q += rep as isize - (e - s) as isize;
                }
            }
            q as usize
        };
        // Where a renamed identifier lands: after `old: ` for shorthand sites.
        let moved = |p: usize| -> usize {
            let sh = edits.iter().any(|((s, _), sh)| *s == p && *sh);
            shift(p) + if sh { o.old.len() + 2 } else { 0 }
        };
        let renamed: std::collections::HashSet<usize> = edits.iter().map(|(s, _)| s.0).collect();
        let anchor = |a: &Analysis, i: usize| a.decls[i].sites[0].0;
        let mut after_uses: HashMap<usize, (&str, Option<usize>)> = HashMap::new();
        for u in &after.uses {
            after_uses.insert(
                u.span.0,
                (
                    &u.name,
                    after.resolve(&u.name, u.span.0).map(|i| anchor(after, i)),
                ),
            );
        }
        let changed = |p: usize, name: &str| {
            let s = site_of(p);
            (
                Status::Conflict,
                format!(
                    "renaming would change what `{name}` at {}:{} refers to (shadowing or capture); nothing written — pick another name",
                    s.line, s.col
                ),
            )
        };
        if after.uses.len() != before.uses.len() {
            return Err((
                Status::Conflict,
                "renaming would change how the file's names are bound; nothing written".into(),
            ));
        }
        for u in &before.uses {
            let was = before.resolve(&u.name, u.span.0);
            let is_ours = was == Some(target);
            let at = if renamed.contains(&u.span.0) {
                moved(u.span.0)
            } else {
                shift(u.span.0)
            };
            let want_name = if is_ours {
                o.new.as_str()
            } else {
                u.name.as_str()
            };
            let want_decl = was.map(|i| {
                let p = anchor(before, i);
                if i == target {
                    moved(p)
                } else {
                    shift(p)
                }
            });
            match after_uses.get(&at) {
                Some((n, d)) if *n == want_name && *d == want_decl => {}
                _ => return Err(changed(u.span.0, &u.name)),
            }
        }
        Ok(())
    }

    fn parse(lang: Lang, src: &str) -> Result<tree_sitter::Tree, Fail> {
        crate::code::parse_tree(lang, src.as_bytes())
            .ok_or_else(|| (Status::InvalidQuery, "parse failed".into()))
    }

    /// Sorted starts of the identifiers that are, or resolve to, a binding inside a
    /// function (a `let`, a parameter, a nested item): a search for a top-level name
    /// can skip them. `None` when the language isn't modeled or names are resolved
    /// at run time (`eval`, `with`, `locals()`).
    pub fn local_name_starts(path: &std::path::Path, src: &str) -> Option<Vec<usize>> {
        local_name_starts_in(&crate::code::parse(path, src.as_bytes())?, src)
    }

    /// [`local_name_starts`] for a file already parsed.
    pub fn local_name_starts_in(syn: &crate::code::Syntax, src: &str) -> Option<Vec<usize>> {
        let a = analyze_tree(family(syn.lang)?, src, &syn.tree);
        if !a.dynamic.is_empty() {
            return None;
        }
        let local = |i: usize| a.decls[i].local && !a.rebound.contains(&a.decls[i].name);
        let mut out: Vec<usize> = (0..a.decls.len())
            .filter(|&i| local(i))
            .flat_map(|i| a.decls[i].sites.iter().map(|s| s.0))
            .chain(
                a.uses
                    .iter()
                    .filter(|u| a.resolve(&u.name, u.span.0).is_some_and(local))
                    .map(|u| u.span.0),
            )
            .collect();
        out.sort_unstable();
        Some(out)
    }

    fn analyze(lang: Lang, fam: L, src: &str) -> Result<Analysis, Fail> {
        Ok(analyze_tree(fam, src, &parse(lang, src)?))
    }

    fn analyze_tree(fam: L, src: &str, tree: &tree_sitter::Tree) -> Analysis {
        let mut a = Analysis::default();
        let mut cx = Cx {
            fam,
            src,
            go_scopes: HashMap::new(),
            py_locals: HashMap::new(),
            fmt_uses: Default::default(),
            fmt_other: Default::default(),
        };
        let mut stack = vec![tree.root_node()];
        while let Some(n) = stack.pop() {
            cx.visit(n, &mut a);
            let mut c = n.walk();
            let kids: Vec<Node> = n.children(&mut c).collect();
            stack.extend(kids.into_iter().rev());
        }
        a
    }

    struct Cx<'s> {
        fam: L,
        src: &'s str,
        /// Go: (scope span, name) → binding, to tell `:=` redeclaration from assignment.
        go_scopes: HashMap<(Span, String), usize>,
        /// Python: (function span, name) → the one local binding of that name.
        py_locals: HashMap<(Span, String), usize>,
        /// Rust: identifier starts inside std formatting macros that are plain uses /
        /// names in another namespace (`.field`, `path::`, `name =`).
        fmt_uses: std::collections::HashSet<usize>,
        fmt_other: std::collections::HashSet<usize>,
    }

    /// Names a Rust format string captures: `{name}`, `{name:?}`, `{:width$}`, with
    /// their byte offsets in `s`.
    fn format_captures(s: &str) -> Vec<(usize, &str)> {
        let ident = |t: &str| {
            t.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
                && t != "_"
                && t.chars().all(|c| c.is_alphanumeric() || c == '_')
        };
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(j) = s[i..].find(['{', '}']).map(|j| i + j) {
            if s[j..].starts_with("{{") || s[j..].starts_with("}}") || s.as_bytes()[j] == b'}' {
                i = j + if s.as_bytes().get(j + 1) == s.as_bytes().get(j) {
                    2
                } else {
                    1
                };
                continue;
            }
            let Some(end) = s[j..].find('}').map(|e| j + e) else {
                break;
            };
            let inner = &s[j + 1..end];
            let (arg, spec) = inner.split_once(':').unwrap_or((inner, ""));
            if ident(arg) {
                out.push((j + 1, arg));
            }
            let spec_at = j + 1 + arg.len() + 1;
            for (k, _) in spec.match_indices('$') {
                let start = spec[..k]
                    .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .map_or(0, |b| b + 1);
                if ident(&spec[start..k]) {
                    out.push((spec_at + start, &spec[start..k]));
                }
            }
            i = end + 1;
        }
        out
    }

    fn span(n: Node) -> Span {
        (n.start_byte(), n.end_byte())
    }

    fn field_is(n: Node, parent: Node, field: &str) -> bool {
        parent
            .child_by_field_name(field)
            .is_some_and(|c| c.id() == n.id())
    }

    fn ancestor<'t>(n: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
        let mut cur = n.parent();
        while let Some(c) = cur {
            if kinds.contains(&c.kind()) {
                return Some(c);
            }
            cur = c.parent();
        }
        None
    }

    impl Cx<'_> {
        fn text(&self, n: Node) -> &str {
            &self.src[n.byte_range()]
        }

        fn decl(&self, n: Node, from: usize, to: usize, local: bool) -> Decl {
            Decl {
                name: self.text(n).to_string(),
                sites: vec![span(n)],
                shorthand: vec![false],
                from,
                to,
                holes: Vec::new(),
                local,
                refuse: None,
            }
        }

        fn use_(&self, a: &mut Analysis, n: Node, shorthand: bool) {
            a.uses.push(Use {
                name: self.text(n).to_string(),
                span: span(n),
                shorthand,
            });
        }

        fn visit(&mut self, n: Node, a: &mut Analysis) {
            match self.fam {
                L::Rust => self.rust(n, a),
                L::Python => self.python(n, a),
                L::Js => self.js(n, a),
                L::Go => self.go(n, a),
            }
        }

        // ---------------------------------------------------------------- Rust
        fn rust(&mut self, n: Node, a: &mut Analysis) {
            let k = n.kind();
            if k == "macro_invocation" {
                self.rust_fmt_macro(n, a);
                return;
            }
            if k != "identifier" && k != "shorthand_field_identifier" {
                if matches!(k, "field_identifier" | "type_identifier") {
                    a.other.push(n.start_byte());
                }
                return;
            }
            let Some(p) = n.parent() else { return };
            if ancestor(n, &["token_tree", "macro_definition"]).is_some() {
                let s = n.start_byte();
                if self.fmt_uses.contains(&s) {
                    self.use_(a, n, false);
                } else if self.fmt_other.contains(&s) {
                    a.other.push(s);
                } else {
                    a.opaque.insert(s, "inside a macro");
                }
                return;
            }
            if ancestor(n, &["use_declaration"]).is_some() {
                a.opaque.insert(n.start_byte(), "in a `use` declaration");
                return;
            }
            match p.kind() {
                "scoped_identifier"
                | "scoped_type_identifier"
                | "macro_invocation"
                | "attribute"
                | "meta_item"
                | "token_binding_pattern"
                | "visibility_modifier"
                | "mod_item" => {
                    a.other.push(n.start_byte());
                    return;
                }
                "function_item" | "const_item" | "static_item" if field_is(n, p, "name") => {
                    // Items are visible in their whole block (or module).
                    let c = p.parent().unwrap_or(p);
                    let local = c.kind() == "block";
                    a.add_decl(self.decl(n, c.start_byte(), c.end_byte(), local));
                    return;
                }
                "const_parameter" | "type_parameter" | "lifetime" | "label" => {
                    a.opaque.insert(n.start_byte(), "a generic parameter");
                    return;
                }
                "tuple_struct_pattern" if field_is(n, p, "type") => {
                    a.other.push(n.start_byte());
                    return;
                }
                "shorthand_field_initializer" => {
                    self.use_(a, n, true);
                    return;
                }
                _ => {}
            }
            if let Some((from, to)) = self.rust_binder(n) {
                if starts_upper(self.text(n)) {
                    // `None`, `MAX`: a path, not a new binding.
                    a.other.push(n.start_byte());
                    return;
                }
                let mut d = self.decl(n, from, to, true);
                if k == "shorthand_field_identifier" {
                    d.shorthand = vec![true];
                    if p.kind() == "field_pattern" && p.child(0).is_some_and(|c| c.id() != n.id()) {
                        d.refuse =
                            Some("a `ref`/`mut` struct-pattern shorthand can't be expanded safely");
                    }
                }
                a.add_decl(d);
                return;
            }
            if k == "identifier" {
                self.use_(a, n, false);
            }
        }

        /// `format!`, `println!`, `write!`, `assert_eq!`… take plain expressions that are
        /// evaluated where the macro is called, so their names resolve like any other
        /// code, and `{name}` in the format string uses the local `name`. Records those
        /// uses when nothing in the call can bind or hide a name (a closure, a block, a
        /// nested macro); otherwise the call stays opaque.
        fn rust_fmt_macro(&mut self, m: Node, a: &mut Analysis) {
            if ancestor(m, &["token_tree", "macro_definition"]).is_some() {
                return;
            }
            let Some(name) = m.child_by_field_name("macro") else {
                return;
            };
            let name = match name.kind() {
                "identifier" => self.text(name),
                "scoped_identifier" => match self.text(name).rsplit_once("::") {
                    Some((p, last))
                        if matches!(p.trim_start_matches("::"), "std" | "core" | "alloc") =>
                    {
                        last
                    }
                    _ => return,
                },
                _ => return,
            };
            // Top-level arguments before the format string.
            let fmt_at = match name {
                "format" | "format_args" | "print" | "println" | "eprint" | "eprintln"
                | "panic" | "todo" | "unimplemented" | "unreachable" => Some(0),
                "write" | "writeln" | "assert" | "debug_assert" => Some(1),
                "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" => Some(2),
                // `matches!(expr, pattern)`: the pattern may not bind (checked below).
                "vec" | "dbg" | "matches" => None,
                _ => return,
            };
            let mut c = m.walk();
            let Some(tt) = m.children(&mut c).find(|c| c.kind() == "token_tree") else {
                return;
            };
            let (mut uses, mut other, mut named, mut strings) = (vec![], vec![], vec![], vec![]);
            let mut fmt_lit = None;
            // (token tree, inside `matches!`'s pattern)
            let mut stack = vec![(tt, false)];
            while let Some((t, in_pat)) = stack.pop() {
                let mut c = t.walk();
                let kids: Vec<Node> = t.children(&mut c).collect();
                if kids.first().is_some_and(|k| k.kind() == "{") {
                    return; // a block or struct literal
                }
                let top = t.id() == tt.id();
                let kind = |i: Option<usize>| i.and_then(|i| kids.get(i)).map(|k| k.kind());
                let mut commas = 0;
                for (i, k) in kids.iter().enumerate() {
                    let (prev, next) = (kind(i.checked_sub(1)), kind(Some(i + 1)));
                    let named_arg = top
                        && matches!(kind(i.checked_sub(2)), Some(","))
                        && prev == Some("identifier");
                    match k.kind() {
                        "token_tree" => stack.push((*k, in_pat || name == "matches" && commas > 0)),
                        "," if top => commas += 1,
                        "=" if !named_arg => return,
                        "|" | "||" if matches!(prev, Some("(" | "[" | ",")) => return,
                        "!" if prev == Some("identifier") && next == Some("token_tree") => return,
                        "let" | "match" | "if" | "while" | "for" | "loop" | "fn" | "move"
                        | "async" | "static" | "struct" | "impl" | "mod" | "use" | "const"
                        | "$" => return,
                        "string_literal" | "raw_string_literal" => {
                            strings.push(span(*k));
                            if top
                                && fmt_lit.is_none()
                                && Some(commas) == fmt_at
                                && matches!(prev, Some("(" | "[" | ","))
                                && matches!(next, Some("," | ")" | "]"))
                            {
                                fmt_lit = Some(*k);
                            }
                        }
                        "identifier" => {
                            let s = k.start_byte();
                            let in_pat = in_pat || name == "matches" && top && commas > 0;
                            if in_pat && !starts_upper(self.text(*k)) && next != Some("::") {
                                return; // a pattern binding (or a lowercase const)
                            }
                            if matches!(prev, Some("." | "::")) || next == Some("::") {
                                other.push(s);
                            } else if top && next == Some("=") && matches!(prev, Some(",")) {
                                named.push(self.text(*k));
                                other.push(s);
                            } else {
                                uses.push(s);
                            }
                        }
                        _ => {}
                    }
                }
            }
            if let Some(lit) = fmt_lit {
                let mut c = lit.walk();
                for part in lit
                    .children(&mut c)
                    .filter(|p| p.kind() == "string_content")
                {
                    for (off, cap) in format_captures(self.text(part)) {
                        let s = part.start_byte() + off;
                        if named.contains(&cap) {
                            a.opaque.insert(s, "a named format argument");
                        } else {
                            a.uses.push(Use {
                                name: cap.to_string(),
                                span: (s, s + cap.len()),
                                shorthand: false,
                            });
                        }
                    }
                }
            }
            self.fmt_uses.extend(uses);
            self.fmt_other.extend(other);
            a.literal.extend(strings);
        }

        /// If `n` is bound by a pattern: the byte range that sees it.
        fn rust_binder(&self, n: Node) -> Option<(usize, usize)> {
            const PAT: &[&str] = &[
                "tuple_pattern",
                "slice_pattern",
                "tuple_struct_pattern",
                "struct_pattern",
                "field_pattern",
                "ref_pattern",
                "mut_pattern",
                "reference_pattern",
                "captured_pattern",
                "or_pattern",
                "match_pattern",
                "remaining_field_pattern",
                "parameter",
            ];
            let mut child = n;
            let mut cur = n.parent()?;
            loop {
                match cur.kind() {
                    "let_declaration" if field_is(child, cur, "pattern") => {
                        let block = cur.parent()?;
                        return Some((cur.end_byte(), block.end_byte()));
                    }
                    "parameters" => {
                        let f = cur.parent()?;
                        let body = f.child_by_field_name("body")?;
                        return Some((body.start_byte(), body.end_byte()));
                    }
                    "closure_parameters" => {
                        let f = cur.parent()?;
                        let body = f.child_by_field_name("body")?;
                        return Some((body.start_byte(), body.end_byte()));
                    }
                    "match_arm" if field_is(child, cur, "pattern") => {
                        return Some((child.end_byte(), cur.end_byte()));
                    }
                    "let_condition" if field_is(child, cur, "pattern") => {
                        let host = ancestor(cur, &["if_expression", "while_expression"])?;
                        let body = host
                            .child_by_field_name("consequence")
                            .or_else(|| host.child_by_field_name("body"))?;
                        return Some((cur.end_byte(), body.end_byte()));
                    }
                    "for_expression" if field_is(child, cur, "pattern") => {
                        let body = cur.child_by_field_name("body")?;
                        return Some((body.start_byte(), body.end_byte()));
                    }
                    k if PAT.contains(&k) => {
                        if k == "field_pattern"
                            && field_is(child, cur, "name")
                            && child.kind() == "field_identifier"
                        {
                            return None;
                        }
                        child = cur;
                        cur = cur.parent()?;
                    }
                    _ => return None,
                }
            }
        }

        // -------------------------------------------------------------- Python
        fn python(&mut self, n: Node, a: &mut Analysis) {
            let k = n.kind();
            if k == "call" {
                if let Some(f) = n.child_by_field_name("function") {
                    if matches!(
                        self.text(f),
                        "eval" | "exec" | "locals" | "vars" | "globals"
                    ) {
                        if let Some(func) = ancestor(n, &["function_definition", "lambda"]) {
                            a.dynamic
                                .push((span(func), "a call to eval/exec/locals/vars/globals"));
                        }
                    }
                }
                return;
            }
            if k != "identifier" {
                return;
            }
            let Some(p) = n.parent() else { return };
            match p.kind() {
                "attribute" if field_is(n, p, "attribute") => {
                    a.other.push(n.start_byte());
                    return;
                }
                "keyword_argument" if field_is(n, p, "name") => {
                    a.other.push(n.start_byte());
                    return;
                }
                "global_statement" | "nonlocal_statement" => {
                    a.rebound.push(self.text(n).to_string());
                    a.other.push(n.start_byte());
                    return;
                }
                _ => {}
            }
            if ancestor(
                n,
                &[
                    "import_statement",
                    "import_from_statement",
                    "future_import_statement",
                ],
            )
            .is_some()
            {
                if ancestor(n, &["function_definition", "lambda", "class_definition"]).is_some() {
                    a.opaque
                        .insert(n.start_byte(), "in an import inside a function");
                } else {
                    let s = self.src.len();
                    a.add_decl(self.decl(n, 0, s, false));
                }
                return;
            }
            // Parameters: local to the function body; callers may pass them by keyword.
            if let Some(f) = self.py_param_owner(n) {
                let body = f.child_by_field_name("body").unwrap_or(f);
                self.py_bind(a, n, (body.start_byte(), body.end_byte()), true, Vec::new(), Some("callers can pass a Python parameter by keyword (`f(name=...)`), possibly from other files; renaming it needs the compiler-backed tier"));
                return;
            }
            if (p.kind() == "function_definition" || p.kind() == "class_definition")
                && field_is(n, p, "name")
            {
                let (sc, local, holes) = self.py_scope(p);
                self.py_bind(a, n, sc, local, holes, None);
                return;
            }
            if self.py_is_target(n) {
                let start = if p.kind() == "named_expression" {
                    // `:=` binds in the enclosing function, even inside a comprehension.
                    let mut cur = n;
                    while let Some(c) = ancestor(
                        cur,
                        &[
                            "list_comprehension",
                            "set_comprehension",
                            "dictionary_comprehension",
                            "generator_expression",
                        ],
                    ) {
                        cur = c;
                    }
                    cur
                } else {
                    n
                };
                let (sc, local, holes) = self.py_scope(start);
                self.py_bind(a, n, sc, local, holes, None);
                return;
            }
            self.use_(a, n, false);
        }

        fn py_param_owner<'t>(&self, n: Node<'t>) -> Option<Node<'t>> {
            let mut cur = n.parent()?;
            if matches!(cur.kind(), "default_parameter" | "typed_default_parameter") {
                if !field_is(n, cur, "name") {
                    return None;
                }
                cur = cur.parent()?;
            } else if matches!(
                cur.kind(),
                "typed_parameter" | "list_splat_pattern" | "dictionary_splat_pattern"
            ) {
                if cur.named_child(0).is_none_or(|c| c.id() != n.id()) {
                    return None;
                }
                cur = cur.parent()?;
            }
            if matches!(cur.kind(), "parameters" | "lambda_parameters") {
                return cur.parent();
            }
            None
        }

        fn py_is_target(&self, n: Node) -> bool {
            const PAT: &[&str] = &[
                "pattern_list",
                "tuple_pattern",
                "list_pattern",
                "list_splat_pattern",
                "parenthesized_expression",
                "tuple",
                "list",
                "expression_list",
            ];
            let mut child = n;
            let mut cur = match n.parent() {
                Some(p) => p,
                None => return false,
            };
            loop {
                match cur.kind() {
                    "assignment" | "augmented_assignment" => return field_is(child, cur, "left"),
                    "for_statement" | "for_in_clause" => return field_is(child, cur, "left"),
                    "named_expression" => return field_is(child, cur, "name"),
                    "as_pattern_target" => return true,
                    k if PAT.contains(&k) => {
                        child = cur;
                        match cur.parent() {
                            Some(p) => cur = p,
                            None => return false,
                        }
                    }
                    _ => return false,
                }
            }
        }

        /// The scope a name bound at `n` belongs to: (range, is a function scope, holes).
        fn py_scope(&self, n: Node) -> (Span, bool, Vec<Span>) {
            let mut cur = n.parent();
            while let Some(c) = cur {
                match c.kind() {
                    "function_definition" | "lambda" => {
                        let b = c.child_by_field_name("body").unwrap_or(c);
                        return (span(b), true, Vec::new());
                    }
                    "list_comprehension"
                    | "set_comprehension"
                    | "dictionary_comprehension"
                    | "generator_expression" => {
                        // The first iterable is evaluated in the enclosing scope.
                        let mut holes = Vec::new();
                        let mut w = c.walk();
                        if let Some(f) = c
                            .named_children(&mut w)
                            .find(|x| x.kind() == "for_in_clause")
                        {
                            if let Some(rt) = f.child_by_field_name("right") {
                                holes.push(span(rt));
                            }
                        }
                        return (span(c), true, holes);
                    }
                    "class_definition" => {
                        // Class-body names are not visible inside its methods.
                        let b = c.child_by_field_name("body").unwrap_or(c);
                        let mut holes = Vec::new();
                        let mut st = vec![b];
                        while let Some(x) = st.pop() {
                            let mut w = x.walk();
                            for ch in x.named_children(&mut w) {
                                if matches!(
                                    ch.kind(),
                                    "function_definition"
                                        | "lambda"
                                        | "class_definition"
                                        | "list_comprehension"
                                        | "set_comprehension"
                                        | "dictionary_comprehension"
                                        | "generator_expression"
                                ) {
                                    holes.push(span(ch));
                                } else {
                                    st.push(ch);
                                }
                            }
                        }
                        return (span(b), false, holes);
                    }
                    _ => cur = c.parent(),
                }
            }
            ((0, self.src.len()), false, Vec::new())
        }

        fn py_bind(
            &mut self,
            a: &mut Analysis,
            n: Node,
            sc: Span,
            local: bool,
            holes: Vec<Span>,
            refuse: Option<&'static str>,
        ) {
            let key = (sc, self.text(n).to_string());
            if let Some(&i) = self.py_locals.get(&key) {
                let d = &mut a.decls[i];
                d.sites.push(span(n));
                d.shorthand.push(false);
                d.refuse = d.refuse.or(refuse);
                return;
            }
            let mut d = self.decl(n, sc.0, sc.1, local);
            d.holes = holes;
            d.refuse = refuse;
            let i = a.add_decl(d);
            self.py_locals.insert(key, i);
        }

        // ---------------------------------------------------------- JS / TS
        fn js(&mut self, n: Node, a: &mut Analysis) {
            let k = n.kind();
            match k {
                "with_statement" => {
                    if let Some(f) = ancestor(n, JS_FN) {
                        a.dynamic.push((span(f), "a `with` statement"));
                    } else {
                        a.dynamic.push(((0, self.src.len()), "a `with` statement"));
                    }
                    return;
                }
                "call_expression" => {
                    if n.child_by_field_name("function")
                        .is_some_and(|f| self.text(f) == "eval")
                    {
                        let r = ancestor(n, JS_FN).map(span).unwrap_or((0, self.src.len()));
                        a.dynamic.push((r, "a call to eval"));
                    }
                    return;
                }
                "property_identifier" | "private_property_identifier" | "statement_identifier" => {
                    a.other.push(n.start_byte());
                    return;
                }
                "identifier"
                | "shorthand_property_identifier"
                | "shorthand_property_identifier_pattern"
                | "type_identifier" => {}
                _ => return,
            }
            let Some(p) = n.parent() else { return };
            if matches!(
                p.kind(),
                "jsx_opening_element"
                    | "jsx_closing_element"
                    | "jsx_self_closing_element"
                    | "nested_identifier"
                    | "jsx_namespace_name"
            ) || ancestor(
                n,
                &[
                    "jsx_opening_element",
                    "jsx_closing_element",
                    "jsx_self_closing_element",
                ],
            )
            .is_some_and(|e| {
                e.child_by_field_name("name").is_some_and(|nm| {
                    nm.start_byte() <= n.start_byte() && n.end_byte() <= nm.end_byte()
                })
            }) {
                a.opaque.insert(n.start_byte(), "a JSX tag name");
                return;
            }
            if matches!(
                p.kind(),
                "import_specifier" | "import_clause" | "namespace_import" | "export_specifier"
            ) {
                let s = self.src.len();
                a.add_decl(self.decl(n, 0, s, false));
                return;
            }
            if k == "type_identifier" {
                // TS class names are values too; other type names are a separate namespace.
                if matches!(p.kind(), "class_declaration" | "abstract_class_declaration")
                    && field_is(n, p, "name")
                {
                    let b = ancestor(p, JS_BLOCK)
                        .map(span)
                        .unwrap_or((0, self.src.len()));
                    let local = ancestor(p, JS_FN).is_some();
                    a.add_decl(self.decl(n, b.0, b.1, local));
                } else {
                    a.other.push(n.start_byte());
                }
                return;
            }
            match p.kind() {
                "function_declaration" | "generator_function_declaration" | "class_declaration"
                    if field_is(n, p, "name") =>
                {
                    let b = ancestor(p, JS_BLOCK)
                        .map(span)
                        .unwrap_or((0, self.src.len()));
                    let local = ancestor(p, JS_FN).is_some();
                    a.add_decl(self.decl(n, b.0, b.1, local));
                    return;
                }
                "function_expression" | "function" | "generator_function" | "class"
                    if field_is(n, p, "name") =>
                {
                    let local = ancestor(p, JS_FN).is_some();
                    a.add_decl(self.decl(n, p.start_byte(), p.end_byte(), local));
                    return;
                }
                "arrow_function" if field_is(n, p, "parameter") => {
                    a.add_decl(self.decl(n, p.start_byte(), p.end_byte(), true));
                    return;
                }
                _ => {}
            }
            let shorthand = k != "identifier";
            if let Some((sc, local, refuse)) = self.js_binder(n) {
                let mut d = self.decl(n, sc.0, sc.1, local);
                d.shorthand = vec![shorthand];
                d.refuse = refuse;
                a.add_decl(d);
                return;
            }
            self.use_(a, n, shorthand);
        }

        fn js_binder(&self, n: Node) -> Option<(Span, bool, Option<&'static str>)> {
            const PAT: &[&str] = &[
                "object_pattern",
                "array_pattern",
                "pair_pattern",
                "assignment_pattern",
                "object_assignment_pattern",
                "rest_pattern",
                "required_parameter",
                "optional_parameter",
            ];
            let mut child = n;
            let mut cur = n.parent()?;
            let mut refuse = None;
            loop {
                match cur.kind() {
                    "variable_declarator" => {
                        if !field_is(child, cur, "name") {
                            return None;
                        }
                        let decl = cur.parent()?;
                        let sc = if decl.kind() == "variable_declaration" {
                            ancestor(decl, JS_FN).map(span)
                        } else {
                            ancestor(decl, JS_BLOCK).map(span)
                        };
                        let local = ancestor(decl, JS_FN).is_some();
                        return Some((sc.unwrap_or((0, self.src.len())), local, refuse));
                    }
                    "formal_parameters" => {
                        let f = cur.parent()?;
                        return Some((span(f), true, refuse));
                    }
                    "catch_clause" if field_is(child, cur, "parameter") => {
                        return Some((span(cur), true, refuse));
                    }
                    "for_in_statement" if field_is(child, cur, "left") => {
                        let kind = cur
                            .child_by_field_name("kind")
                            .map(|k| self.text(k).to_string());
                        let local = ancestor(cur, JS_FN).is_some();
                        return match kind.as_deref() {
                            Some("var") => Some((
                                ancestor(cur, JS_FN)
                                    .map(span)
                                    .unwrap_or((0, self.src.len())),
                                local,
                                refuse,
                            )),
                            Some(_) => Some((span(cur), local, refuse)),
                            None => None,
                        };
                    }
                    "pair_pattern" if !field_is(child, cur, "value") => return None,
                    "assignment_pattern" | "object_assignment_pattern"
                        if !field_is(child, cur, "left") =>
                    {
                        return None
                    }
                    "required_parameter" | "optional_parameter" => {
                        if !field_is(child, cur, "pattern") {
                            return None;
                        }
                        let mut w = cur.walk();
                        if cur.children(&mut w).any(|c| {
                            matches!(c.kind(), "accessibility_modifier" | "override_modifier")
                                || c.kind() == "readonly"
                        }) {
                            refuse = Some("a TypeScript parameter property is also a class field");
                        }
                        child = cur;
                        cur = cur.parent()?;
                    }
                    k if PAT.contains(&k) => {
                        child = cur;
                        cur = cur.parent()?;
                    }
                    _ => return None,
                }
            }
        }

        // ------------------------------------------------------------------ Go
        fn go(&mut self, n: Node, a: &mut Analysis) {
            let k = n.kind();
            if matches!(k, "field_identifier" | "package_identifier" | "label_name") {
                a.other.push(n.start_byte());
                return;
            }
            if !matches!(k, "identifier" | "type_identifier") || self.text(n) == "_" {
                return;
            }
            let Some(p) = n.parent() else { return };
            if k == "type_identifier" {
                // Go has one namespace for types and values, and the grammar can't
                // tell `xs[i](…)` from a generic instantiation `f[T](…)`: a type name
                // is resolved like any other name.
                if matches!(p.kind(), "type_spec" | "type_alias") && field_is(n, p, "name") {
                    let sc = self.go_scope(p);
                    let local = sc != (0, self.src.len());
                    let from = if local { p.start_byte() } else { 0 };
                    self.go_bind(a, n, sc, from, local, false);
                } else if p.kind() == "type_parameter_declaration" {
                    a.opaque.insert(n.start_byte(), "a type parameter");
                } else {
                    self.use_(a, n, false);
                }
                return;
            }
            if p.kind() == "literal_element"
                && p.parent()
                    .is_some_and(|ke| ke.kind() == "keyed_element" && field_is(p, ke, "key"))
            {
                a.opaque.insert(
                    n.start_byte(),
                    "a composite-literal key (struct field or map key)",
                );
                return;
            }
            match p.kind() {
                "parameter_declaration" | "variadic_parameter_declaration"
                    if field_is_any(n, p, "name") =>
                {
                    let f = ancestor(
                        p,
                        &["function_declaration", "method_declaration", "func_literal"],
                    );
                    if let Some(f) = f {
                        let body = f.child_by_field_name("body").unwrap_or(f);
                        a.add_decl(self.decl(n, body.start_byte(), body.end_byte(), true));
                    }
                    return;
                }
                "function_declaration" if field_is(n, p, "name") => {
                    a.add_decl(self.decl(n, 0, self.src.len(), false));
                    return;
                }
                "var_spec" | "const_spec" if field_is_any(n, p, "name") => {
                    let sc = self.go_scope(p);
                    let local = sc != (0, self.src.len());
                    let from = if local { p.end_byte() } else { 0 };
                    self.go_bind(a, n, sc, from, local, false);
                    return;
                }
                _ => {}
            }
            // `a, b := …` and `for k, v := range …`, `case v := <-ch:`.
            if p.kind() == "expression_list" {
                if let Some(stmt) = p.parent() {
                    let defines = |s: Node| {
                        let mut w = s.walk();
                        let r = s.children(&mut w).any(|c| c.kind() == ":=");
                        r
                    };
                    if field_is(p, stmt, "left") && defines(stmt) {
                        match stmt.kind() {
                            "short_var_declaration" | "receive_statement" => {
                                let sc = self.go_scope(stmt);
                                self.go_bind(a, n, sc, stmt.end_byte(), true, true);
                                return;
                            }
                            "range_clause" => {
                                if let Some(f) = stmt.parent() {
                                    let body = f.child_by_field_name("body").unwrap_or(f);
                                    self.go_bind(a, n, span(f), body.start_byte(), true, false);
                                }
                                return;
                            }
                            _ => {}
                        }
                    }
                    if stmt.kind() == "type_switch_statement" && field_is(p, stmt, "alias") {
                        let from = stmt
                            .child_by_field_name("value")
                            .map(|v| v.end_byte())
                            .unwrap_or(stmt.start_byte());
                        self.go_bind(a, n, span(stmt), from, true, false);
                        return;
                    }
                }
            }
            self.use_(a, n, false);
        }

        fn go_scope(&self, n: Node) -> Span {
            const SC: &[&str] = &[
                "block",
                "if_statement",
                "for_statement",
                "expression_switch_statement",
                "type_switch_statement",
                "select_statement",
                "expression_case",
                "default_case",
                "type_case",
                "communication_case",
                "func_literal",
                "function_declaration",
                "method_declaration",
            ];
            ancestor(n, SC).map(span).unwrap_or((0, self.src.len()))
        }

        /// Bind `n` in scope `sc`; with `redeclare_assigns`, a name already bound in the
        /// same scope is an assignment (`x, err := …`), not a new variable.
        fn go_bind(
            &mut self,
            a: &mut Analysis,
            n: Node,
            sc: Span,
            from: usize,
            local: bool,
            redeclare_assigns: bool,
        ) {
            let key = (sc, self.text(n).to_string());
            if redeclare_assigns && self.go_scopes.contains_key(&key) {
                self.use_(a, n, false);
                return;
            }
            let i = a.add_decl(self.decl(n, from, sc.1, local));
            self.go_scopes.insert(key, i);
        }
    }

    fn field_is_any(n: Node, parent: Node, field: &str) -> bool {
        let mut w = parent.walk();
        let r = parent
            .children_by_field_name(field, &mut w)
            .any(|c| c.id() == n.id());
        r
    }

    const JS_FN: &[&str] = &[
        "function_declaration",
        "function_expression",
        "function",
        "generator_function_declaration",
        "generator_function",
        "arrow_function",
        "method_definition",
        "class_static_block",
    ];
    const JS_BLOCK: &[&str] = &[
        "statement_block",
        "for_statement",
        "for_in_statement",
        "switch_body",
        "class_static_block",
        "program",
    ];
}
