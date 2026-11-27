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

/// What an identifier occurrence is, where the syntax alone can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdRole {
    Plain,
    /// Rust `.name` / `name:` — a method or field of some type, never a free item.
    Member,
    /// The name of a Rust `fn` outside any `impl` / `trait`.
    FreeFn,
}

#[cfg(feature = "ast")]
mod imp {
    use super::{CodeSpan, IdRole};
    use std::cell::RefCell;
    use std::path::Path;
    use tree_sitter::{Language, Node, Parser, Tree};

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

        pub fn spec(self) -> &'static LangSpec {
            &LANGS[self as usize]
        }

        /// A `--lang` word: a language name, an alias, or an extension.
        pub fn parse(s: &str) -> Option<Self> {
            let w = s.trim().to_ascii_lowercase();
            LANGS
                .iter()
                .find(|l| {
                    l.name == w || l.aliases.contains(&w.as_str()) || l.exts.contains(&w.as_str())
                })
                .map(|l| l.lang)
        }

        /// By extension; an extension two languages share (`.h`) goes to the first.
        pub fn from_path(path: &Path) -> Option<Self> {
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            LANGS
                .iter()
                .find(|l| l.exts.contains(&ext.as_str()))
                .map(|l| l.lang)
        }

        pub fn name(self) -> &'static str {
            self.spec().name
        }

        pub fn extensions(self) -> &'static [&'static str] {
            self.spec().exts
        }

        pub fn language(self) -> Language {
            (self.spec().grammar)()
        }
    }

    /// Everything lancet knows about one language, in one place. Adding a language
    /// is one entry here (plus a `Lang` variant) and its tests.
    pub struct LangSpec {
        pub lang: Lang,
        pub name: &'static str,
        /// `--lang` words besides the name and the extensions.
        pub aliases: &'static [&'static str],
        pub exts: &'static [&'static str],
        pub grammar: fn() -> Language,
        /// Node kinds that are functions (enclosing names, `outline`, `--symbol`).
        pub functions: &'static [&'static str],
        /// Declaration node kind -> the symbol kind lancet reports.
        pub symbols: &'static [(&'static str, &'static str)],
        /// Node kinds whose symbol kind depends on more than the kind.
        pub symbol_if: Option<fn(Node<'_>) -> Option<&'static str>>,
        /// Code an AST pattern is wrapped in until it parses (tried in order).
        pub pattern_wrappers: &'static [(&'static str, &'static str)],
        /// The order to try instead when the pattern ends with `;`.
        pub pattern_wrappers_semi: Option<&'static [(&'static str, &'static str)]>,
        /// The scope rules `rename` and `refs` resolve names with, if modeled.
        pub(crate) scopes: Option<crate::scopes::Family>,
    }

    /// A JS/TS variable bound to a function is a function.
    fn js_symbol(node: Node<'_>) -> Option<&'static str> {
        (node.kind() == "variable_declarator"
            && node
                .child_by_field_name("value")
                .is_some_and(|v| matches!(v.kind(), "arrow_function" | "function_expression")))
        .then_some("function")
    }

    /// A C/C++ struct, class, enum or union is declared where it has a body.
    fn c_symbol(node: Node<'_>) -> Option<&'static str> {
        let kind = match node.kind() {
            "struct_specifier" => "struct",
            "class_specifier" => "class",
            "enum_specifier" => "enum",
            "union_specifier" => "union",
            _ => return None,
        };
        node.child_by_field_name("body").is_some().then_some(kind)
    }

    /// Top-level JSON keys.
    fn json_symbol(node: Node<'_>) -> Option<&'static str> {
        (node.kind() == "pair"
            && node
                .parent()
                .and_then(|p| p.parent())
                .is_some_and(|gp| gp.kind() == "document"))
        .then_some("key")
    }

    /// Top-level TOML keys (keys in tables belong to the table).
    fn toml_symbol(node: Node<'_>) -> Option<&'static str> {
        (node.kind() == "pair" && node.parent().is_some_and(|p| p.kind() == "document"))
            .then_some("key")
    }

    const NO_WRAP: &[(&str, &str)] = &[("", "")];
    const JS_FUNCTIONS: &[&str] = &[
        "function_declaration",
        "method_definition",
        "arrow_function",
    ];
    const JS_SYMBOLS: &[(&str, &str)] = &[
        ("function_declaration", "function"),
        ("method_definition", "method"),
        ("class_declaration", "class"),
        ("interface_declaration", "interface"),
        ("type_alias_declaration", "type"),
        ("enum_declaration", "enum"),
    ];
    const JS_WRAP: &[(&str, &str)] = &[
        ("", ""),
        ("const __lancet_wrap__ = (", ");"),
        ("class __LancetWrap__ { ", " }"),
    ];
    const C_SYMBOLS: &[(&str, &str)] = &[
        ("function_definition", "function"),
        ("namespace_definition", "namespace"),
        ("type_definition", "type"),
        ("alias_declaration", "type"),
    ];

    /// In `Lang` order (checked by a test).
    static LANGS: [LangSpec; 24] = [
        LangSpec {
            lang: Lang::Rust,
            name: "rust",
            aliases: &[],
            exts: &["rs"],
            grammar: || tree_sitter_rust::LANGUAGE.into(),
            functions: &["function_item"],
            symbols: &[
                ("function_item", "function"),
                ("struct_item", "struct"),
                ("enum_item", "enum"),
                ("trait_item", "trait"),
                ("type_item", "type"),
                ("const_item", "const"),
                ("static_item", "static"),
                ("mod_item", "mod"),
            ],
            symbol_if: None,
            pattern_wrappers: &[
                ("fn __lancet_wrap__() { let _ = ", "; }"),
                ("fn __lancet_wrap__() { ", " }"),
                ("", ""),
                ("impl __LancetWrap__ { ", " }"),
                ("type __LancetWrap__ = ", ";"),
            ],
            scopes: Some(crate::scopes::Family::Rust),
            pattern_wrappers_semi: Some(&[
                ("fn __lancet_wrap__() { ", " }"),
                ("fn __lancet_wrap__() { let _ = ", "; }"),
                ("", ""),
                ("impl __LancetWrap__ { ", " }"),
                ("type __LancetWrap__ = ", ";"),
            ]),
        },
        LangSpec {
            lang: Lang::Python,
            name: "python",
            aliases: &[],
            exts: &["py", "pyi"],
            grammar: || tree_sitter_python::LANGUAGE.into(),
            functions: &["function_definition"],
            symbols: &[
                ("function_definition", "function"),
                ("class_definition", "class"),
            ],
            symbol_if: None,
            pattern_wrappers: NO_WRAP,
            scopes: Some(crate::scopes::Family::Python),
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Js,
            name: "javascript",
            aliases: &[],
            exts: &["js", "mjs", "cjs", "jsx"],
            grammar: || tree_sitter_javascript::LANGUAGE.into(),
            functions: JS_FUNCTIONS,
            symbols: JS_SYMBOLS,
            symbol_if: Some(js_symbol),
            pattern_wrappers: JS_WRAP,
            scopes: Some(crate::scopes::Family::Js),
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Ts,
            name: "typescript",
            aliases: &[],
            exts: &["ts", "mts", "cts"],
            grammar: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            functions: JS_FUNCTIONS,
            symbols: JS_SYMBOLS,
            symbol_if: Some(js_symbol),
            pattern_wrappers: JS_WRAP,
            scopes: Some(crate::scopes::Family::Js),
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Tsx,
            name: "tsx",
            aliases: &[],
            exts: &["tsx"],
            grammar: || tree_sitter_typescript::LANGUAGE_TSX.into(),
            functions: JS_FUNCTIONS,
            symbols: JS_SYMBOLS,
            symbol_if: Some(js_symbol),
            pattern_wrappers: JS_WRAP,
            scopes: Some(crate::scopes::Family::Js),
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Go,
            name: "go",
            aliases: &["golang"],
            exts: &["go"],
            grammar: || tree_sitter_go::LANGUAGE.into(),
            functions: &["function_declaration", "method_declaration"],
            symbols: &[
                ("function_declaration", "function"),
                ("method_declaration", "function"),
                ("type_spec", "type"),
            ],
            symbol_if: None,
            pattern_wrappers: &[
                ("package p\nfunc __lancet_wrap__() { _ = ", " }"),
                ("package p\nfunc __lancet_wrap__() {\n", "\n}"),
                ("package p\n", ""),
                ("package p\ntype __LancetWrap__ ", ""),
            ],
            scopes: Some(crate::scopes::Family::Go),
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::C,
            name: "c",
            aliases: &[],
            exts: &["c", "h"],
            grammar: || tree_sitter_c::LANGUAGE.into(),
            functions: &["function_definition"],
            symbols: C_SYMBOLS,
            symbol_if: Some(c_symbol),
            pattern_wrappers: &[
                ("void __lancet_wrap__(void) { int __lv = (", "); }"),
                ("void __lancet_wrap__(void) {\n", "\n}"),
                ("", ""),
            ],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Cpp,
            name: "cpp",
            aliases: &["c++"],
            exts: &["cpp", "cc", "cxx", "hpp", "hh", "hxx", "h"],
            grammar: || tree_sitter_cpp::LANGUAGE.into(),
            functions: &["function_definition"],
            symbols: C_SYMBOLS,
            symbol_if: Some(c_symbol),
            pattern_wrappers: &[
                ("void __lancet_wrap__() { auto __lv = (", "); }"),
                ("void __lancet_wrap__() {\n", "\n}"),
                ("class __LancetWrap__ {\n", "\n};"),
                ("", ""),
            ],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Zig,
            name: "zig",
            aliases: &[],
            exts: &["zig"],
            grammar: || tree_sitter_zig::LANGUAGE.into(),
            functions: &["function_declaration"],
            symbols: &[
                ("function_declaration", "function"),
                ("variable_declaration", "const"),
            ],
            symbol_if: None,
            pattern_wrappers: &[
                ("fn __lancet_wrap__() void { _ = ", "; }"),
                ("fn __lancet_wrap__() void {\n", "\n}"),
                ("", ""),
            ],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Java,
            name: "java",
            aliases: &[],
            exts: &["java"],
            grammar: || tree_sitter_java::LANGUAGE.into(),
            functions: &["method_declaration", "constructor_declaration"],
            symbols: &[
                ("method_declaration", "method"),
                ("constructor_declaration", "method"),
                ("class_declaration", "class"),
                ("interface_declaration", "interface"),
                ("enum_declaration", "enum"),
                ("record_declaration", "record"),
            ],
            symbol_if: None,
            pattern_wrappers: &[
                ("class __LW { Object __lf() { return (", "); } }"),
                ("class __LW { void __lf() {\n", "\n} }"),
                ("class __LW {\n", "\n}"),
                ("", ""),
            ],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::CSharp,
            name: "csharp",
            aliases: &["c#"],
            exts: &["cs"],
            grammar: || tree_sitter_c_sharp::LANGUAGE.into(),
            functions: &[
                "method_declaration",
                "constructor_declaration",
                "local_function_statement",
            ],
            symbols: &[
                ("method_declaration", "method"),
                ("constructor_declaration", "method"),
                ("local_function_statement", "method"),
                ("class_declaration", "class"),
                ("struct_declaration", "struct"),
                ("interface_declaration", "interface"),
                ("enum_declaration", "enum"),
                ("record_declaration", "record"),
                ("namespace_declaration", "namespace"),
                ("file_scoped_namespace_declaration", "namespace"),
            ],
            symbol_if: None,
            pattern_wrappers: &[
                ("class __LW { object __lf() { return (", "); } }"),
                ("class __LW { void __lf() {\n", "\n} }"),
                ("class __LW {\n", "\n}"),
                ("", ""),
            ],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Kotlin,
            name: "kotlin",
            aliases: &[],
            exts: &["kt", "kts"],
            grammar: || tree_sitter_kotlin_ng::LANGUAGE.into(),
            functions: &["function_declaration"],
            symbols: &[
                ("function_declaration", "function"),
                ("class_declaration", "class"),
                ("object_declaration", "object"),
                ("type_alias", "type"),
            ],
            symbol_if: None,
            pattern_wrappers: &[
                ("fun __lw(): Any? = (", ")"),
                ("fun __lw() {\n", "\n}"),
                ("", ""),
            ],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Scala,
            name: "scala",
            aliases: &[],
            exts: &["scala", "sc"],
            grammar: || tree_sitter_scala::LANGUAGE.into(),
            functions: &["function_definition", "function_declaration"],
            symbols: &[
                ("function_definition", "function"),
                ("function_declaration", "function"),
                ("class_definition", "class"),
                ("object_definition", "object"),
                ("trait_definition", "trait"),
                ("enum_definition", "enum"),
                ("type_definition", "type"),
            ],
            symbol_if: None,
            pattern_wrappers: &[
                ("object __LW { def __lf() = (", ") }"),
                ("object __LW {\n", "\n}"),
                ("", ""),
            ],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Swift,
            name: "swift",
            aliases: &[],
            exts: &["swift"],
            grammar: || tree_sitter_swift::LANGUAGE.into(),
            functions: &["function_declaration", "init_declaration"],
            symbols: &[
                ("function_declaration", "function"),
                ("init_declaration", "function"),
                ("class_declaration", "type"),
                ("protocol_declaration", "protocol"),
                ("typealias_declaration", "type"),
            ],
            symbol_if: None,
            pattern_wrappers: &[
                ("", ""),
                ("func __lw() -> Any { return (", ") }"),
                ("struct __LW {\n", "\n}"),
            ],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Ruby,
            name: "ruby",
            aliases: &[],
            exts: &["rb"],
            grammar: || tree_sitter_ruby::LANGUAGE.into(),
            functions: &["method", "singleton_method"],
            symbols: &[
                ("method", "method"),
                ("singleton_method", "method"),
                ("class", "class"),
                ("module", "module"),
            ],
            symbol_if: None,
            pattern_wrappers: NO_WRAP,
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Php,
            name: "php",
            aliases: &[],
            exts: &["php"],
            grammar: || tree_sitter_php::LANGUAGE_PHP.into(),
            functions: &["function_definition", "method_declaration"],
            symbols: &[
                ("function_definition", "function"),
                ("method_declaration", "method"),
                ("class_declaration", "class"),
                ("interface_declaration", "interface"),
                ("trait_declaration", "trait"),
                ("enum_declaration", "enum"),
            ],
            symbol_if: None,
            pattern_wrappers: &[
                ("<?php\n$__lv = (", ");"),
                ("<?php\n", ";"),
                ("<?php\n", ""),
                ("<?php\nclass __LW {\n", "\n}"),
            ],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Bash,
            name: "bash",
            aliases: &["shell"],
            exts: &["sh", "bash", "zsh"],
            grammar: || tree_sitter_bash::LANGUAGE.into(),
            functions: &["function_definition"],
            symbols: &[("function_definition", "function")],
            symbol_if: None,
            pattern_wrappers: NO_WRAP,
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Lua,
            name: "lua",
            aliases: &[],
            exts: &["lua"],
            grammar: || tree_sitter_lua::LANGUAGE.into(),
            functions: &["function_declaration", "function_definition"],
            symbols: &[
                ("function_declaration", "function"),
                ("function_definition", "function"),
            ],
            symbol_if: None,
            pattern_wrappers: &[("local __lv = (", ")"), ("", "")],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Hcl,
            name: "hcl",
            aliases: &["terraform"],
            exts: &["hcl", "tf", "tfvars"],
            grammar: || tree_sitter_hcl::LANGUAGE.into(),
            functions: &[],
            symbols: &[("block", "block"), ("attribute", "attr")],
            symbol_if: None,
            pattern_wrappers: &[("", ""), ("__lancet_attr = ", "")],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Html,
            name: "html",
            aliases: &[],
            exts: &["html", "htm"],
            grammar: || tree_sitter_html::LANGUAGE.into(),
            functions: &[],
            symbols: &[],
            symbol_if: None,
            pattern_wrappers: NO_WRAP,
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Css,
            name: "css",
            aliases: &[],
            exts: &["css"],
            grammar: || tree_sitter_css::LANGUAGE.into(),
            functions: &[],
            symbols: &[("rule_set", "rule")],
            symbol_if: None,
            pattern_wrappers: &[("", ""), (".__lw { ", " }")],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Json,
            name: "json",
            aliases: &[],
            exts: &["json", "jsonc"],
            grammar: || tree_sitter_json::LANGUAGE.into(),
            functions: &[],
            symbols: &[],
            symbol_if: Some(json_symbol),
            pattern_wrappers: &[("", ""), ("{ ", " }")],
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Yaml,
            name: "yaml",
            aliases: &[],
            exts: &["yaml", "yml"],
            grammar: || tree_sitter_yaml::LANGUAGE.into(),
            functions: &[],
            symbols: &[("block_mapping_pair", "key")],
            symbol_if: None,
            pattern_wrappers: NO_WRAP,
            scopes: None,
            pattern_wrappers_semi: None,
        },
        LangSpec {
            lang: Lang::Toml,
            name: "toml",
            aliases: &[],
            exts: &["toml"],
            grammar: || tree_sitter_toml_ng::LANGUAGE.into(),
            functions: &[],
            symbols: &[("table", "table"), ("table_array_element", "table")],
            symbol_if: Some(toml_symbol),
            pattern_wrappers: NO_WRAP,
            scopes: None,
            pattern_wrappers_semi: None,
        },
    ];

    pub fn supports_path(path: &Path) -> bool {
        Lang::from_path(path).is_some()
    }

    thread_local! {
        static PARSER: RefCell<(Option<Lang>, Parser)> = RefCell::new((None, Parser::new()));
    }

    /// Parses `src` as `lang` with this thread's parser (one per thread, reused).
    pub fn parse_tree(lang: Lang, src: &[u8]) -> Option<Tree> {
        PARSER.with(|cell| {
            let mut st = cell.borrow_mut();
            if st.0 != Some(lang) {
                st.1.set_language(&lang.language()).ok()?;
                st.0 = Some(lang);
            }
            st.1.parse(src, None)
        })
    }

    /// A parsed source file. Parse once, then read every structural view from it
    /// (symbols, function spans, identifiers, syntax errors, scopes).
    pub struct Syntax {
        pub lang: Lang,
        pub tree: Tree,
    }

    /// `None` for a file whose language lancet doesn't parse.
    pub fn parse(path: &Path, src: &[u8]) -> Option<Syntax> {
        let lang = Lang::from_path(path)?;
        Some(Syntax {
            lang,
            tree: parse_tree(lang, src)?,
        })
    }

    impl Syntax {
        /// Number of Tree-sitter ERROR / MISSING nodes, or `None` for unsupported files.
        pub fn errors(&self) -> usize {
            let root = self.tree.root_node();
            if !root.has_error() {
                return 0;
            }
            let mut n = 0;
            visit_dfs(root, |node| {
                n += (node.is_error() || node.is_missing()) as usize
            });
            n
        }
        pub fn functions(&self, src: &[u8]) -> Vec<CodeSpan> {
            let lang = self.lang;
            let root = self.tree.root_node();
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
        }
        pub fn symbols(&self, src: &[u8]) -> Vec<CodeSpan> {
            let lang = self.lang;
            let root = self.tree.root_node();
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
        }
        /// One parse: the file's declarations, and how many times each code token
        /// (not in a comment or string) whose text passes `keep` occurs.
        pub fn symbols_and_token_counts(
            &self,
            src: &[u8],
            keep: &dyn Fn(&str) -> bool,
        ) -> (Vec<CodeSpan>, std::collections::HashMap<String, usize>) {
            let lang = self.lang;
            let root = self.tree.root_node();
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
        }
        /// Return `(start_byte, end_byte)` of all non-comment, non-string AST leaf tokens matching `ident`.
        pub fn identifiers(&self, src: &[u8], ident: &str) -> Vec<(usize, usize, IdRole)> {
            let target = ident.as_bytes();
            let lang = self.lang;
            let root = self.tree.root_node();
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
                if is_comment_or_string(node) {
                    return;
                }
                let role = match (lang, node.parent()) {
                    (Lang::Rust, _) if node.kind() == "field_identifier" => IdRole::Member,
                    // `x.name` / `name: value` inside a macro's token tree
                    (Lang::Rust, Some(p))
                        if p.kind() == "token_tree"
                            && (node.prev_sibling().is_some_and(|b| b.kind() == ".")
                                || node.next_sibling().is_some_and(|b| b.kind() == ":")) =>
                    {
                        IdRole::Member
                    }
                    (Lang::Rust, Some(p))
                        if p.kind() == "function_item"
                            && p.child_by_field_name("name")
                                .is_some_and(|n| n.id() == node.id()) =>
                    {
                        let mut a = p.parent();
                        while let Some(x) = a {
                            if matches!(x.kind(), "impl_item" | "trait_item") {
                                break;
                            }
                            a = x.parent();
                        }
                        if a.is_none() {
                            IdRole::FreeFn
                        } else {
                            IdRole::Plain
                        }
                    }
                    _ => IdRole::Plain,
                };
                out.push((s, e, role));
            });
            out.sort_unstable_by_key(|x| (x.0, x.1));
            out
        }
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
        lang.spec().functions.contains(&k)
    }

    fn symbol_kind(lang: Lang, node: Node<'_>) -> Option<&'static str> {
        let spec = lang.spec();
        let k = node.kind();
        spec.symbols
            .iter()
            .find(|(kind, _)| *kind == k)
            .map(|&(_, sym)| sym)
            .or_else(|| spec.symbol_if.and_then(|f| f(node)))
    }

    /// Number of Tree-sitter ERROR / MISSING nodes, or `None` for unsupported files.
    pub fn syntax_errors(path: &Path, src: &[u8]) -> Option<usize> {
        parse(path, src).map(|s| s.errors())
    }

    pub fn extract_functions(path: &Path, src: &[u8]) -> Option<Vec<CodeSpan>> {
        parse(path, src).map(|s| s.functions(src))
    }

    pub fn extract_symbols(path: &Path, src: &[u8]) -> Option<Vec<CodeSpan>> {
        parse(path, src).map(|s| s.symbols(src))
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
        parse(path, src).map(|s| s.symbols_and_token_counts(src, keep))
    }
}

#[cfg(all(test, feature = "ast"))]
mod tests {
    use super::imp::Lang;
    use std::path::Path;

    #[test]
    fn lang_table_matches_the_enum_and_old_names() {
        for (i, l) in Lang::ALL.iter().enumerate() {
            assert_eq!(l.spec().lang, *l, "LANGS[{i}] is out of order");
            assert_eq!(Lang::parse(l.name()), Some(*l));
            assert!(l.language().node_kind_count() > 0);
        }
        // `--lang` words and extensions that worked before the table.
        for (w, l) in [
            ("rs", Lang::Rust),
            ("pyi", Lang::Python),
            ("jsx", Lang::Js),
            ("mts", Lang::Ts),
            ("golang", Lang::Go),
            ("c++", Lang::Cpp),
            ("hpp", Lang::Cpp),
            ("c#", Lang::CSharp),
            ("kts", Lang::Kotlin),
            ("sc", Lang::Scala),
            ("shell", Lang::Bash),
            ("zsh", Lang::Bash),
            ("terraform", Lang::Hcl),
            ("tfvars", Lang::Hcl),
            ("htm", Lang::Html),
            ("jsonc", Lang::Json),
            ("yml", Lang::Yaml),
            ("TOML", Lang::Toml),
        ] {
            assert_eq!(Lang::parse(w), Some(l), "{w}");
        }
        assert_eq!(Lang::parse("cobol"), None);
        // `.h` is C (C++ lists it too, for `--lang cpp` directory searches).
        assert_eq!(Lang::from_path(Path::new("x.h")), Some(Lang::C));
        assert!(Lang::Cpp.extensions().contains(&"h"));
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
    /// Without the `ast` feature nothing is parsed, so no `Syntax` value exists.
    pub enum Syntax {}
    pub fn parse(_path: &Path, _src: &[u8]) -> Option<Syntax> {
        None
    }
    impl Syntax {
        pub fn functions(&self, _src: &[u8]) -> Vec<CodeSpan> {
            match *self {}
        }
        pub fn symbols(&self, _src: &[u8]) -> Vec<CodeSpan> {
            match *self {}
        }
        pub fn identifiers(&self, _src: &[u8], _ident: &str) -> Vec<(usize, usize, super::IdRole)> {
            match *self {}
        }
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

pub use imp::{
    extract_functions, extract_symbols, parse, supports_path, symbols_and_token_counts,
    syntax_errors, Syntax, ENABLED,
};
#[cfg(feature = "ast")]
pub use imp::{parse_tree, Lang};
