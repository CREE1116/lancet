//! Scope resolution: which binding each name refers to, for four language
//! families (Rust, Python, JavaScript/TypeScript, Go). `rename` uses it to rename a
//! local safely; `refs` uses it to tell a same-named local from the definition.

#[cfg(not(feature = "ast"))]
mod imp {
    pub fn local_name_starts(_path: &std::path::Path, _src: &str) -> Option<Vec<usize>> {
        None
    }
    pub fn local_name_starts_in(_syn: &crate::code::Syntax, _src: &str) -> Option<Vec<usize>> {
        None
    }
}

#[cfg(feature = "ast")]
pub(crate) use imp::*;
pub use imp::{local_name_starts, local_name_starts_in};

#[cfg(feature = "ast")]
mod imp {
    use crate::code::Lang;
    use crate::output::Status;
    use std::collections::HashMap;
    use tree_sitter::Node;

    pub(crate) type Fail = (Status, String);
    pub(crate) type Span = (usize, usize);

    /// The scope rules a language follows (`LangSpec::scopes`).
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(crate) enum Family {
        Rust,
        Python,
        Js,
        Go,
    }

    pub(crate) fn family(lang: Lang) -> Option<Family> {
        lang.spec().scopes
    }

    /// A binding: where it is written and where uses of its name see it.
    #[derive(Clone, Debug)]
    pub(crate) struct Decl {
        pub(crate) name: String,
        /// Identifier spans that bind it (Python may bind one local several times).
        pub(crate) sites: Vec<Span>,
        /// Which of `sites` are shorthand (`S { x }`, `{x}`): renamed as `x: new`.
        pub(crate) shorthand: Vec<bool>,
        /// Uses at byte `p` see this binding when `from <= p < to`, outside `holes`.
        pub(crate) from: usize,
        pub(crate) to: usize,
        pub(crate) holes: Vec<Span>,
        /// Declared inside a function (so no other file can use it).
        pub(crate) local: bool,
        /// Why renaming this binding is refused even though it is local.
        pub(crate) refuse: Option<&'static str>,
    }

    #[derive(Clone, Debug)]
    pub(crate) struct Use {
        pub(crate) name: String,
        pub(crate) span: Span,
        pub(crate) shorthand: bool,
    }

    #[derive(Default, Debug)]
    pub(crate) struct Analysis {
        pub(crate) decls: Vec<Decl>,
        pub(crate) uses: Vec<Use>,
        /// Identifier start → why its meaning can't be determined (macro, JSX tag…).
        pub(crate) opaque: HashMap<usize, &'static str>,
        /// Identifier starts that are names in another namespace (fields, types…).
        pub(crate) other: Vec<usize>,
        /// Byte ranges where names are resolved at run time (`eval`, `with`, `locals()`).
        pub(crate) dynamic: Vec<(Span, &'static str)>,
        /// Names declared `global` / `nonlocal` somewhere in the file.
        pub(crate) rebound: Vec<String>,
        /// String literals inside std formatting macros lancet resolved: plain text
        /// (the format string's `{name}` captures are in `uses`).
        pub(crate) literal: Vec<Span>,
        pub(crate) by_name: HashMap<String, Vec<usize>>,
    }

    impl Analysis {
        pub(crate) fn add_decl(&mut self, d: Decl) -> usize {
            let i = self.decls.len();
            self.by_name.entry(d.name.clone()).or_default().push(i);
            self.decls.push(d);
            i
        }

        /// The binding a use of `name` at byte `p` refers to: the innermost one in
        /// view (the latest `from`), or `None` for a free name (global, import, builtin).
        pub(crate) fn resolve(&self, name: &str, p: usize) -> Option<usize> {
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

    pub(crate) fn starts_upper(s: &str) -> bool {
        s.chars().next().is_some_and(char::is_uppercase)
    }

    pub(crate) fn parse(lang: Lang, src: &str) -> Result<tree_sitter::Tree, Fail> {
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

    pub(crate) fn analyze(lang: Lang, fam: Family, src: &str) -> Result<Analysis, Fail> {
        Ok(analyze_tree(fam, src, &parse(lang, src)?))
    }

    pub(crate) fn analyze_tree(fam: Family, src: &str, tree: &tree_sitter::Tree) -> Analysis {
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

    pub(crate) struct Cx<'s> {
        pub(crate) fam: Family,
        pub(crate) src: &'s str,
        /// Go: (scope span, name) → binding, to tell `:=` redeclaration from assignment.
        pub(crate) go_scopes: HashMap<(Span, String), usize>,
        /// Python: (function span, name) → the one local binding of that name.
        pub(crate) py_locals: HashMap<(Span, String), usize>,
        /// Rust: identifier starts inside std formatting macros that are plain uses /
        /// names in another namespace (`.field`, `path::`, `name =`).
        pub(crate) fmt_uses: std::collections::HashSet<usize>,
        pub(crate) fmt_other: std::collections::HashSet<usize>,
    }

    /// Names a Rust format string captures: `{name}`, `{name:?}`, `{:width$}`, with
    /// their byte offsets in `s`.
    pub(crate) fn format_captures(s: &str) -> Vec<(usize, &str)> {
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

    pub(crate) fn span(n: Node) -> Span {
        (n.start_byte(), n.end_byte())
    }

    pub(crate) fn field_is(n: Node, parent: Node, field: &str) -> bool {
        parent
            .child_by_field_name(field)
            .is_some_and(|c| c.id() == n.id())
    }

    pub(crate) fn ancestor<'t>(n: Node<'t>, kinds: &[&str]) -> Option<Node<'t>> {
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
                Family::Rust => self.rust(n, a),
                Family::Python => self.python(n, a),
                Family::Js => self.js(n, a),
                Family::Go => self.go(n, a),
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

    pub(crate) fn field_is_any(n: Node, parent: Node, field: &str) -> bool {
        let mut w = parent.walk();
        let r = parent
            .children_by_field_name(field, &mut w)
            .any(|c| c.id() == n.id());
        r
    }

    pub(crate) const JS_FN: &[&str] = &[
        "function_declaration",
        "function_expression",
        "function",
        "generator_function_declaration",
        "generator_function",
        "arrow_function",
        "method_definition",
        "class_static_block",
    ];
    pub(crate) const JS_BLOCK: &[&str] = &[
        "statement_block",
        "for_statement",
        "for_in_statement",
        "switch_body",
        "class_static_block",
        "program",
    ];
}
