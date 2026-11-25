//! Tree-sitter based code structure extraction (`in function`, `symbol(...)`).
//! Enabled by the default `ast` Cargo feature; compile with
//! `--no-default-features` for a small pure-Rust core binary.

#[derive(Debug, Clone)]
pub struct CodeSpan {
    pub name: Option<String>,
    pub kind: &'static str,
    /// Full node byte range (e.g., entire function definition).
    pub start: usize,
    pub end: usize,
    /// Identifier token byte range (lies on the declaration line).
    pub name_start: usize,
    pub name_end: usize,
}

#[cfg(feature = "ast")]
mod imp {
    use super::CodeSpan;
    use std::cell::RefCell;
    use std::path::Path;
    use tree_sitter::{Language, Node, Parser};

    pub const ENABLED: bool = true;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Lang {
        Rust,
        Python,
        Js,
        Ts,
        Tsx,
        Go,
    }

    impl Lang {
        pub fn parse(s: &str) -> Option<Self> {
            match s.to_ascii_lowercase().as_str() {
                "rs" | "rust" => Some(Lang::Rust),
                "py" | "python" | "pyi" => Some(Lang::Python),
                "js" | "javascript" | "mjs" | "cjs" | "jsx" => Some(Lang::Js),
                "ts" | "typescript" => Some(Lang::Ts),
                "tsx" => Some(Lang::Tsx),
                "go" | "golang" => Some(Lang::Go),
                _ => None,
            }
        }

        pub fn from_path(path: &Path) -> Option<Self> {
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            match ext.as_str() {
                "rs" => Some(Lang::Rust),
                "py" | "pyi" => Some(Lang::Python),
                "js" | "mjs" | "cjs" | "jsx" => Some(Lang::Js),
                "ts" => Some(Lang::Ts),
                "tsx" => Some(Lang::Tsx),
                "go" => Some(Lang::Go),
                _ => None,
            }
        }

        pub fn name(self) -> &'static str {
            match self {
                Lang::Rust => "rust",
                Lang::Python => "python",
                Lang::Js => "javascript",
                Lang::Ts => "typescript",
                Lang::Tsx => "tsx",
                Lang::Go => "go",
            }
        }

