//! Tree-sitter structural AST pattern search and safe refactoring (`lancet ast`),
//! automatically accelerated by the `.weft` trigram index when available.

use crate::output::{ErrorInfo, Status};
use crate::search::IndexMode;
use crate::walk::WalkOpts;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct AstOptions {
    pub pattern: String,
    pub lang: Option<String>,
    pub root: PathBuf,
    pub rewrite: Option<String>,
    pub apply: bool,
    pub paths: Vec<String>,
    pub limit: Option<usize>,
    pub walk: WalkOpts,
    pub index: IndexMode,
    pub index_dir: Option<PathBuf>,
}

impl AstOptions {
    pub fn new(pattern: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        AstOptions {
            pattern: pattern.into(),
            lang: None,
            root: root.into(),
            rewrite: None,
            apply: false,
            paths: Vec::new(),
            limit: None,
            walk: WalkOpts::default(),
            index: IndexMode::Auto,
            index_dir: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AstMatch {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub text: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub captures: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub replacements: usize,
    pub skipped_overlaps: usize,
    pub diff: String,
    pub applied: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AstStats {
    pub files_considered: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_candidates: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_files: Option<usize>,
    pub files_parsed: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trigram_literals: Vec<String>,
    pub elapsed_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct AstResponse {
    pub status: Status,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    pub pattern: String,
    pub matched: usize,
    pub returned: usize,
    pub truncated: bool,
    pub matches: Vec<AstMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diffs: Vec<FileDiff>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
    pub stats: AstStats,
}

impl AstResponse {
    fn failure(
        status: Status,
        lang: Option<String>,
        pattern: String,
        message: String,
        elapsed_ms: f64,
    ) -> Self {
        AstResponse {
            status,
            mode: "scan".into(),
            lang,
            pattern,
            matched: 0,
            returned: 0,
            truncated: false,
            matches: Vec::new(),
            diffs: Vec::new(),
            warnings: Vec::new(),
            error: Some(ErrorInfo { message, pos: None }),
            stats: AstStats {
                elapsed_ms,
                ..AstStats::default()
            },
        }
    }
}

#[cfg(feature = "ast")]
mod imp {
    use super::{AstMatch, AstOptions, AstResponse, AstStats, FileDiff};
    use crate::code::Lang;
    use crate::index::{self, Req};
    use crate::output::{Stats, Status};
    use crate::query::{Expr, Filters, Matcher};
    use crate::search::{plan_targets_with_req, IndexMode, Target};
    use crate::walk::{walk, FileFilter};
    use memchr::memmem;
    use rayon::prelude::*;
    use std::cell::RefCell;
    use std::cmp::Reverse;
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;
    use std::time::Instant;
    use tree_sitter::{Node, Parser, Tree};

    thread_local! {
        static AST_PARSER: RefCell<(Option<Lang>, Parser)> = RefCell::new((None, Parser::new()));
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct MetaSpec {
        name: Option<String>,
        multi: bool,
    }

    #[derive(Clone, Debug)]
    enum PatNode {
        Meta(MetaSpec),
        Leaf {
            kind_id: u16,
            is_comma: bool,
            is_closing: bool,
            text: String,
        },
        Branch {
            kind_id: u16,
            children: Vec<PatNode>,
        },
    }

    struct CompiledPattern {
        lang: Lang,
        root: PatNode,
        declared_names: HashSet<String>,
        trigram_literals: Vec<String>,
        prefilters: Vec<memmem::Finder<'static>>,
    }

    fn is_ident_start(b: u8) -> bool {
        b.is_ascii_uppercase()
    }

    fn is_ident_cont(b: u8) -> bool {
        b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'
    }

    fn is_word_byte(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_'
    }

    type PreprocessOut = (String, HashMap<String, MetaSpec>, HashSet<String>);

    fn preprocess_pattern(raw: &str) -> Result<PreprocessOut, String> {
        let bytes = raw.as_bytes();
        let mut out = String::with_capacity(raw.len() + 32);
        let mut sentinels = HashMap::new();
        let mut declared = HashSet::new();
        let mut anon_counter = 0usize;
        let mut i = 0usize;

        while i < bytes.len() {
            if bytes[i] == b'$' {
                if bytes[i..].starts_with(b"$$$") {
                    let after = i + 3;
                    if after < bytes.len() && is_ident_start(bytes[after]) {
                        let mut end = after + 1;
                        while end < bytes.len() && is_ident_cont(bytes[end]) {
                            end += 1;
                        }
                        let name = &raw[after..end];
                        let sent = format!("__LANCET_MULTI_{name}__");
                        declared.insert(name.to_string());
                        sentinels.insert(
                            sent.clone(),
                            MetaSpec {
                                name: Some(name.to_string()),
                                multi: true,
                            },
                        );
                        out.push_str(&sent);
                        i = end;
                        continue;
                    } else if after >= bytes.len() || !is_word_byte(bytes[after]) {
                        anon_counter += 1;
                        let sent = format!("__LANCET_MULTI_ANON_{anon_counter}__");
                        sentinels.insert(
                            sent.clone(),
                            MetaSpec {
                                name: None,
                                multi: true,
                            },
                        );
                        out.push_str(&sent);
                        i = after;
                        continue;
                    }
                }
                let after = i + 1;
                if after < bytes.len()
                    && bytes[after] == b'_'
                    && (after + 1 >= bytes.len() || !is_word_byte(bytes[after + 1]))
                {
                    anon_counter += 1;
                    let sent = format!("__LANCET_MV_ANON_{anon_counter}__");
                    sentinels.insert(
                        sent.clone(),
                        MetaSpec {
                            name: None,
                            multi: false,
                        },
                    );
                    out.push_str(&sent);
                    i = after + 1;
                    continue;
                }
                if after < bytes.len() && is_ident_start(bytes[after]) {
                    let mut end = after + 1;
                    while end < bytes.len() && is_ident_cont(bytes[end]) {
                        end += 1;
                    }
                    let name = &raw[after..end];
                    let sent = format!("__LANCET_MV_{name}__");
                    declared.insert(name.to_string());
                    sentinels.insert(
                        sent.clone(),
                        MetaSpec {
                            name: Some(name.to_string()),
                            multi: false,
                        },
                    );
                    out.push_str(&sent);
                    i = end;
                    continue;
                }
            }
            let ch = raw[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        Ok((out, sentinels, declared))
    }

    fn parse_src(lang: Lang, src: &str) -> Option<Tree> {
        AST_PARSER.with(|cell| {
            let mut st = cell.borrow_mut();
            if st.0 != Some(lang) {
                st.1.set_language(&lang.language()).ok()?;
                st.0 = Some(lang);
            }
            st.1.parse(src.as_bytes(), None)
        })
    }

    fn path_to_descendant(root: Node<'_>, target: Node<'_>) -> Option<Vec<usize>> {
        if root.id() == target.id() {
            return Some(Vec::new());
        }
        let mut cursor = root.walk();
        for (idx, child) in root.children(&mut cursor).enumerate() {
            if let Some(mut sub) = path_to_descendant(child, target) {
                let mut p = Vec::with_capacity(1 + sub.len());
                p.push(idx);
                p.append(&mut sub);
                return Some(p);
            }
        }
        None
    }

    fn node_at_path<'a>(root: Node<'a>, path: &[usize]) -> Option<Node<'a>> {
        let mut cur = root;
        for &idx in path {
            cur = cur.child(idx as u32)?;
        }
        Some(cur)
    }

    fn is_comment(kind: &str) -> bool {
        matches!(kind, "comment" | "line_comment" | "block_comment")
    }

    fn single_content_child<'a>(node: Node<'a>, skip_delims: &[&str]) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let mut found: Option<Node<'a>> = None;
        for c in node.children(&mut cursor) {
            if is_comment(c.kind()) || skip_delims.contains(&c.kind()) {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(c);
        }
        found
    }

    fn first_named_child<'a>(node: Node<'a>) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor);
        children.next()
    }

    fn find_named_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let mut children = node.named_children(&mut cursor);
        children.find(|n| n.kind() == kind)
    }

    fn unwrap_expr_stmt<'a>(node: Node<'a>, ends_with_semi: bool) -> Node<'a> {
        if !ends_with_semi && node.kind() == "expression_statement" {
            let mut cursor = node.walk();
            let named: Vec<Node<'a>> = node.named_children(&mut cursor).collect();
            if named.len() == 1 {
                return named[0];
            }
        }
        node
    }

    fn try_extract(
        lang: Lang,
        prep: &str,
        ends_with_semi: bool,
    ) -> Option<(Tree, String, Vec<usize>)> {
        #[derive(Clone, Copy)]
        enum Strategy {
            RustLetValue,
            RustBlockSingle,
            RustTopSingle,
            RustImplSingle,
            RustTypeExpr,
            PyDirect,
            JsDirect,
            JsConstParen,
            JsClassMember,
            GoAssignRhs,
            GoBlockSingle,
            GoTopSingle,
            GoTypeExpr,
        }

        let strategies: &[Strategy] = match lang {
            Lang::Rust => {
                if ends_with_semi {
                    &[
                        Strategy::RustBlockSingle,
                        Strategy::RustLetValue,
                        Strategy::RustTopSingle,
                        Strategy::RustImplSingle,
                        Strategy::RustTypeExpr,
                    ]
                } else {
                    &[
                        Strategy::RustLetValue,
                        Strategy::RustBlockSingle,
                        Strategy::RustTopSingle,
                        Strategy::RustImplSingle,
                        Strategy::RustTypeExpr,
                    ]
                }
            }
            Lang::Python => &[Strategy::PyDirect],
            Lang::Js | Lang::Ts | Lang::Tsx => &[
                Strategy::JsDirect,
                Strategy::JsConstParen,
                Strategy::JsClassMember,
            ],
            Lang::Go => &[
                Strategy::GoAssignRhs,
                Strategy::GoBlockSingle,
                Strategy::GoTopSingle,
                Strategy::GoTypeExpr,
            ],
        };

        for &strat in strategies {
            let wrapped = match strat {
                Strategy::RustLetValue => format!("fn __lancet_wrap__() {{ let _ = {prep}; }}"),
                Strategy::RustBlockSingle => format!("fn __lancet_wrap__() {{ {prep} }}"),
                Strategy::RustTopSingle => prep.to_string(),
                Strategy::RustImplSingle => format!("impl __LancetWrap__ {{ {prep} }}"),
                Strategy::RustTypeExpr => format!("type __LancetWrap__ = {prep};"),
                Strategy::PyDirect | Strategy::JsDirect => prep.to_string(),
                Strategy::JsConstParen => format!("const __lancet_wrap__ = ({prep});"),
                Strategy::JsClassMember => format!("class __LancetWrap__ {{ {prep} }}"),
                Strategy::GoAssignRhs => {
                    format!("package p\nfunc __lancet_wrap__() {{ _ = {prep} }}")
                }
                Strategy::GoBlockSingle => {
                    format!("package p\nfunc __lancet_wrap__() {{\n{prep}\n}}")
                }
                Strategy::GoTopSingle => format!("package p\n{prep}"),
                Strategy::GoTypeExpr => format!("package p\ntype __LancetWrap__ {prep}"),
            };

            let Some(tree) = parse_src(lang, &wrapped) else {
                continue;
            };
            let root = tree.root_node();
            if root.has_error() {
                continue;
            }

            let picked: Option<Node<'_>> = match strat {
                Strategy::RustLetValue => single_content_child(root, &[])
                    .and_then(|f| f.child_by_field_name("body"))
                    .and_then(|b| single_content_child(b, &["{", "}"]))
                    .and_then(|l| l.child_by_field_name("value")),
                Strategy::RustBlockSingle => single_content_child(root, &[])
                    .and_then(|f| f.child_by_field_name("body"))
                    .and_then(|b| single_content_child(b, &["{", "}"])),
                Strategy::RustTopSingle => single_content_child(root, &[]),
                Strategy::RustImplSingle => single_content_child(root, &[])
                    .and_then(|imp_n| imp_n.child_by_field_name("body"))
                    .and_then(|dl| single_content_child(dl, &["{", "}"])),
                Strategy::RustTypeExpr => {
                    single_content_child(root, &[]).and_then(|ti| ti.child_by_field_name("type"))
                }
                Strategy::PyDirect | Strategy::JsDirect => {
                    single_content_child(root, &[]).map(|n| unwrap_expr_stmt(n, ends_with_semi))
                }
                Strategy::JsConstParen => single_content_child(root, &[])
                    .and_then(first_named_child)
                    .and_then(|decl| decl.child_by_field_name("value"))
                    .and_then(|paren| single_content_child(paren, &["(", ")"])),
                Strategy::JsClassMember => single_content_child(root, &[])
                    .and_then(|cls| cls.child_by_field_name("body"))
                    .and_then(|body| single_content_child(body, &["{", "}"])),
                Strategy::GoAssignRhs => find_named_child(root, "function_declaration")
                    .and_then(|f| f.child_by_field_name("body"))
                    .and_then(|b| {
                        let inner = single_content_child(b, &["{", "}"])?;
                        let stmt = if inner.kind() == "statement_list" {
                            single_content_child(inner, &[])?
                        } else {
                            inner
                        };
                        stmt.child_by_field_name("right")
                            .and_then(|rhs| single_content_child(rhs, &[]))
                    }),
                Strategy::GoBlockSingle => find_named_child(root, "function_declaration")
                    .and_then(|f| f.child_by_field_name("body"))
                    .and_then(|b| {
                        let inner = single_content_child(b, &["{", "}"])?;
                        if inner.kind() == "statement_list" {
                            single_content_child(inner, &[])
                        } else {
                            Some(inner)
                        }
                    }),
                Strategy::GoTopSingle => {
                    let mut cur = root.walk();
                    let decls: Vec<Node<'_>> = root
                        .named_children(&mut cur)
                        .filter(|n| n.kind() != "package_clause" && !is_comment(n.kind()))
                        .collect();
                    if decls.len() == 1 {
                        Some(decls[0])
                    } else {
                        None
                    }
                }
                Strategy::GoTypeExpr => find_named_child(root, "type_declaration")
                    .and_then(|td| find_named_child(td, "type_spec"))
                    .and_then(|ts| ts.child_by_field_name("type")),
            };

            let Some(node) = picked else {
                continue;
            };
            if node.is_missing() || node.has_error() {
                continue;
            }
            if let Some(path) = path_to_descendant(root, node) {
                return Some((tree, wrapped, path));
            }
        }
        None
    }

    fn is_string_or_char_node(node: Node<'_>) -> bool {
        let mut cur = Some(node);
        while let Some(n) = cur {
            let k = n.kind();
            if k.contains("string") || k.contains("char_literal") || k == "rune_literal" {
                return true;
            }
            cur = n.parent();
        }
        false
    }

    fn extract_trigram_literals(
        root: Node<'_>,
        src: &str,
        sentinels: &HashMap<String, MetaSpec>,
    ) -> Vec<String> {
        let mut out = BTreeSet::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            let k = node.kind();
            if is_comment(k) {
                continue;
            }
            let text = src[node.byte_range()].trim();
            if sentinels.contains_key(text) {
                continue;
            }
            if node.child_count() == 0 {
                if !is_string_or_char_node(node)
                    && text.len() >= 3
                    && text.bytes().all(is_word_byte)
                    && !text.starts_with("__LANCET_")
                {
                    out.insert(text.to_string());
                }
                continue;
            }
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                stack.push(child);
            }
        }
        out.into_iter().collect()
    }

    fn build_pat_node(node: Node<'_>, src: &str, sentinels: &HashMap<String, MetaSpec>) -> PatNode {
        let trimmed = src[node.byte_range()].trim();
        if let Some(spec) = sentinels.get(trimmed) {
            return PatNode::Meta(spec.clone());
        }
        let children: Vec<PatNode> = non_comment_children(node)
            .into_iter()
            .map(|c| build_pat_node(c, src, sentinels))
            .collect();
        if children.is_empty() {
            let k = node.kind();
            PatNode::Leaf {
                kind_id: node.kind_id(),
                is_comma: k == ",",
                is_closing: matches!(k, ")" | "]" | "}" | ">"),
                text: src[node.byte_range()].to_string(),
            }
        } else {
            PatNode::Branch {
                kind_id: node.kind_id(),
                children,
            }
        }
    }

    impl CompiledPattern {
        fn compile(raw: &str, lang: Lang) -> Result<Self, String> {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Err("AST pattern cannot be empty".into());
            }
            let (prep, sentinels, declared_names) = preprocess_pattern(trimmed)?;
            let ends_with_semi = trimmed.ends_with(';');
            let (pat_tree, pat_src, node_path) = try_extract(lang, &prep, ends_with_semi)
                .ok_or_else(|| {
                    format!(
                        "could not parse pattern `{raw}` as valid {} AST",
                        lang.name()
                    )
                })?;
            let root_node = node_at_path(pat_tree.root_node(), &node_path)
                .ok_or_else(|| "internal error locating parsed AST root".to_string())?;
            let trigram_literals = extract_trigram_literals(root_node, &pat_src, &sentinels);
            let root = build_pat_node(root_node, &pat_src, &sentinels);
            let mut sorted_lits = trigram_literals.clone();
            sorted_lits.sort_by_key(|s| Reverse(s.len()));
            let prefilters = sorted_lits
                .into_iter()
                .map(|s| memmem::Finder::new(s.as_bytes()).into_owned())
                .collect();
            Ok(CompiledPattern {
                lang,
                root,
                declared_names,
                trigram_literals,
                prefilters,
            })
        }

        #[inline]
        fn root_kind_id(&self) -> Option<u16> {
            match &self.root {
                PatNode::Meta(_) => None,
                PatNode::Leaf { kind_id, .. } | PatNode::Branch { kind_id, .. } => Some(*kind_id),
            }
        }
    }

    fn non_comment_children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .filter(|c| !is_comment(c.kind()))
            .collect()
    }

    fn bind_meta(env: &mut BTreeMap<String, String>, spec: &MetaSpec, text: &str) -> bool {
        if let Some(name) = &spec.name {
            match env.get(name) {
                Some(prev) => prev == text,
                None => {
                    env.insert(name.clone(), text.to_string());
                    true
                }
            }
        } else {
            true
        }
    }

    fn match_node(
        pnode: &PatNode,
        snode: Node<'_>,
        src: &str,
        env: &mut BTreeMap<String, String>,
    ) -> bool {
        match pnode {
            PatNode::Meta(spec) => bind_meta(env, spec, &src[snode.byte_range()]),
            PatNode::Leaf { kind_id, text, .. } => {
                if snode.kind_id() != *kind_id {
                    return false;
                }
                non_comment_children(snode).is_empty() && &src[snode.byte_range()] == text
            }
            PatNode::Branch { kind_id, children } => {
                if snode.kind_id() != *kind_id {
                    return false;
                }
                let s_children = non_comment_children(snode);
                match_seq(children, &s_children, src, env)
            }
        }
    }

    fn is_closing_delim(node: Node<'_>) -> bool {
        matches!(node.kind(), ")" | "]" | "}" | ">")
    }

    fn match_seq(
        p_slice: &[PatNode],
        s_slice: &[Node<'_>],
        src: &str,
        env: &mut BTreeMap<String, String>,
    ) -> bool {
        if p_slice.is_empty() {
            return s_slice.is_empty();
        }
        if let PatNode::Meta(spec) = &p_slice[0] {
            if spec.multi {
                for take in (0..=s_slice.len()).rev() {
                    let mut next_env = env.clone();
                    let captured = if take == 0 {
                        String::new()
                    } else {
                        let start = s_slice[0].start_byte();
                        let end = s_slice[take - 1].end_byte();
                        src[start..end].to_string()
                    };
                    if !bind_meta(&mut next_env, spec, &captured) {
                        continue;
                    }
                    if match_seq(&p_slice[1..], &s_slice[take..], src, &mut next_env) {
                        *env = next_env;
                        return true;
                    }
                }
                return false;
            }
        }

        if p_slice.len() >= 2 && matches!(p_slice[0], PatNode::Leaf { is_comma: true, .. }) {
            if let PatNode::Meta(spec) = &p_slice[1] {
                let next_pat_closes = p_slice.len() == 2
                    || matches!(
                        p_slice[2],
                        PatNode::Leaf {
                            is_closing: true,
                            ..
                        }
                    );
                if spec.multi
                    && next_pat_closes
                    && (s_slice.is_empty() || is_closing_delim(s_slice[0]))
                {
                    let mut next_env = env.clone();
                    if bind_meta(&mut next_env, spec, "")
                        && match_seq(&p_slice[2..], s_slice, src, &mut next_env)
                    {
                        *env = next_env;
                        return true;
                    }
                }
            }
        }

        if s_slice.is_empty() {
            return false;
        }
        let mut next_env = env.clone();
        if !match_node(&p_slice[0], s_slice[0], src, &mut next_env) {
            return false;
        }
        if match_seq(&p_slice[1..], &s_slice[1..], src, &mut next_env) {
            *env = next_env;
            return true;
        }
        false
    }

    fn validate_rewrite_template(tpl: &str, declared: &HashSet<String>) -> Result<(), String> {
        let bytes = tpl.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' {
                if bytes[i..].starts_with(b"$$$") {
                    let after = i + 3;
                    if after < bytes.len() && is_ident_start(bytes[after]) {
                        let mut end = after + 1;
                        while end < bytes.len() && is_ident_cont(bytes[end]) {
                            end += 1;
                        }
                        let name = &tpl[after..end];
                        if !declared.contains(name) {
                            return Err(format!(
                                "rewrite template references undeclared metavariable `$${name}`"
                            )
                            .replacen("$$", "$$$", 1));
                        }
                        i = end;
                        continue;
                    } else {
                        return Err("rewrite template cannot reference anonymous `$$$`".into());
                    }
                }
                let after = i + 1;
                if after < bytes.len()
                    && bytes[after] == b'_'
                    && (after + 1 >= bytes.len() || !is_word_byte(bytes[after + 1]))
                {
                    return Err("rewrite template cannot reference anonymous `$_`".into());
                }
                if after < bytes.len() && is_ident_start(bytes[after]) {
                    let mut end = after + 1;
                    while end < bytes.len() && is_ident_cont(bytes[end]) {
                        end += 1;
                    }
                    let name = &tpl[after..end];
                    if !declared.contains(name) {
                        return Err(format!(
                            "rewrite template references undeclared metavariable `${name}`"
                        ));
                    }
                    i = end;
                    continue;
                }
            }
            let ch = tpl[i..].chars().next().unwrap();
            i += ch.len_utf8();
        }
        Ok(())
    }

    fn render_rewrite(tpl: &str, captures: &BTreeMap<String, String>) -> String {
        let bytes = tpl.as_bytes();
        let mut out = String::with_capacity(tpl.len() + 16);
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' {
                if bytes[i..].starts_with(b"$$$") {
                    let after = i + 3;
                    if after < bytes.len() && is_ident_start(bytes[after]) {
                        let mut end = after + 1;
                        while end < bytes.len() && is_ident_cont(bytes[end]) {
                            end += 1;
                        }
                        let name = &tpl[after..end];
                        if let Some(val) = captures.get(name) {
                            out.push_str(val);
                            i = end;
                            continue;
                        }
                    }
                }
                let after = i + 1;
                if after < bytes.len() && is_ident_start(bytes[after]) {
                    let mut end = after + 1;
                    while end < bytes.len() && is_ident_cont(bytes[end]) {
                        end += 1;
                    }
                    let name = &tpl[after..end];
                    if let Some(val) = captures.get(name) {
                        out.push_str(val);
                        i = end;
                        continue;
                    }
                }
            }
            let ch = tpl[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }

    fn resolve_lang(opts: &AstOptions) -> Result<Lang, String> {
        if let Some(raw) = &opts.lang {
            return Lang::parse(raw).ok_or_else(|| {
                format!(
                    "unsupported language `{raw}` (supported: rust/rs, python/py, javascript/js, typescript/ts, tsx, go)"
                )
            });
        }
        if let Some(l) = Lang::from_path(&opts.root) {
            return Ok(l);
        }
        Err("specify `--lang <LANG>` (`rs`, `py`, `js`, `ts`, `tsx`, `go`) when searching a directory".into())
    }

    fn build_ast_req(lits: &[String]) -> Req {
        if lits.is_empty() {
            return Req::All;
        }
        let expr = if lits.len() == 1 {
            Expr::Match {
                m: Matcher::Lit {
                    text: lits[0].clone(),
                },
            }
        } else {
            Expr::And {
                args: lits
                    .iter()
                    .map(|t| Expr::Match {
                        m: Matcher::Lit { text: t.clone() },
                    })
                    .collect(),
            }
        };
        Req::from_expr(&expr)
    }

    struct FileResult {
        parsed: bool,
        matches: Vec<AstMatch>,
        diff: Option<FileDiff>,
        warning: Option<String>,
    }

    fn process_file(
        target: &Target,
        pat: &CompiledPattern,
        rewrite: Option<&str>,
        apply: bool,
    ) -> Option<FileResult> {
        let bytes = match fs::read(&target.path) {
            Ok(b) => b,
            Err(_) => return None,
        };
        if memchr::memchr(0, &bytes).is_some() {
            return None;
        }
        let may_have_js_unicode_ident = matches!(pat.lang, Lang::Js | Lang::Ts | Lang::Tsx)
            && memmem::find(&bytes, b"\\u").is_some();
        if !may_have_js_unicode_ident && pat.prefilters.iter().any(|f| f.find(&bytes).is_none()) {
            return Some(FileResult {
                parsed: false,
                matches: Vec::new(),
                diff: None,
                warning: None,
            });
        }

        let src = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => return None,
        };

        let tree = parse_src(pat.lang, &src)?;
        let root_kind_id = pat.root_kind_id();
        let mut matches = Vec::new();
        let mut cursor = tree.root_node().walk();
        let mut descend = true;

        loop {
            if descend {
                let node = cursor.node();
                let kind_ok = match root_kind_id {
                    Some(kid) => node.kind_id() == kid,
                    None => !is_comment(node.kind()),
                };
                if kind_ok {
                    let mut env = BTreeMap::new();
                    if match_node(&pat.root, node, &src, &mut env) {
                        let start = node.start_position();
                        let end = node.end_position();
                        let rep = rewrite.map(|tpl| render_rewrite(tpl, &env));
                        matches.push(AstMatch {
                            path: target.rel.clone(),
                            start_line: start.row + 1,
                            end_line: end.row + 1,
                            start_col: start.column + 1,
                            end_col: end.column + 1,
                            start_byte: node.start_byte(),
                            end_byte: node.end_byte(),
                            text: src[node.byte_range()].to_string(),
                            captures: env,
                            replacement: rep,
                        });
                    }
                }
                if cursor.goto_first_child() {
                    continue;
                }
            }
            if cursor.goto_next_sibling() {
                descend = true;
            } else if cursor.goto_parent() {
                descend = false;
            } else {
                break;
            }
        }

        if matches.is_empty() {
            return Some(FileResult {
                parsed: true,
                matches,
                diff: None,
                warning: None,
            });
        }

        let mut diff = None;
        let mut warning = None;
        if rewrite.is_some() {
            let mut ordered: Vec<&AstMatch> = matches.iter().collect();
            ordered.sort_by_key(|m| (m.start_byte, Reverse(m.end_byte)));
            let mut chosen: Vec<&AstMatch> = Vec::with_capacity(ordered.len());
            let mut last_end = 0usize;
            let mut skipped_overlaps = 0usize;
            for m in ordered {
                if m.start_byte >= last_end {
                    last_end = m.end_byte;
                    chosen.push(m);
                } else {
                    skipped_overlaps += 1;
                }
            }

            let mut new_src = src.clone();
            for m in chosen.iter().rev() {
                if let Some(rep) = &m.replacement {
                    new_src.replace_range(m.start_byte..m.end_byte, rep);
                }
            }

            if new_src != src {
                let udiff = unified_diff(&target.rel, &src, &new_src);
                let mut applied = false;
                if apply {
                    match write_atomic_preserving_perms(&target.path, new_src.as_bytes()) {
                        Ok(()) => applied = true,
                        Err(e) => {
                            warning =
                                Some(format!("failed writing rewrite to {}: {e}", target.rel));
                        }
                    }
                }
                diff = Some(FileDiff {
                    path: target.rel.clone(),
                    replacements: chosen.len(),
                    skipped_overlaps,
                    diff: udiff,
                    applied,
                });
            }
        }

        Some(FileResult {
            parsed: true,
            matches,
            diff,
            warning,
        })
    }

    fn write_atomic_preserving_perms(path: &Path, data: &[u8]) -> io::Result<()> {
        let perms = fs::metadata(path)?.permissions();
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("lancet_tmp");
        let tmp = parent.join(format!(".{name}.lancet-tmp.{}", std::process::id()));
        fs::write(&tmp, data)?;
        let _ = fs::set_permissions(&tmp, perms);
        if let Err(e) = fs::rename(&tmp, path) {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
        Ok(())
    }

    /// Compute a standard unified diff (`--- a/...`, `+++ b/...`) with 3 context lines.
    fn unified_diff(rel: &str, old: &str, new: &str) -> String {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();
        let n = old_lines.len();
        let m = new_lines.len();

        // LCS DP when size is reasonable; fallback to single coarse hunk for huge files.
        let mut ops: Vec<(char, usize, usize)> = Vec::new();
        if n.saturating_mul(m) <= 2_000_000 {
            let mut dp = vec![0u32; (n + 1) * (m + 1)];
            let stride = m + 1;
            for i in (0..n).rev() {
                for j in (0..m).rev() {
                    dp[i * stride + j] = if old_lines[i] == new_lines[j] {
                        dp[(i + 1) * stride + (j + 1)] + 1
                    } else {
                        dp[(i + 1) * stride + j].max(dp[i * stride + (j + 1)])
                    };
                }
            }
            let (mut i, mut j) = (0, 0);
            while i < n && j < m {
                if old_lines[i] == new_lines[j] {
                    ops.push((' ', i, j));
                    i += 1;
                    j += 1;
                } else if dp[(i + 1) * stride + j] >= dp[i * stride + (j + 1)] {
                    ops.push(('-', i, j));
                    i += 1;
                } else {
                    ops.push(('+', i, j));
                    j += 1;
                }
            }
            while i < n {
                ops.push(('-', i, j));
                i += 1;
            }
            while j < m {
                ops.push(('+', i, j));
                j += 1;
            }
        } else {
            let mut pref = 0;
            while pref < n && pref < m && old_lines[pref] == new_lines[pref] {
                ops.push((' ', pref, pref));
                pref += 1;
            }
            let mut suff = 0;
            while suff + pref < n
                && suff + pref < m
                && old_lines[n - 1 - suff] == new_lines[m - 1 - suff]
            {
                suff += 1;
            }
            for idx in pref..(n - suff) {
                ops.push(('-', idx, pref));
            }
            for idx in pref..(m - suff) {
                ops.push(('+', n - suff, idx));
            }
            for k in 0..suff {
                ops.push((' ', n - suff + k, m - suff + k));
            }
        }

        let ctx = 3usize;
        let change_idxs: Vec<usize> = ops
            .iter()
            .enumerate()
            .filter_map(|(k, &(op, _, _))| (op != ' ').then_some(k))
            .collect();
        if change_idxs.is_empty() {
            return String::new();
        }

        let mut hunks: Vec<(usize, usize)> = Vec::new();
        let mut h_start = change_idxs[0].saturating_sub(ctx);
        let mut h_end = (change_idxs[0] + ctx + 1).min(ops.len());
        for &ci in &change_idxs[1..] {
            let s = ci.saturating_sub(ctx);
            let e = (ci + ctx + 1).min(ops.len());
            if s <= h_end {
                h_end = e;
            } else {
                hunks.push((h_start, h_end));
                h_start = s;
                h_end = e;
            }
        }
        hunks.push((h_start, h_end));

        let mut out = format!("--- a/{rel}\n+++ b/{rel}\n");
        for (s, e) in hunks {
            let slice = &ops[s..e];
            let old_start = slice.first().map_or(0, |&(_, oi, _)| oi) + 1;
            let new_start = slice.first().map_or(0, |&(_, _, nj)| nj) + 1;
            let old_cnt = slice.iter().filter(|&&(op, _, _)| op != '+').count();
            let new_cnt = slice.iter().filter(|&&(op, _, _)| op != '-').count();
            out.push_str(&format!(
                "@@ -{old_start},{old_cnt} +{new_start},{new_cnt} @@\n"
            ));
            for &(op, oi, nj) in slice {
                match op {
                    ' ' => {
                        out.push(' ');
                        out.push_str(old_lines[oi]);
                        out.push('\n');
                    }
                    '-' => {
                        out.push('-');
                        out.push_str(old_lines[oi]);
                        out.push('\n');
                    }
                    '+' => {
                        out.push('+');
                        out.push_str(new_lines[nj]);
                        out.push('\n');
                    }
                    _ => {}
                }
            }
        }
        out
    }

    pub fn search_ast(opts: &AstOptions) -> AstResponse {
        let t0 = Instant::now();
        let lang = match resolve_lang(opts) {
            Ok(l) => l,
            Err(e) => {
                return AstResponse::failure(
                    Status::InvalidQuery,
                    None,
                    opts.pattern.clone(),
                    e,
                    super::ms(t0),
                )
            }
        };
        let lang_name = lang.name().to_string();

        if opts.apply && opts.rewrite.is_none() {
            return AstResponse::failure(
                Status::InvalidQuery,
                Some(lang_name),
                opts.pattern.clone(),
                "`--apply` requires `--rewrite <TEMPLATE>`".into(),
                super::ms(t0),
            );
        }
        if opts.apply && opts.limit.is_some() {
            return AstResponse::failure(
                Status::InvalidQuery,
                Some(lang_name),
                opts.pattern.clone(),
                "`--apply` cannot be combined with `--limit` (would partially apply rewrites)"
                    .into(),
                super::ms(t0),
            );
        }

        let compiled = match CompiledPattern::compile(&opts.pattern, lang) {
            Ok(c) => c,
            Err(e) => {
                return AstResponse::failure(
                    Status::InvalidQuery,
                    Some(lang_name),
                    opts.pattern.clone(),
                    e,
                    super::ms(t0),
                )
            }
        };

        if let Some(tpl) = &opts.rewrite {
            if let Err(e) = validate_rewrite_template(tpl, &compiled.declared_names) {
                return AstResponse::failure(
                    Status::InvalidQuery,
                    Some(lang_name),
                    opts.pattern.clone(),
                    e,
                    super::ms(t0),
                );
            }
        }

        let mut filters = Filters::default();
        if opts.root.is_dir() {
            filters.exts = lang.extensions().iter().map(|s| (*s).to_string()).collect();
        }
        filters.paths = opts.paths.clone();
        let filter = match FileFilter::new(&filters) {
            Ok(f) => f,
            Err(e) => {
                return AstResponse::failure(
                    Status::InvalidQuery,
                    Some(lang_name),
                    opts.pattern.clone(),
                    e,
                    super::ms(t0),
                )
            }
        };

        let req = build_ast_req(&compiled.trigram_literals);
        let mut warnings = Vec::new();
        let mut mode = "scan".to_string();
        let mut core_stats = Stats::default();
        let mut targets: Option<Vec<Target>> = None;

        if opts.index != IndexMode::Off {
            match index::discover(&opts.root, opts.index_dir.as_deref()) {
                Ok(Some((ix, rel_prefix))) => {
                    mode = if opts.index == IndexMode::Trust {
                        "index_trusted".into()
                    } else {
                        "index".into()
                    };
                    targets = Some(plan_targets_with_req(
                        &ix,
                        &req,
                        &filter,
                        &opts.root,
                        &rel_prefix,
                        &opts.walk,
                        opts.index,
                        None,
                        &mut core_stats,
                        None,
                    ));
                }
                Ok(None) => {
                    if opts.index == IndexMode::Trust {
                        warnings.push(
                            "no index found; fell back to a full scan (run `lancet index`)".into(),
                        );
                    }
                }
                Err(e) => {
                    warnings.push(format!("index unusable ({e:#}); fell back to a full scan"));
                }
            }
        }

        let targets = targets.unwrap_or_else(|| {
            walk(&opts.root, &opts.walk, &filter, false)
                .into_iter()
                .map(|e| Target {
                    path: e.path,
                    rel: e.rel,
                })
                .collect()
        });
        if core_stats.files_considered == 0 {
            core_stats.files_considered = targets.len();
        }

        let file_results: Vec<Option<FileResult>> = targets
            .par_iter()
            .map(|t| process_file(t, &compiled, opts.rewrite.as_deref(), opts.apply))
            .collect();

        let mut files_parsed = 0usize;
        let mut matches = Vec::new();
        let mut diffs = Vec::new();
        for res in file_results.into_iter().flatten() {
            files_parsed += res.parsed as usize;
            matches.extend(res.matches);
            if let Some(d) = res.diff {
                diffs.push(d);
            }
            if let Some(w) = res.warning {
                warnings.push(w);
            }
        }

        let matched = matches.len();
        let mut truncated = false;
        if let Some(lim) = opts.limit {
            if matches.len() > lim {
                matches.truncate(lim);
                truncated = true;
            }
        }
        let returned = matches.len();

        AstResponse {
            status: if matched == 0 {
                Status::Empty
            } else {
                Status::Complete
            },
            mode,
            lang: Some(lang_name),
            pattern: opts.pattern.clone(),
            matched,
            returned,
            truncated,
            matches,
            diffs,
            warnings,
            error: None,
            stats: AstStats {
                files_considered: core_stats.files_considered,
                index_candidates: core_stats.index_candidates,
                stale_files: core_stats.stale_files,
                files_parsed,
                trigram_literals: compiled.trigram_literals,
                elapsed_ms: super::ms(t0),
            },
        }
    }

    pub fn print_ast_human(resp: &AstResponse, w: &mut impl Write) -> io::Result<()> {
        if let Some(err) = &resp.error {
            return writeln!(w, "error: {}", err.message);
        }
        if !resp.diffs.is_empty() {
            for d in &resp.diffs {
                write!(w, "{}", d.diff)?;
            }
            return Ok(());
        }
        for m in &resp.matches {
            if m.text.contains('\n') {
                writeln!(w, "{}:{}:{}:", m.path, m.start_line, m.start_col)?;
                for line in m.text.lines() {
                    writeln!(w, "  {line}")?;
                }
            } else {
                writeln!(w, "{}:{}:{}: {}", m.path, m.start_line, m.start_col, m.text)?;
            }
        }
        Ok(())
    }
}

#[cfg(not(feature = "ast"))]
mod imp {
    use super::{AstOptions, AstResponse};
    use crate::output::Status;
    use std::io::{self, Write};
    use std::time::Instant;

    pub fn search_ast(opts: &AstOptions) -> AstResponse {
        let t0 = Instant::now();
        AstResponse::failure(
            Status::Unsupported,
            opts.lang.clone(),
            opts.pattern.clone(),
            "`lancet ast` requires building with `--features ast`".into(),
            super::ms(t0),
        )
    }

    pub fn print_ast_human(resp: &AstResponse, w: &mut impl Write) -> io::Result<()> {
        if let Some(err) = &resp.error {
            return writeln!(w, "error: {}", err.message);
        }
        Ok(())
    }
}

pub use imp::{print_ast_human, search_ast};

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}
