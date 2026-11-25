//! Strict recursive-descent parser for the readable grammar (see
//! `docs/grammar.md`). Unknown operators, stages, filters, or trailing tokens
//! are errors — never silently ignored — so a model gets precise feedback.

use super::ast::*;
use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParseError {
    /// Byte offset into the query string.
    pub pos: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.pos)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Str(String),
    Word(String),
    LParen,
    RParen,
    Comma,
    Pipe,
    Colon,
    Eof,
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Str(s) => format!("string {}", quote(s)),
        Tok::Word(w) => format!("'{w}'"),
        Tok::LParen => "'('".into(),
        Tok::RParen => "')'".into(),
        Tok::Comma => "','".into(),
        Tok::Pipe => "'|'".into(),
        Tok::Colon => "':'".into(),
        Tok::Eof => "end of query".into(),
    }
}

const KEYWORDS: &[&str] = &["find", "in", "and", "or", "not"];
const FUNCS: &[&str] = &["regex", "fuzzy", "near", "before", "symbol"];
const SCOPE_HINT: &str = "line, block, file, function, type:..., ext:..., path:...";
const STAGE_HINT: &str =
    "limit N, count, files, group file, expand parent, expand context:N, sort count [asc|desc], terms [top:N], budget tokens:N";

fn is_keyword(w: &str) -> bool {
    KEYWORDS.iter().any(|k| w.eq_ignore_ascii_case(k))
}

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, ParseError> {
    let cs: Vec<(usize, char)> = src.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let (pos, c) = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let simple = match c {
            '(' => Some(Tok::LParen),
            ')' => Some(Tok::RParen),
            ',' => Some(Tok::Comma),
            '|' => Some(Tok::Pipe),
            ':' => Some(Tok::Colon),
            _ => None,
        };
        if let Some(t) = simple {
            out.push((t, pos));
            i += 1;
            continue;
        }
        if c == '"' {
            let mut s = String::new();
            let mut closed = false;
            i += 1;
            while i < cs.len() {
                let c2 = cs[i].1;
                if c2 == '\\' && i + 1 < cs.len() && matches!(cs[i + 1].1, '"' | '\\') {
                    s.push(cs[i + 1].1);
                    i += 2;
                    continue;
                }
                if c2 == '"' {
                    closed = true;
                    i += 1;
                    break;
                }
                s.push(c2);
                i += 1;
            }
            if !closed {
                return Err(ParseError {
                    pos,
                    message: "unterminated string".into(),
                });
            }
            out.push((Tok::Str(s), pos));
            continue;
        }
        let mut w = String::new();
        while i < cs.len() {
            let c2 = cs[i].1;
            if c2.is_whitespace() || "\"(),|:".contains(c2) {
                break;
            }
            w.push(c2);
            i += 1;
        }
        out.push((Tok::Word(w), pos));
    }
    out.push((Tok::Eof, src.len()));
    Ok(out)
}

type R<T> = Result<T, ParseError>;

struct P {
    toks: Vec<(Tok, usize)>,
    i: usize,
}

