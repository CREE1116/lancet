//! Polyglot Tree-sitter code structure extraction (`in function`, `symbol(...)`,
//! `lancet outline`, `lancet read --symbol`, `lancet refs`) across 24 languages.
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
        C,
        Cpp,
        Zig,
        Java,
        CSharp,
        Kotlin,
        Scala,
        Swift,
        Ruby,
        Php,
        Bash,
        Lua,
        Hcl,
        Html,
        Css,
        Json,
        Yaml,
        Toml,
    }

    impl Lang {
        pub const ALL: [Lang; 24] = [
            Lang::Rust,
            Lang::Python,
            Lang::Js,
            Lang::Ts,
            Lang::Tsx,
            Lang::Go,
            Lang::C,
            Lang::Cpp,
            Lang::Zig,
            Lang::Java,
            Lang::CSharp,
            Lang::Kotlin,
            Lang::Scala,
            Lang::Swift,
            Lang::Ruby,
            Lang::Php,
            Lang::Bash,
            Lang::Lua,
            Lang::Hcl,
            Lang::Html,
            Lang::Css,
            Lang::Json,
            Lang::Yaml,
            Lang::Toml,
        ];

        pub fn parse(s: &str) -> Option<Self> {
            match s.trim().to_ascii_lowercase().as_str() {
                "rs" | "rust" => Some(Lang::Rust),
                "py" | "python" | "pyi" => Some(Lang::Python),
                "js" | "javascript" | "mjs" | "cjs" | "jsx" => Some(Lang::Js),
                "ts" | "typescript" | "mts" | "cts" => Some(Lang::Ts),
                "tsx" => Some(Lang::Tsx),
                "go" | "golang" => Some(Lang::Go),
                "c" => Some(Lang::C),
                "cpp" | "c++" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => Some(Lang::Cpp),
                "zig" => Some(Lang::Zig),
                "java" => Some(Lang::Java),
                "cs" | "csharp" | "c#" => Some(Lang::CSharp),
                "kt" | "kts" | "kotlin" => Some(Lang::Kotlin),
                "scala" | "sc" => Some(Lang::Scala),
                "swift" => Some(Lang::Swift),
                "rb" | "ruby" => Some(Lang::Ruby),
                "php" => Some(Lang::Php),
                "sh" | "bash" | "zsh" | "shell" => Some(Lang::Bash),
                "lua" => Some(Lang::Lua),
                "hcl" | "tf" | "tfvars" | "terraform" => Some(Lang::Hcl),
                "html" | "htm" => Some(Lang::Html),
                "css" => Some(Lang::Css),
                "json" | "jsonc" => Some(Lang::Json),
                "yaml" | "yml" => Some(Lang::Yaml),
                "toml" => Some(Lang::Toml),
                _ => None,
            }
        }

        pub fn from_path(path: &Path) -> Option<Self> {
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            match ext.as_str() {
                "rs" => Some(Lang::Rust),
                "py" | "pyi" => Some(Lang::Python),
                "js" | "mjs" | "cjs" | "jsx" => Some(Lang::Js),
                "ts" | "mts" | "cts" => Some(Lang::Ts),
                "tsx" => Some(Lang::Tsx),
                "go" => Some(Lang::Go),
                "c" | "h" => Some(Lang::C),
                "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some(Lang::Cpp),
                "zig" => Some(Lang::Zig),
                "java" => Some(Lang::Java),
                "cs" => Some(Lang::CSharp),
                "kt" | "kts" => Some(Lang::Kotlin),
                "scala" | "sc" => Some(Lang::Scala),
                "swift" => Some(Lang::Swift),
                "rb" => Some(Lang::Ruby),
                "php" => Some(Lang::Php),
                "sh" | "bash" | "zsh" => Some(Lang::Bash),
                "lua" => Some(Lang::Lua),
                "hcl" | "tf" | "tfvars" => Some(Lang::Hcl),
                "html" | "htm" => Some(Lang::Html),
                "css" => Some(Lang::Css),
                "json" | "jsonc" => Some(Lang::Json),
                "yaml" | "yml" => Some(Lang::Yaml),
                "toml" => Some(Lang::Toml),
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
                Lang::C => "c",
                Lang::Cpp => "cpp",
                Lang::Zig => "zig",
                Lang::Java => "java",
                Lang::CSharp => "csharp",
                Lang::Kotlin => "kotlin",
                Lang::Scala => "scala",
                Lang::Swift => "swift",
                Lang::Ruby => "ruby",
                Lang::Php => "php",
                Lang::Bash => "bash",
                Lang::Lua => "lua",
                Lang::Hcl => "hcl",
                Lang::Html => "html",
                Lang::Css => "css",
                Lang::Json => "json",
                Lang::Yaml => "yaml",
                Lang::Toml => "toml",
            }
        }

        pub fn extensions(self) -> &'static [&'static str] {
            match self {
                Lang::Rust => &["rs"],
                Lang::Python => &["py", "pyi"],
                Lang::Js => &["js", "mjs", "cjs", "jsx"],
                Lang::Ts => &["ts", "mts", "cts"],
                Lang::Tsx => &["tsx"],
                Lang::Go => &["go"],
                Lang::C => &["c", "h"],
                Lang::Cpp => &["cpp", "cc", "cxx", "hpp", "hh", "hxx", "h"],
                Lang::Zig => &["zig"],
                Lang::Java => &["java"],
                Lang::CSharp => &["cs"],
                Lang::Kotlin => &["kt", "kts"],
                Lang::Scala => &["scala", "sc"],
                Lang::Swift => &["swift"],
                Lang::Ruby => &["rb"],
                Lang::Php => &["php"],
                Lang::Bash => &["sh", "bash", "zsh"],
                Lang::Lua => &["lua"],
                Lang::Hcl => &["hcl", "tf", "tfvars"],
                Lang::Html => &["html", "htm"],
                Lang::Css => &["css"],
                Lang::Json => &["json", "jsonc"],
                Lang::Yaml => &["yaml", "yml"],
                Lang::Toml => &["toml"],
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
                Lang::C => tree_sitter_c::LANGUAGE.into(),
                Lang::Cpp => tree_sitter_cpp::LANGUAGE.into(),
                Lang::Zig => tree_sitter_zig::LANGUAGE.into(),
                Lang::Java => tree_sitter_java::LANGUAGE.into(),
                Lang::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
                Lang::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
                Lang::Scala => tree_sitter_scala::LANGUAGE.into(),
                Lang::Swift => tree_sitter_swift::LANGUAGE.into(),
                Lang::Ruby => tree_sitter_ruby::LANGUAGE.into(),
                Lang::Php => tree_sitter_php::LANGUAGE_PHP.into(),
                Lang::Bash => tree_sitter_bash::LANGUAGE.into(),
                Lang::Lua => tree_sitter_lua::LANGUAGE.into(),
                Lang::Hcl => tree_sitter_hcl::LANGUAGE.into(),
                Lang::Html => tree_sitter_html::LANGUAGE.into(),
                Lang::Css => tree_sitter_css::LANGUAGE.into(),
                Lang::Json => tree_sitter_json::LANGUAGE.into(),
                Lang::Yaml => tree_sitter_yaml::LANGUAGE.into(),
                Lang::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
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

    fn node_text_clean(node: Node<'_>, src: &[u8]) -> Option<(String, usize, usize)> {
        let s = std::str::from_utf8(&src[node.start_byte()..node.end_byte()]).ok()?;
        let trimmed = s.trim().trim_matches(['"', '\'']);
        (!trimmed.is_empty()).then(|| (trimmed.to_string(), node.start_byte(), node.end_byte()))
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

    /// Recursively unwrap C/C++ declarators (`*foo(...)`, `Class::method(...)`) to the identifier token.
    fn declarator_ident(node: Node<'_>, src: &[u8]) -> Option<(String, usize, usize)> {
        let k = node.kind();
        if matches!(
            k,
            "identifier"
                | "field_identifier"
                | "type_identifier"
                | "destructor_name"
                | "operator_name"
        ) {
            return node_text_clean(node, src);
        }
        if k == "qualified_identifier" || k == "scoped_identifier" {
            if let Some(name_n) = node.child_by_field_name("name") {
                return declarator_ident(name_n, src);
            }
            return node_text_clean(node, src);
        }
        if let Some(inner) = node.child_by_field_name("declarator") {
            return declarator_ident(inner, src);
        }
        let mut cur = node.walk();
        for c in node.named_children(&mut cur) {
            if let Some(found) = declarator_ident(c, src) {
                return Some(found);
            }
        }
        None
    }

    /// Universal declaration name resolver across 24 grammars.
    fn resolve_decl_name(lang: Lang, node: Node<'_>, src: &[u8]) -> Option<(String, usize, usize)> {
        if let Some(n) = node.child_by_field_name("name") {
            return node_text_clean(n, src);
        }
        if matches!(lang, Lang::C | Lang::Cpp) {
            if let Some(d) = node.child_by_field_name("declarator") {
                return declarator_ident(d, src);
            }
        }
        if let Some(k) = node.child_by_field_name("key") {
            return node_text_clean(k, src);
        }
        // Fallback: inspect direct named children for identifier-like nodes (used by Kotlin, Zig, Swift, HCL, etc.)
        let mut cur = node.walk();
        let children: Vec<Node<'_>> = node.named_children(&mut cur).collect();
        if lang == Lang::Hcl && node.kind() == "block" {
            // `resource "aws_s3_bucket" "main"` -> join type + labels cleanly
            let parts: Vec<(String, usize, usize)> = children
                .iter()
                .filter(|c| matches!(c.kind(), "identifier" | "string_lit"))
                .filter_map(|c| node_text_clean(*c, src))
                .collect();
            if let Some(first) = parts.first() {
                let joined = parts
                    .iter()
                    .map(|p| p.0.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let last_end = parts.last().unwrap().2;
                return Some((joined, first.1, last_end));
            }
        }
        for c in children {
            if matches!(
                c.kind(),
                "identifier"
                    | "simple_identifier"
                    | "type_identifier"
                    | "constant"
                    | "word"
                    | "function_name"
                    | "property_identifier"
                    | "bare_key"
                    | "dotted_key"
                    | "selectors"
            ) {
                return node_text_clean(c, src);
            }
            if c.kind() == "fn_proto" {
                if let Some(res) = resolve_decl_name(lang, c, src) {
                    return Some(res);
                }
            }
        }
        None
    }

    fn is_function_node(lang: Lang, k: &str) -> bool {
        match lang {
            Lang::Rust => k == "function_item",
            Lang::Python | Lang::C | Lang::Cpp | Lang::Bash => k == "function_definition",
            Lang::Js | Lang::Ts | Lang::Tsx => {
                matches!(
                    k,
                    "function_declaration" | "method_definition" | "arrow_function"
                )
            }
            Lang::Go => matches!(k, "function_declaration" | "method_declaration"),
            Lang::Zig => k == "function_declaration",
            Lang::Java => matches!(k, "method_declaration" | "constructor_declaration"),
            Lang::CSharp => matches!(
                k,
                "method_declaration" | "constructor_declaration" | "local_function_statement"
            ),
            Lang::Kotlin => k == "function_declaration",
            Lang::Scala => matches!(k, "function_definition" | "function_declaration"),
            Lang::Swift => matches!(k, "function_declaration" | "init_declaration"),
            Lang::Ruby => matches!(k, "method" | "singleton_method"),
            Lang::Php => matches!(k, "function_definition" | "method_declaration"),
            Lang::Lua => matches!(k, "function_declaration" | "function_definition"),
            Lang::Hcl | Lang::Html | Lang::Css | Lang::Json | Lang::Yaml | Lang::Toml => false,
        }
    }

    fn symbol_kind(lang: Lang, node: Node<'_>) -> Option<&'static str> {
        let k = node.kind();
        match lang {
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
            Lang::C | Lang::Cpp => match k {
                "function_definition" => Some("function"),
                "struct_specifier" if node.child_by_field_name("body").is_some() => Some("struct"),
                "class_specifier" if node.child_by_field_name("body").is_some() => Some("class"),
                "enum_specifier" if node.child_by_field_name("body").is_some() => Some("enum"),
                "union_specifier" if node.child_by_field_name("body").is_some() => Some("union"),
                "namespace_definition" => Some("namespace"),
                "type_definition" | "alias_declaration" => Some("type"),
                _ => None,
            },
            Lang::Zig => match k {
                "function_declaration" => Some("function"),
                "variable_declaration" => Some("const"),
                _ => None,
            },
            Lang::Java => match k {
                "method_declaration" | "constructor_declaration" => Some("method"),
                "class_declaration" => Some("class"),
                "interface_declaration" => Some("interface"),
                "enum_declaration" => Some("enum"),
                "record_declaration" => Some("record"),
                _ => None,
            },
            Lang::CSharp => match k {
                "method_declaration" | "constructor_declaration" | "local_function_statement" => {
                    Some("method")
                }
                "class_declaration" => Some("class"),
                "struct_declaration" => Some("struct"),
                "interface_declaration" => Some("interface"),
                "enum_declaration" => Some("enum"),
                "record_declaration" => Some("record"),
                "namespace_declaration" | "file_scoped_namespace_declaration" => Some("namespace"),
                _ => None,
            },
            Lang::Kotlin => match k {
                "function_declaration" => Some("function"),
                "class_declaration" => Some("class"),
                "object_declaration" => Some("object"),
                "type_alias" => Some("type"),
                _ => None,
            },
            Lang::Scala => match k {
                "function_definition" | "function_declaration" => Some("function"),
                "class_definition" => Some("class"),
                "object_definition" => Some("object"),
                "trait_definition" => Some("trait"),
                "enum_definition" => Some("enum"),
                "type_definition" => Some("type"),
                _ => None,
            },
            Lang::Swift => match k {
                "function_declaration" | "init_declaration" => Some("function"),
                "class_declaration" => Some("type"),
                "protocol_declaration" => Some("protocol"),
                "typealias_declaration" => Some("type"),
                _ => None,
            },
            Lang::Ruby => match k {
                "method" | "singleton_method" => Some("method"),
                "class" => Some("class"),
                "module" => Some("module"),
                _ => None,
            },
            Lang::Php => match k {
                "function_definition" => Some("function"),
                "method_declaration" => Some("method"),
                "class_declaration" => Some("class"),
                "interface_declaration" => Some("interface"),
                "trait_declaration" => Some("trait"),
                "enum_declaration" => Some("enum"),
                _ => None,
            },
            Lang::Bash => (k == "function_definition").then_some("function"),
            Lang::Lua => {
                matches!(k, "function_declaration" | "function_definition").then_some("function")
            }
            Lang::Hcl => match k {
                "block" => Some("block"),
                "attribute" => Some("attr"),
                _ => None,
            },
            Lang::Css => (k == "rule_set").then_some("rule"),
            Lang::Json => {
                let top_level = node
                    .parent()
                    .and_then(|p| p.parent())
                    .is_some_and(|gp| gp.kind() == "document");
                (k == "pair" && top_level).then_some("key")
            }
            Lang::Yaml => (k == "block_mapping_pair").then_some("key"),
            Lang::Toml => match k {
                "table" | "table_array_element" => Some("table"),
                "pair" if node.parent().is_some_and(|p| p.kind() == "document") => Some("key"),
                _ => None,
            },
            Lang::Html => None,
        }
    }

    /// Number of Tree-sitter ERROR / MISSING nodes, or `None` for unsupported files.
    pub fn syntax_errors(path: &Path, src: &[u8]) -> Option<usize> {
        with_tree(path, src, |_, root| {
            if !root.has_error() {
                return 0;
            }
            let mut n = 0;
            visit_dfs(root, |node| {
                n += (node.is_error() || node.is_missing()) as usize
            });
            n
        })
    }

    pub fn extract_functions(path: &Path, src: &[u8]) -> Option<Vec<CodeSpan>> {
        with_tree(path, src, |lang, root| {
            let mut out = Vec::new();
            visit_dfs(root, |node| {
                let k = node.kind();
                if !is_function_node(lang, k) {
                    return;
                }
                let (name, ns, ne) = if k == "arrow_function" {
                    if let Some(parent) = node.parent() {
                        if parent.kind() == "variable_declarator" {
                            if let Some((nm, s, e)) = resolve_decl_name(lang, parent, src) {
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
                } else if let Some((nm, s, e)) = resolve_decl_name(lang, node, src) {
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
                let Some(kind) = symbol_kind(lang, node) else {
                    return;
                };
                let Some((nm, ns, ne)) = resolve_decl_name(lang, node, src) else {
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
            if k.ends_with("comment")
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

    /// One parse: the file's declarations, and how many times each code token
    /// (not in a comment or string) whose text passes `keep` occurs.
    pub fn symbols_and_token_counts(
        path: &Path,
        src: &[u8],
        keep: &dyn Fn(&str) -> bool,
    ) -> Option<(Vec<CodeSpan>, std::collections::HashMap<String, usize>)> {
        with_tree(path, src, |lang, root| {
            let mut syms = Vec::new();
            let mut counts = std::collections::HashMap::new();
            visit_dfs(root, |node| {
                if node.child_count() == 0 {
                    if let Ok(t) = std::str::from_utf8(&src[node.byte_range()]) {
                        if keep(t) && !is_comment_or_string(node) {
                            *counts.entry(t.to_string()).or_insert(0) += 1;
                        }
                    }
                    return;
                }
                let Some(kind) = symbol_kind(lang, node) else {
                    return;
                };
                if let Some((nm, ns, ne)) = resolve_decl_name(lang, node, src) {
                    syms.push(CodeSpan {
                        name: Some(nm),
                        kind,
                        start: node.start_byte(),
                        end: node.end_byte(),
                        name_start: ns,
                        name_end: ne,
                    });
                }
            });
            (syms, counts)
        })
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
    pub fn syntax_errors(_path: &Path, _src: &[u8]) -> Option<usize> {
        None
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
    #[allow(clippy::type_complexity)]
    pub fn symbols_and_token_counts(
        _path: &Path,
        _src: &[u8],
        _keep: &dyn Fn(&str) -> bool,
    ) -> Option<(Vec<CodeSpan>, std::collections::HashMap<String, usize>)> {
        None
    }
}

#[cfg(feature = "ast")]
pub use imp::Lang;
pub use imp::{
    extract_functions, extract_symbols, find_identifier_spans, supports_path,
    symbols_and_token_counts, syntax_errors, ENABLED,
};