        pub fn extensions(self) -> &'static [&'static str] {
            match self {
                Lang::Rust => &["rs"],
                Lang::Python => &["py", "pyi"],
                Lang::Js => &["js", "mjs", "cjs", "jsx"],
                Lang::Ts => &["ts"],
                Lang::Tsx => &["tsx"],
                Lang::Go => &["go"],
            }
        }

        pub fn language(self) -> Language {
            match self {
                Lang::Rust => tree_sitter_rust::LANGUAGE.into(),
                Lang::Python => tree_sitter_python::LANGUAGE.into(),
                Lang::Js => tree_sitter_javascript::LANGUAGE.into(),
                Lang::Ts => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                Lang::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
                Lang::Go => tree_sitter_go::LANGUAGE.into(),
            }
        }
    }

    pub fn supports_path(path: &Path) -> bool {
        Lang::from_path(path).is_some()
    }

    thread_local! {
        static PARSER: RefCell<(Option<Lang>, Parser)> = RefCell::new((None, Parser::new()));
    }

    fn with_tree<T>(path: &Path, src: &[u8], f: impl FnOnce(Lang, Node<'_>) -> T) -> Option<T> {
        let lang = Lang::from_path(path)?;
        PARSER.with(|cell| {
            let mut st = cell.borrow_mut();
            if st.0 != Some(lang) {
                st.1.set_language(&lang.language()).ok()?;
                st.0 = Some(lang);
            }
            let tree = st.1.parse(src, None)?;
            Some(f(lang, tree.root_node()))
        })
    }

    fn node_text(node: Node<'_>, src: &[u8]) -> Option<String> {
        let s = std::str::from_utf8(&src[node.start_byte()..node.end_byte()]).ok()?;
        (!s.is_empty()).then(|| s.to_string())
    }

    fn visit_dfs(root: Node<'_>, mut f: impl FnMut(Node<'_>)) {
        let mut cursor = root.walk();
        let mut descend = true;
        loop {
            if descend {
                f(cursor.node());
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
    }

    fn field_name(node: Node<'_>, src: &[u8]) -> Option<(String, usize, usize)> {
        let n = node.child_by_field_name("name")?;
        let text = node_text(n, src)?;
        Some((text, n.start_byte(), n.end_byte()))
    }

    pub fn extract_functions(path: &Path, src: &[u8]) -> Option<Vec<CodeSpan>> {
        with_tree(path, src, |lang, root| {
            let mut out = Vec::new();
            visit_dfs(root, |node| {
                let k = node.kind();
                let matched = match lang {
                    Lang::Rust => k == "function_item",
                    Lang::Python => k == "function_definition",
                    Lang::Js | Lang::Ts | Lang::Tsx => {
                        matches!(
                            k,
                            "function_declaration" | "method_definition" | "arrow_function"
                        )
                    }
                    Lang::Go => matches!(k, "function_declaration" | "method_declaration"),
                };
                if !matched {
                    return;
                }
                let (name, ns, ne) = if k == "arrow_function" {
                    if let Some(parent) = node.parent() {
                        if parent.kind() == "variable_declarator" {
                            if let Some((nm, s, e)) = field_name(parent, src) {
                                (Some(nm), s, e)
                            } else {
                                (None, node.start_byte(), node.start_byte())
                            }
                        } else {
                            (None, node.start_byte(), node.start_byte())
                        }
                    } else {
                        (None, node.start_byte(), node.start_byte())
                    }
                } else if let Some((nm, s, e)) = field_name(node, src) {
                    (Some(nm), s, e)
                } else {
                    (None, node.start_byte(), node.start_byte())
                };
                out.push(CodeSpan {
                    name,
                    kind: "function",
                    start: node.start_byte(),
                    end: node.end_byte(),
                    name_start: ns,
                    name_end: ne,
                });
            });
            out.sort_by_key(|s| (s.start, s.end));
            out
        })
    }

    pub fn extract_symbols(path: &Path, src: &[u8]) -> Option<Vec<CodeSpan>> {
        with_tree(path, src, |lang, root| {
            let mut out = Vec::new();
            visit_dfs(root, |node| {
                let k = node.kind();
                let sym_kind: Option<&'static str> = match lang {
                    Lang::Rust => match k {
                        "function_item" => Some("function"),
                        "struct_item" => Some("struct"),
                        "enum_item" => Some("enum"),
                        "trait_item" => Some("trait"),
                        "type_item" => Some("type"),
                        "const_item" => Some("const"),
                        "static_item" => Some("static"),
                        "mod_item" => Some("mod"),
                        _ => None,
                    },
                    Lang::Python => match k {
                        "function_definition" => Some("function"),
                        "class_definition" => Some("class"),
                        _ => None,
                    },
                    Lang::Js | Lang::Ts | Lang::Tsx => match k {
                        "function_declaration" => Some("function"),
                        "method_definition" => Some("method"),
                        "class_declaration" => Some("class"),
                        "interface_declaration" => Some("interface"),
                        "type_alias_declaration" => Some("type"),
                        "enum_declaration" => Some("enum"),
                        "variable_declarator" => {
                            let is_fn = node.child_by_field_name("value").is_some_and(|v| {
                                matches!(v.kind(), "arrow_function" | "function_expression")
                            });
                            is_fn.then_some("function")
                        }
                        _ => None,
                    },
                    Lang::Go => match k {
                        "function_declaration" | "method_declaration" => Some("function"),
                        "type_spec" => Some("type"),
                        _ => None,
                    },
                };
                let Some(kind) = sym_kind else { return };
                let Some((nm, ns, ne)) = field_name(node, src) else {
                    return;
                };
                out.push(CodeSpan {
                    name: Some(nm),
                    kind,
                    start: node.start_byte(),
                    end: node.end_byte(),
                    name_start: ns,
                    name_end: ne,
                });
            });
            out.sort_by_key(|s| (s.name_start, s.name_end));
            out
        })
    }

    fn is_comment_or_string(node: Node<'_>) -> bool {
        let mut cur = Some(node);
        while let Some(n) = cur {
            let k = n.kind();
            if matches!(k, "comment" | "line_comment" | "block_comment")
                || k.contains("string")
                || k.contains("char_literal")
                || k == "rune_literal"
            {
                return true;
            }
            cur = n.parent();
        }
        false
    }

    /// Return `(start_byte, end_byte)` of all non-comment, non-string AST leaf tokens matching `ident`.
    pub fn find_identifier_spans(
        path: &Path,
        src: &[u8],
        ident: &str,
    ) -> Option<Vec<(usize, usize)>> {
        let target = ident.as_bytes();
        with_tree(path, src, |_lang, root| {
            let mut out = Vec::new();
            visit_dfs(root, |node| {
                if node.child_count() != 0 {
                    return;
                }
                let s = node.start_byte();
                let e = node.end_byte();
                if e.saturating_sub(s) != target.len() || &src[s..e] != target {
                    return;
                }
                if !is_comment_or_string(node) {
                    out.push((s, e));
                }
            });
            out.sort_unstable();
            out
        })
    }
}

#[cfg(not(feature = "ast"))]
mod imp {
    use super::CodeSpan;
    use std::path::Path;

    pub const ENABLED: bool = false;
    pub fn supports_path(_path: &Path) -> bool {
        false
    }
    pub fn extract_functions(_path: &Path, _src: &[u8]) -> Option<Vec<CodeSpan>> {
        None
    }
    pub fn extract_symbols(_path: &Path, _src: &[u8]) -> Option<Vec<CodeSpan>> {
        None
    }
    pub fn find_identifier_spans(
        _path: &Path,
        _src: &[u8],
        _ident: &str,
    ) -> Option<Vec<(usize, usize)>> {
        None
    }
}

#[cfg(feature = "ast")]
pub use imp::Lang;
pub use imp::{extract_functions, extract_symbols, find_identifier_spans, supports_path, ENABLED};