impl P {
    fn peek(&self) -> &Tok {
        &self.toks[self.i].0
    }
    fn peek2(&self) -> &Tok {
        &self.toks[(self.i + 1).min(self.toks.len() - 1)].0
    }
    fn pos(&self) -> usize {
        self.toks[self.i].1
    }
    fn bump(&mut self) -> Tok {
        let t = self.toks[self.i].0.clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        t
    }
    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Tok::Word(w) if w.eq_ignore_ascii_case(kw))
    }
    fn err<T>(&self, msg: impl Into<String>) -> R<T> {
        Err(ParseError {
            pos: self.pos(),
            message: msg.into(),
        })
    }
    fn expect(&mut self, t: Tok, what: &str) -> R<()> {
        if *self.peek() == t {
            self.bump();
            Ok(())
        } else {
            self.err(format!("expected {what}, found {}", describe(self.peek())))
        }
    }
    fn num(&mut self) -> R<u32> {
        if let Tok::Word(w) = self.peek() {
            if let Ok(n) = w.parse::<u32>() {
                self.bump();
                return Ok(n);
            }
        }
        self.err(format!(
            "expected a number, found {}",
            describe(self.peek())
        ))
    }

    fn query(&mut self) -> R<Query> {
        if self.is_kw("find") {
            self.bump();
        }
        if *self.peek() == Tok::Eof {
            return self.err("empty query: expected a search term");
        }
        let expr = self.or()?;
        let mut q = Query::new(expr);
        let mut unit_set = false;
        while self.is_kw("in") {
            self.bump();
            self.scope(&mut q, &mut unit_set)?;
        }
        while *self.peek() == Tok::Pipe {
            self.bump();
            self.stage(&mut q)?;
        }
        if *self.peek() != Tok::Eof {
            return self.err(format!(
                "unexpected {}; expected AND, OR, NOT, 'in <scope>', or '| <stage>'",
                describe(self.peek())
            ));
        }
        q.normalize();
        Ok(q)
    }

    fn or(&mut self) -> R<Expr> {
        let mut args = vec![self.and()?];
        while self.is_kw("or") {
            self.bump();
            args.push(self.and()?);
        }
        Ok(if args.len() == 1 {
            args.pop().unwrap()
        } else {
            Expr::Or { args }
        })
    }

    fn starts_unary(&self) -> bool {
        match self.peek() {
            Tok::Str(_) | Tok::LParen => true,
            Tok::Word(w) => !is_keyword(w) || w.eq_ignore_ascii_case("not"),
            _ => false,
        }
    }

    /// `a AND b`, `a b` (implicit AND) and `a NOT b` (== `a AND NOT b`).
    fn and(&mut self) -> R<Expr> {
        let mut args = vec![self.unary()?];
        loop {
            if self.is_kw("and") {
                self.bump();
                args.push(self.unary()?);
            } else if self.starts_unary() {
                args.push(self.unary()?);
            } else {
                break;
            }
        }
        Ok(if args.len() == 1 {
            args.pop().unwrap()
        } else {
            Expr::And { args }
        })
    }

    fn unary(&mut self) -> R<Expr> {
        if self.is_kw("not") {
            self.bump();
            return Ok(Expr::Not {
                arg: Box::new(self.unary()?),
            });
        }
        self.primary()
    }

    fn primary(&mut self) -> R<Expr> {
        match self.peek().clone() {
            Tok::Str(s) => {
                if s.is_empty() {
                    return self.err("empty string is not a valid search term");
                }
                self.bump();
                Ok(Expr::lit(s))
            }
            Tok::LParen => {
                self.bump();
                let e = self.or()?;
                self.expect(Tok::RParen, "')'")?;
                Ok(e)
            }
            Tok::Word(w) => {
                let lw = w.to_ascii_lowercase();
                if lw == "symbol" && *self.peek2() == Tok::Colon {
                    self.bump();
                    self.bump();
                    return Ok(Expr::Match {
                        m: self.symbol_name()?,
                    });
                }
                if FUNCS.contains(&lw.as_str()) && *self.peek2() == Tok::LParen {
                    return self.func(&lw);
                }
                if is_keyword(&w) {
                    return self.err(format!("unexpected keyword '{w}'; expected a search term"));
                }
                self.bump();
                Ok(Expr::lit(w))
            }
            t => self.err(format!("expected a search term, found {}", describe(&t))),
        }
    }

    fn func(&mut self, name: &str) -> R<Expr> {
        self.bump(); // name
        self.expect(Tok::LParen, "'('")?;
        let e = match name {
            "regex" => Expr::Match {
                m: self.regex_body()?,
            },
            "fuzzy" => Expr::Match {
                m: self.fuzzy_body()?,
            },
            "symbol" => Expr::Match {
                m: self.symbol_name()?,
            },
            _ => {
                let a = self.matcher_arg()?;
                self.expect(Tok::Comma, "','")?;
                let b = self.matcher_arg()?;
                self.expect(Tok::Comma, "',' followed by a word distance")?;
                let dist = self.num()?;
                Expr::Near {
                    a,
                    b,
                    dist,
                    ordered: name == "before",
                }
            }
        };
        self.expect(Tok::RParen, "')'")?;
        Ok(e)
    }

    fn matcher_arg(&mut self) -> R<Matcher> {
        match self.peek().clone() {
            Tok::Str(s) => {
                if s.is_empty() {
                    return self.err("empty string is not a valid search term");
                }
                self.bump();
                Ok(Matcher::Lit { text: s })
            }
            Tok::Word(w) => {
                let lw = w.to_ascii_lowercase();
                if matches!(lw.as_str(), "regex" | "fuzzy" | "symbol")
                    && *self.peek2() == Tok::LParen
                {
                    self.bump();
                    self.bump();
                    let m = match lw.as_str() {
                        "regex" => self.regex_body()?,
                        "fuzzy" => self.fuzzy_body()?,
                        _ => self.symbol_name()?,
                    };
                    self.expect(Tok::RParen, "')'")?;
                    Ok(m)
                } else if lw == "symbol" && *self.peek2() == Tok::Colon {
                    self.bump();
                    self.bump();
                    self.symbol_name()
                } else if is_keyword(&w) || FUNCS.contains(&lw.as_str()) {
                    self.err(format!(
                        "'{w}' is not allowed here; expected a string, regex(...), fuzzy(...), or symbol(...)"
                    ))
                } else {
                    self.bump();
                    Ok(Matcher::Lit { text: w })
                }
            }
            t => self.err(format!(
                "expected a string, regex(...) or fuzzy(...), found {}",
                describe(&t)
            )),
        }
    }

    fn text_arg(&mut self) -> R<String> {
        match self.peek().clone() {
            Tok::Str(s) | Tok::Word(s) if !s.is_empty() => {
                self.bump();
                Ok(s)
            }
            t => self.err(format!(
                "expected a non-empty string, found {}",
                describe(&t)
            )),
        }
    }

    fn symbol_name(&mut self) -> R<Matcher> {
        let pos = self.pos();
        let name = self.text_arg()?;
        if name.chars().any(char::is_whitespace) {
            return Err(ParseError {
                pos,
                message: "symbol name cannot contain whitespace".into(),
            });
        }
        Ok(Matcher::Symbol { name })
    }

    fn regex_body(&mut self) -> R<Matcher> {
        let pos = self.pos();
        let pattern = self.text_arg()?;
        if let Err(e) = regex_syntax::ParserBuilder::new()
            .multi_line(true)
            .build()
            .parse(&pattern)
        {
            let msg = e.to_string();
            let last = msg
                .lines()
                .rfind(|l| !l.trim().is_empty())
                .unwrap_or("")
                .trim()
                .to_string();
            return Err(ParseError {
                pos,
                message: format!("invalid regex: {last}"),
            });
        }
        Ok(Matcher::Regex { pattern })
    }

    fn fuzzy_body(&mut self) -> R<Matcher> {
        let pos = self.pos();
        let text = self.text_arg()?;
        if !text.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(ParseError {
                pos,
                message: "fuzzy() takes a single word (letters, digits, '_')".into(),
            });
        }
        let k = if *self.peek() == Tok::Comma {
            self.bump();
            let kpos = self.pos();
            let k = self.num()?;
            if k > 3 {
                return Err(ParseError {
                    pos: kpos,
                    message: "fuzzy distance must be between 0 and 3".into(),
                });
            }
            k
        } else {
            1
        };
        Ok(Matcher::Fuzzy { text, k })
    }

    fn scope(&mut self, q: &mut Query, unit_set: &mut bool) -> R<()> {
        let mut n = 0;
        while let Tok::Word(w) = self.peek().clone() {
            let lw = w.to_ascii_lowercase();
            if *self.peek2() == Tok::Colon {
                if !matches!(lw.as_str(), "type" | "ext" | "path") {
                    return self.err(format!(
                        "unknown filter '{w}:'; expected type:, ext: or path:"
                    ));
                }
                self.bump();
                self.bump();
                for (v, pos) in self.values()? {
                    match lw.as_str() {
                        "type" => match FileType::parse(&v) {
                            Some(t) => q.filters.types.push(t),
                            None => {
                                let names: Vec<&str> =
                                    FileType::ALL.iter().map(|t| t.name()).collect();
                                return Err(ParseError {
                                    pos,
                                    message: format!(
                                        "unknown type '{v}'; expected one of {}",
                                        names.join(", ")
                                    ),
                                });
                            }
                        },
                        "ext" => {
                            let e = v.trim_start_matches('.').to_ascii_lowercase();
                            if e.is_empty() {
                                return Err(ParseError {
                                    pos,
                                    message: "empty extension".into(),
                                });
                            }
                            q.filters.exts.push(e);
                        }
                        _ => {
                            if let Err(e) = globset::Glob::new(&v) {
                                return Err(ParseError {
                                    pos,
                                    message: format!("invalid path glob: {e}"),
                                });
                            }
                            q.filters.paths.push(v);
                        }
                    }
                }
            } else if let Some(u) = Unit::parse(&lw) {
                if *unit_set && q.unit != u {
                    return self.err(format!(
                        "conflicting units: '{}' and '{}'",
                        q.unit.name(),
                        u.name()
                    ));
                }
                q.unit = u;
                *unit_set = true;
                self.bump();
            } else {
                break;
            }
            n += 1;
        }
        if n == 0 {
            return self.err(format!(
                "expected a scope after 'in' ({SCOPE_HINT}), found {}",
                describe(self.peek())
            ));
        }
        Ok(())
    }

    fn values(&mut self) -> R<Vec<(String, usize)>> {
        if *self.peek() == Tok::LParen {
            self.bump();
            let mut v = Vec::new();
            loop {
                v.push(self.value()?);
                if *self.peek() == Tok::Comma {
                    self.bump();
                    continue;
                }
                self.expect(Tok::RParen, "')' or ','")?;
                break;
            }
            Ok(v)
        } else {
            Ok(vec![self.value()?])
        }
    }

    fn value(&mut self) -> R<(String, usize)> {
        let pos = self.pos();
        match self.peek().clone() {
            Tok::Str(s) | Tok::Word(s) => {
                self.bump();
                Ok((s, pos))
            }
            t => self.err(format!("expected a value, found {}", describe(&t))),
        }
    }

    fn stage(&mut self, q: &mut Query) -> R<()> {
        let Tok::Word(w) = self.peek().clone() else {
            return self.err(format!("expected a stage after '|' ({STAGE_HINT})"));
        };
        let pos = self.pos();
        self.bump();
        match w.to_ascii_lowercase().as_str() {
            "limit" => {
                let n = self.num()?;
                if n == 0 {
                    return Err(ParseError {
                        pos,
                        message: "limit must be at least 1".into(),
                    });
                }
                q.limit = Some(n as usize);
            }
            "count" => q.count = true,
            "files" => q.files_only = true,
            "group" => {
                if self.is_kw("file") {
                    self.bump();
                    q.group_file = true;
                } else {
                    return self.err("expected 'file' after 'group'");
                }
            }
            "expand" => {
                if self.is_kw("parent") || self.is_kw("unit") {
                    self.bump();
                    q.expand_unit = true;
                } else if self.is_kw("context") {
                    self.bump();
                    self.expect(Tok::Colon, "':' after 'context'")?;
                    q.context = self.num()?.min(50);
                } else {
                    return self.err("expected 'parent' or 'context:N' after 'expand'");
                }
            }
            "sort" => {
                if self.is_kw("count") {
                    self.bump();
                    if self.is_kw("asc") {
                        self.bump();
                        q.sort = SortOrder::CountAsc;
                    } else if self.is_kw("desc") {
                        self.bump();
                        q.sort = SortOrder::CountDesc;
                    } else {
                        q.sort = SortOrder::CountDesc;
                    }
                } else if self.is_kw("path") {
                    self.bump();
                    q.sort = SortOrder::Path;
                } else {
                    return self.err("expected 'count' or 'path' after 'sort'");
                }
            }
            "terms" => {
                let mut top = 15usize;
                if self.is_kw("top") {
                    self.bump();
                    self.expect(Tok::Colon, "':' after 'top'")?;
                    let tpos = self.pos();
                    let n = self.num()? as usize;
                    if n == 0 || n > 100 {
                        return Err(ParseError {
                            pos: tpos,
                            message: "terms top must be between 1 and 100".into(),
                        });
                    }
                    top = n;
                }
                q.terms_top = Some(top);
            }
            "budget" => {
                if self.is_kw("tokens") {
                    self.bump();
                    self.expect(Tok::Colon, "':' after 'tokens'")?;
                }
                let bpos = self.pos();
                let n = self.num()? as usize;
                if n == 0 {
                    return Err(ParseError {
                        pos: bpos,
                        message: "budget must be at least 1 token".into(),
                    });
                }
                q.budget_tokens = Some(n);
            }
            _ => {
                return Err(ParseError {
                    pos,
                    message: format!("unknown stage '{w}'; expected {STAGE_HINT}"),
                });
            }
        }
        Ok(())
    }
}

/// Parse and normalize a query.
pub fn parse(src: &str) -> Result<Query, ParseError> {
    let toks = lex(src)?;
    P { toks, i: 0 }.query()
}
