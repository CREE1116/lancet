//! Tree-sitter structural AST pattern search and safe refactoring (`lancet ast`),
//! automatically accelerated by the `.lancet` trigram index when available.

use crate::output::{ErrorInfo, Status};
use crate::scope::IndexMode;
use crate::walk::WalkOpts;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct AstOptions {
    pub pattern: String,
    pub lang: Option<String>,
    /// Directories or files to search; results show paths joined onto each.
    pub roots: Vec<PathBuf>,
    pub rewrite: Option<String>,
    /// Write every rewritten file, or none (see [`crate::change::commit`]).
    pub apply: bool,
    pub allow_syntax_errors: bool,
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
            roots: vec![root.into()],
            rewrite: None,
            apply: false,
            allow_syntax_errors: false,
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
    /// Content revision of the file before the rewrite (see [`crate::edit::rev`]).
    pub rev: String,
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
    use crate::change::FileChange;
    use crate::code::Lang;
    use crate::edit;
    use crate::index::Req;
    use crate::output::{ErrorInfo, Status};
    use crate::query::{Expr, Filters, Matcher};
    use crate::scope::Target;
    use crate::walk::FileFilter;
    use memchr::memmem;
    use rayon::prelude::*;
    use std::cell::RefCell;
    use std::cmp::Reverse;
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
    use std::fs;
    use std::io::{self, Write};
    use std::time::Instant;
    use tree_sitter::{Node, Tree};

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

    fn push_sentinel(out: &mut String, sent: &str, quote: bool) {
        if quote {
            out.push('"');
            out.push_str(sent);
            out.push('"');
        } else {
            out.push_str(sent);
        }
    }

    fn preprocess_pattern(raw: &str, quote: bool) -> Result<PreprocessOut, String> {
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
                        push_sentinel(&mut out, &sent, quote);
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
                        push_sentinel(&mut out, &sent, quote);
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
                    push_sentinel(&mut out, &sent, quote);
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
                    push_sentinel(&mut out, &sent, quote);
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
        crate::code::parse_tree(lang, src.as_bytes())
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
        kind.ends_with("comment")
    }

    fn find_exact_span_node<'a>(
        root: Node<'a>,
        span_start: usize,
        span_end: usize,
    ) -> Option<Node<'a>> {
        // Walk down to the outermost node matching `[span_start..span_end]`, then unwrap transparent containers.
        let mut cur = root;
        let mut matched: Option<Node<'a>> = None;
        loop {
            if cur.start_byte() == span_start && cur.end_byte() == span_end {
                matched = Some(cur);
                break;
            }
            let mut walk = cur.walk();
            let next = cur.children(&mut walk).find(|c| {
                !is_comment(c.kind()) && c.start_byte() <= span_start && span_end <= c.end_byte()
            });
            match next {
                Some(child) => cur = child,
                None => break,
            }
        }
        let mut node = matched?;
        // Unwrap every single-child node that spans exactly the pattern: the outer node
        // adds no text, only context (`_ = f(x)` puts the call in an `expression_list`),
        // and keeping it would match only where that context repeats — e.g. never
        // `return 0, f(x)` or `g(1, f(x))`. A trailing `;` in the pattern stays a
        // statement because the `;` is part of the span.
        loop {
            let mut walk = node.walk();
            let children: Vec<Node<'a>> = node
                .children(&mut walk)
                .filter(|c| !is_comment(c.kind()) && c.kind() != ";")
                .collect();
            if children.len() == 1
                && children[0].start_byte() == span_start
                && children[0].end_byte() == span_end
            {
                node = children[0];
            } else {
                break;
            }
        }
        Some(node)
    }

    fn wrapper_candidates(
        lang: Lang,
        ends_with_semi: bool,
    ) -> &'static [(&'static str, &'static str)] {
        let spec = lang.spec();
        match spec.pattern_wrappers_semi {
            Some(semi) if ends_with_semi => semi,
            _ => spec.pattern_wrappers,
        }
    }

    fn try_extract(
        lang: Lang,
        prep: &str,
        ends_with_semi: bool,
    ) -> Option<(Tree, String, Vec<usize>)> {
        let trimmed = prep.trim();
        for &(prefix, suffix) in wrapper_candidates(lang, ends_with_semi) {
            let wrapped = format!("{prefix}{trimmed}{suffix}");
            let span_start = prefix.len();
            let span_end = prefix.len() + trimmed.len();

            let Some(tree) = parse_src(lang, &wrapped) else {
                continue;
            };
            let root = tree.root_node();
            if root.has_error() {
                continue;
            }
            let Some(node) = find_exact_span_node(root, span_start, span_end) else {
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

    /// `if (c) $S` / `{ $$$B }`: a metavariable standing alone as a statement needs a
    /// `;` to parse in C-like languages. Returns the pattern with one added after each
    /// such metavariable, or `None` if there is none.
    fn add_statement_semicolons(
        prep: &str,
        sentinels: &HashMap<String, MetaSpec>,
    ) -> Option<String> {
        let mut at: Vec<usize> = Vec::new();
        for k in sentinels.keys() {
            for (i, _) in prep.match_indices(k.as_str()) {
                let head = prep[..i].trim_end();
                let before = head.chars().last();
                let after = prep[i + k.len()..].trim_start().chars().next();
                // `else $S` / `do $S`: a keyword also opens a statement position.
                let after_keyword = ["else", "do"].iter().any(|kw| {
                    head.ends_with(kw)
                        && !head[..head.len() - kw.len()]
                            .chars()
                            .last()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_')
                });
                // …and a statement ends before `}`, the end, or `else` / `while`.
                let tail = prep[i + k.len()..].trim_start();
                let before_keyword = ["else", "while"].iter().any(|kw| {
                    tail.starts_with(kw)
                        && !tail[kw.len()..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_')
                });
                if (matches!(before, None | Some('{' | ';' | ')' | '}')) || after_keyword)
                    && (matches!(after, None | Some('}')) || before_keyword)
                {
                    at.push(i + k.len());
                }
            }
        }
        if at.is_empty() {
            return None;
        }
        at.sort_unstable();
        let mut out = prep.to_string();
        for i in at.into_iter().rev() {
            out.insert(i, ';');
        }
        Some(out)
    }

    fn build_pat_node(node: Node<'_>, src: &str, sentinels: &HashMap<String, MetaSpec>) -> PatNode {
        let trimmed = src[node.byte_range()].trim().trim_matches('"');
        if let Some(spec) = sentinels.get(trimmed) {
            return PatNode::Meta(spec.clone());
        }
        // A statement that is only a metavariable (`$S;`) stands for any statement, so
        // `$S` matches blocks, returns, ifs…, and `$$$B` several statements in a block.
        let kids = non_comment_children(node);
        if node.kind().ends_with("statement")
            && (kids.len() == 1 || (kids.len() == 2 && kids[1].kind() == ";"))
        {
            let t = src[kids[0].byte_range()].trim().trim_matches('"');
            if let Some(spec) = sentinels.get(t) {
                return PatNode::Meta(spec.clone());
            }
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
            let (prep, sentinels, declared_names) =
                preprocess_pattern(trimmed, lang == Lang::Json)?;
            let ends_with_semi = trimmed.ends_with(';');
            let (pat_tree, pat_src, node_path) = try_extract(lang, &prep, ends_with_semi)
                .or_else(|| {
                    let with_semis = add_statement_semicolons(&prep, &sentinels)?;
                    try_extract(lang, &with_semis, ends_with_semi)
                })
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

    /// Matching steps allowed per candidate node. Memoisation keeps ordinary `$$$`
    /// patterns far below this; a pathological one is cut off at the node and the
    /// file is reported as incomplete instead of hanging or silently dropping matches.
    const MATCH_BUDGET: u32 = 1_000_000;

    thread_local! {
        static MATCH_STEPS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
        /// Set when any node of the current file ran out of budget.
        static MATCH_EXHAUSTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        /// Failed matches for the current file; turns n^k `$$$` backtracking into
        /// roughly k·n² work.
        static MATCH_MEMO: RefCell<HashSet<MemoKey>> = RefCell::new(HashSet::new());
    }

    type MemoKey = (usize, usize, usize, usize, u64);

    /// Whether `p` matches `s` depends only on the slices and on the current values
    /// of the metavariables that occur in `p` (back-references), so those are the key.
    fn memo_key(p: &[PatNode], s: &[Node<'_>], env: &BTreeMap<String, String>) -> MemoKey {
        use std::hash::{Hash, Hasher};
        fn names<'a>(n: &'a PatNode, out: &mut Vec<&'a str>) {
            match n {
                PatNode::Meta(MetaSpec { name: Some(nm), .. }) => out.push(nm),
                PatNode::Meta(_) | PatNode::Leaf { .. } => {}
                PatNode::Branch { children, .. } => children.iter().for_each(|c| names(c, out)),
            }
        }
        let mut used = Vec::new();
        p.iter().for_each(|n| names(n, &mut used));
        let mut h = std::collections::hash_map::DefaultHasher::new();
        if !used.is_empty() {
            for (k, v) in env {
                if used.contains(&k.as_str()) {
                    (k, v).hash(&mut h);
                }
            }
        }
        let first = s.first().map_or(0, |n| n.id() + 1);
        (p.as_ptr() as usize, p.len(), first, s.len(), h.finish())
    }

    /// Lower bound on how many source siblings `p` must consume: every pattern
    /// node takes exactly one, except `$$$` (any number) and a comma right before a
    /// `$$$` (skipped when that `$$$` is empty).
    fn min_nodes(p: &[PatNode]) -> usize {
        let multi = |n: Option<&PatNode>| matches!(n, Some(PatNode::Meta(s)) if s.multi);
        p.iter()
            .enumerate()
            .filter(|(i, n)| {
                !(multi(Some(n))
                    || matches!(n, PatNode::Leaf { is_comma: true, .. }) && multi(p.get(i + 1)))
            })
            .count()
    }

    fn match_seq(
        p_slice: &[PatNode],
        s_slice: &[Node<'_>],
        src: &str,
        env: &mut BTreeMap<String, String>,
    ) -> bool {
        let steps = MATCH_STEPS.with(|c| {
            let n = c.get().saturating_add(1);
            c.set(n);
            n
        });
        if steps > MATCH_BUDGET {
            return false;
        }
        if p_slice.is_empty() {
            return s_slice.is_empty();
        }
        let key = memo_key(p_slice, s_slice, env);
        let known_fail = MATCH_MEMO.with(|m| m.borrow().contains(&key));
        if known_fail {
            return false;
        }
        let ok = match_seq_inner(p_slice, s_slice, src, env);
        if !ok && MATCH_STEPS.with(|c| c.get()) <= MATCH_BUDGET {
            // Only a genuine failure is remembered, not one cut short by the budget.
            MATCH_MEMO.with(|m| m.borrow_mut().insert(key));
        }
        ok
    }

    fn match_seq_inner(
        p_slice: &[PatNode],
        s_slice: &[Node<'_>],
        src: &str,
        env: &mut BTreeMap<String, String>,
    ) -> bool {
        if let PatNode::Meta(spec) = &p_slice[0] {
            if spec.multi {
                // Never take siblings the rest of the pattern needs.
                let max_take = s_slice.len().saturating_sub(min_nodes(&p_slice[1..]));
                // No env clone per split: bind, recurse, undo. A failed `match_seq`
                // leaves `env` untouched, so undoing our own insert restores it.
                for take in (0..=max_take).rev() {
                    let captured = if take == 0 {
                        ""
                    } else {
                        &src[s_slice[0].start_byte()..s_slice[take - 1].end_byte()]
                    };
                    let inserted = match &spec.name {
                        Some(name) => match env.get(name) {
                            Some(prev) if prev != captured => continue,
                            Some(_) => None,
                            None => {
                                env.insert(name.clone(), captured.to_string());
                                Some(name)
                            }
                        },
                        None => None,
                    };
                    if match_seq(&p_slice[1..], &s_slice[take..], src, env) {
                        return true;
                    }
                    if let Some(name) = inserted {
                        env.remove(name);
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
        // A trailing comma before the closing delimiter is insignificant (`f(a, b,)`,
        // black-formatted calls): skip it where the pattern expects the delimiter.
        if s_slice.len() >= 2
            && s_slice[0].kind() == ","
            && is_closing_delim(s_slice[1])
            && matches!(
                p_slice[0],
                PatNode::Leaf {
                    is_closing: true,
                    ..
                }
            )
        {
            return match_seq(p_slice, &s_slice[1..], src, env);
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

    const SUPPORTED_LANGS: &str =
        "rs, py, js, ts, tsx, go, c, cpp, zig, java, cs, kt, scala, swift, rb, php, bash, lua, hcl, html, css, json, yaml, toml";

    fn resolve_lang(opts: &AstOptions) -> Result<Lang, String> {
        if let Some(raw) = &opts.lang {
            return Lang::parse(raw).ok_or_else(|| {
                format!("unsupported language `{raw}` (supported: {SUPPORTED_LANGS})")
            });
        }
        // Files given by name pick the language (and must agree on it).
        let files: Vec<&std::path::PathBuf> = opts.roots.iter().filter(|r| r.is_file()).collect();
        if let Some(f) = files.iter().find(|f| Lang::from_path(f).is_none()) {
            return Err(format!(
                "can't tell the language of {}: pass `--lang <LANG>` ({SUPPORTED_LANGS})",
                f.display()
            ));
        }
        let mut langs = files.iter().filter_map(|r| Lang::from_path(r));
        if let Some(l) = langs.next() {
            if let Some(other) = langs.find(|o| o.name() != l.name()) {
                return Err(format!(
                    "files in different languages ({} and {}): run one `lancet ast` per language",
                    l.name(),
                    other.name()
                ));
            }
            return Ok(l);
        }
        Err(format!(
            "specify `--lang <LANG>` ({SUPPORTED_LANGS}) when searching a directory"
        ))
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
        /// The rewrite to commit with the others.
        change: Option<FileChange>,
        warning: Option<String>,
        /// Why this file's rewrite cannot be written at all (it would add syntax errors).
        refusal: Option<String>,
    }

    fn process_file(
        target: &Target,
        pat: &CompiledPattern,
        rewrite: Option<&str>,
        allow_syntax_errors: bool,
    ) -> Option<FileResult> {
        // Rewrites read under a shared lock so a concurrent lancet writer can't make
        // the read fail (Windows byte-range locks), and a failed read is reported.
        let read = if rewrite.is_some() {
            edit::read_shared(&target.path)
        } else {
            fs::read(&target.path)
        };
        let bytes = match read {
            Ok(b) => b,
            Err(e) if rewrite.is_some() => {
                return Some(FileResult {
                    parsed: false,
                    matches: Vec::new(),
                    diff: None,
                    change: None,
                    warning: Some(format!("{}: not rewritten, read failed: {e}", target.rel)),
                    refusal: None,
                })
            }
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
                change: None,
                warning: None,
                refusal: None,
            });
        }

        let src = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => return None,
        };

        let tree = parse_src(pat.lang, &src)?;
        let root_kind_id = pat.root_kind_id();
        MATCH_EXHAUSTED.with(|c| c.set(false));
        MATCH_MEMO.with(|m| m.borrow_mut().clear());
        let mut matches = Vec::new();
        let mut cursor = tree.root_node().walk();
        let mut descend = true;
        // A match contains every pattern literal verbatim (leaves compare by text), so
        // subtrees holding no occurrence of the longest one are skipped unvisited.
        let lit_hits: Option<(Vec<usize>, usize)> = pat
            .prefilters
            .first()
            .filter(|_| !may_have_js_unicode_ident)
            .map(|f| (f.find_iter(src.as_bytes()).collect(), f.needle().len()));

        loop {
            let may_match = descend
                && lit_hits.as_ref().is_none_or(|(hits, len)| {
                    let node = cursor.node();
                    let i = hits.partition_point(|&h| h < node.start_byte());
                    i < hits.len() && hits[i] + len <= node.end_byte()
                });
            if may_match {
                let node = cursor.node();
                let kind_ok = match root_kind_id {
                    Some(kid) => node.kind_id() == kid,
                    None => !is_comment(node.kind()),
                };
                if kind_ok {
                    let mut env = BTreeMap::new();
                    MATCH_STEPS.with(|c| c.set(0));
                    let hit = match_node(&pat.root, node, &src, &mut env);
                    if MATCH_STEPS.with(|c| c.get()) > MATCH_BUDGET {
                        MATCH_EXHAUSTED.with(|c| c.set(true));
                    }
                    if hit {
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

        if MATCH_EXHAUSTED.with(|c| c.get()) {
            // Partial matches are reported, but flagged, and never rewritten.
            return Some(FileResult {
                parsed: true,
                matches,
                diff: None,
                change: None,
                refusal: None,
                warning: Some(format!(
                    "{}: pattern too ambiguous (over {MATCH_BUDGET} matching steps at one node); matches in this file are incomplete and it was not rewritten — make the pattern more specific (fewer `$$$`)",
                    target.rel
                )),
            });
        }

        if matches.is_empty() {
            return Some(FileResult {
                parsed: true,
                matches,
                diff: None,
                change: None,
                warning: None,
                refusal: None,
            });
        }

        let mut diff = None;
        let mut change = None;
        let mut refusal = None;
        if rewrite.is_some() {
            let edits: Vec<edit::Edit> = matches
                .iter()
                .filter_map(|m| {
                    Some(edit::Edit {
                        start: m.start_byte,
                        end: m.end_byte,
                        replacement: m.replacement.clone()?,
                    })
                })
                .collect();
            let planned = edit::plan(&src, &edits);
            let new_src = planned.new_src;

            if new_src != src {
                if !allow_syntax_errors {
                    if let Some((b0, b1)) = crate::change::added_syntax_errors(
                        &target.path,
                        &src,
                        &target.path,
                        &new_src,
                    ) {
                        refusal = Some(format!(
                            "{}: rewrite would add {} syntax error(s) ({b0} -> {b1})",
                            target.rel,
                            b1 - b0
                        ));
                    }
                }
                diff = Some(FileDiff {
                    path: target.rel.clone(),
                    rev: edit::rev(src.as_bytes()),
                    replacements: planned.applied,
                    skipped_overlaps: planned.skipped_overlaps,
                    diff: edit::unified_diff(&target.rel, &src, &new_src),
                    applied: false,
                });
                change = Some(FileChange::update(
                    &target.path,
                    target.rel.clone(),
                    src,
                    new_src,
                ));
            }
        }

        Some(FileResult {
            parsed: true,
            matches,
            diff,
            change,
            warning: None,
            refusal,
        })
    }

    pub fn search_ast(opts: &AstOptions) -> AstResponse {
        let missing: Vec<String> = opts
            .roots
            .iter()
            .filter_map(|r| crate::walk::missing_root(r))
            .collect();
        if let Some(msg) = (!missing.is_empty()).then(|| missing.join("; ")) {
            return AstResponse::failure(
                Status::InvalidQuery,
                opts.lang.clone(),
                opts.pattern.clone(),
                msg,
                0.0,
            );
        }
        search_ast_raw(opts)
    }

    fn search_ast_raw(opts: &AstOptions) -> AstResponse {
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

        let req = build_ast_req(&compiled.trigram_literals);
        let filters = Filters {
            exts: lang.extensions().iter().map(|s| (*s).to_string()).collect(),
            paths: opts.paths.clone(),
            ..Default::default()
        };
        let planned = crate::scope::plan(
            &crate::scope::Request {
                roots: &opts.roots,
                walk: &opts.walk,
                index: opts.index,
                index_dir: opts.index_dir.as_deref(),
                shown: crate::scope::Shown::Clean,
            },
            &req,
            &|_| FileFilter::new(&filters),
        );
        let planned = match planned {
            Ok(p) => p,
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
        let (mut warnings, mode, core_stats) = (
            planned.summary.warnings,
            planned.summary.mode,
            planned.summary.stats,
        );
        // Matches and diff headers show each path the way it was given.
        let targets: Vec<Target> = planned
            .files
            .into_iter()
            .map(|c| Target {
                path: c.path,
                rel: c.shown,
            })
            .collect();

        let file_results: Vec<Option<FileResult>> = targets
            .par_iter()
            .map(|t| {
                process_file(
                    t,
                    &compiled,
                    opts.rewrite.as_deref(),
                    opts.allow_syntax_errors,
                )
            })
            .collect();

        let mut files_parsed = 0usize;
        let mut matches = Vec::new();
        let mut diffs = Vec::new();
        let mut changes = Vec::new();
        let mut refusals = Vec::new();
        for res in file_results.into_iter().flatten() {
            files_parsed += res.parsed as usize;
            matches.extend(res.matches);
            diffs.extend(res.diff);
            changes.extend(res.change);
            refusals.extend(res.refusal);
            warnings.extend(res.warning);
        }
        // Rewrites are one change: refused, or written, all together.
        let mut error = None;
        if !refusals.is_empty() {
            error = Some((
                Status::InvalidQuery,
                format!(
                    "{}; nothing written (pass --allow-syntax-errors to write anyway)",
                    refusals.join("; ")
                ),
            ));
        } else if opts.apply && !warnings.is_empty() {
            error = Some((
                Status::InvalidQuery,
                format!("{}; nothing written", warnings.join("; ")),
            ));
        } else if opts.apply {
            match crate::change::commit(&changes) {
                Ok(()) => diffs.iter_mut().for_each(|d| d.applied = true),
                Err(e) => error = Some((e.status, e.message)),
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

        let status = match &error {
            Some((status, _)) => *status,
            None if matched == 0 => Status::Empty,
            None => Status::Complete,
        };
        AstResponse {
            status,
            mode,
            lang: Some(lang_name),
            pattern: opts.pattern.clone(),
            matched,
            returned,
            truncated,
            matches,
            diffs,
            warnings,
            error: error.map(|(_, message)| ErrorInfo { message, pos: None }),
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
            eprintln!("error: {}", err.message);
            return Ok(());
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

    pub fn print_ast_human(resp: &AstResponse, _w: &mut impl Write) -> io::Result<()> {
        if let Some(err) = &resp.error {
            eprintln!("error: {}", err.message);
        }
        Ok(())
    }
}

pub use imp::{print_ast_human, search_ast};

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}
