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

pub use crate::scopes::{local_name_starts, local_name_starts_in};

#[cfg(not(feature = "ast"))]
mod imp {
    use super::*;
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

    use crate::scopes::*;

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
        if fam == Family::Rust && [&o.old, &o.new].iter().any(|n| starts_upper(n)) {
            return Err(unsupported(
                "Rust names starting with an uppercase letter can be constants or enum variants in patterns; rename only covers snake_case locals".into(),
            ));
        }

        let crate::change::Text { src, rev } =
            crate::change::load(&o.path).map_err(|e| (e.status, e.message))?;
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
        let line_starts = crate::change::line_starts(&src);
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
        if let Some((a, b)) = crate::change::added_syntax_errors(&o.path, &src, &o.path, &new_src) {
            return Err(invalid(format!(
                "renaming would add {} syntax error(s); nothing written",
                b - a
            )));
        }

        r.declaration = Some(decl_site);
        r.sites = edits.iter().map(|(s, _)| site_of(s.0)).collect();
        r.diff = edit::unified_diff(&r.path, &src, &new_src);
        r.rev_after = Some(edit::rev(new_src.as_bytes()));
        if o.apply {
            let change = crate::change::FileChange::update(&o.path, r.path.clone(), src, new_src);
            crate::change::commit(&[change]).map_err(|e| (e.status, e.message))?;
            r.applied = true;
        }
        Ok(())
    }

    fn is_ident_char(fam: Family, c: char) -> bool {
        c.is_alphanumeric() || c == '_' || (fam == Family::Js && c == '$')
    }

    fn check_new_name(fam: Family, n: &str) -> Result<(), String> {
        let mut cs = n.chars();
        let ok_first = cs
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || (fam == Family::Js && c == '$'));
        if !ok_first || !cs.all(|c| is_ident_char(fam, c)) || n == "_" {
            return Err(format!("`{n}` is not a valid identifier"));
        }
        let kw: &[&str] = match fam {
            Family::Rust => &[
                "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
                "enum", "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop",
                "match", "mod", "move", "mut", "pub", "ref", "return", "self", "static", "struct",
                "super", "trait", "true", "try", "type", "union", "unsafe", "use", "where",
                "while", "yield", "abstract", "become", "box", "do", "final", "macro", "override",
                "priv", "typeof", "unsized", "virtual",
            ],
            Family::Python => &[
                "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
                "continue", "def", "del", "elif", "else", "except", "finally", "for", "from",
                "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass",
                "raise", "return", "try", "while", "with", "yield",
            ],
            Family::Js => &[
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
            Family::Go => &[
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
                } else if fam == Family::Rust
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
}
