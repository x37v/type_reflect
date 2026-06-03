use std::iter::Peekable;

use proc_macro2::*;
use quote::*;
use syn::{parse_str, Lit};

use std::collections::HashSet;

struct ExprSubstitution {
    name: String,
    expr: TokenStream,
}

impl ToTokens for ExprSubstitution {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let name = Ident::new(self.name.as_str(), Span::call_site());
        let expr = &self.expr;
        tokens.extend(quote! {
            #name = #expr
        })
    }
}

struct RepetitionSubstitution {
    name: String,
    iter_var: Ident,
    inner_format: String,
    inner_subs: Vec<ExprSubstitution>,
    separator: String,
}

impl ToTokens for RepetitionSubstitution {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let name = Ident::new(self.name.as_str(), Span::call_site());
        let iter_var = &self.iter_var;
        let inner_format = &self.inner_format;
        let separator = &self.separator;
        let inner_subs = &self.inner_subs;
        tokens.extend(quote! {
            #name = {
                let mut __result = ::std::vec::Vec::<::std::string::String>::new();
                for #iter_var in #iter_var.into_iter() {
                    __result.push(format!(#inner_format, #(#inner_subs,)*));
                }
                __result.join(#separator)
            }
        })
    }
}

enum Substitution {
    Expr(ExprSubstitution),
    Repetition(RepetitionSubstitution),
}

impl ToTokens for Substitution {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Substitution::Expr(e) => e.to_tokens(tokens),
            Substitution::Repetition(r) => r.to_tokens(tokens),
        }
    }
}

pub struct ParseContext {
    id: String,
    subgroup_index: u32,
    token_iter: Peekable<token_stream::IntoIter>,
    string_val: String,
    ident_substituions: HashSet<Ident>,
    expr_substituions: Vec<Substitution>,
}

impl ParseContext {
    /// Initialize the parser, with a given ID
    /// and the input token stream
    pub fn new(id: String, tokens: TokenStream) -> Self {
        Self {
            id,
            subgroup_index: 0,
            token_iter: tokens.into_iter().peekable(),
            string_val: String::new(),
            ident_substituions: HashSet::new(),
            expr_substituions: vec![],
        }
    }

    /// Parse the input tokens until finished
    pub fn parse(&mut self) {
        while self.consume_next() {}
    }

    /// Return the format string for this parser
    pub fn format_string(&self) -> &String {
        &self.string_val
    }

    pub fn substitution_mappings(&self) -> TokenStream {
        let subsitutions = &self.expr_substituions;
        quote! {
            #(#subsitutions,)*
        }
    }

    fn create_expr_substitution(&mut self, expr: TokenStream) -> String {
        let name = format!("_expr_sub_{}_{}", self.id, self.expr_substituions.len());
        self.expr_substituions.push(Substitution::Expr(ExprSubstitution {
            name: name.clone(),
            expr,
        }));
        name
    }

    fn merge_args(&mut self, other: ParseContext) {
        self.ident_substituions.extend(other.ident_substituions);
        self.expr_substituions.extend(other.expr_substituions);
    }

    fn push_back_str(&mut self, s: &str) {
        self.string_val.push_str(format!("{}", s).as_str());
    }

    fn push_back_token(&mut self, token: TokenTree) {
        self.string_val.push_str(format!("{}", token).as_str());
    }

    fn push_back_literal(&mut self, lit: Literal) {
        let Ok(Lit::Str(_)) = parse_str::<Lit>(lit.to_string().as_str()) else {
            self.string_val.push_str(lit.to_string().as_str());
            return;
        };
        let str = lit.to_string();
        let str = str.replace("{", "{{").replace("}", "}}");
        self.string_val.push_str(str.as_str());
    }

    fn next(&mut self) -> Option<TokenTree> {
        self.token_iter.next()
    }

    fn peek_next(&mut self) -> Option<&TokenTree> {
        self.token_iter.peek()
    }

    fn consume_next(&mut self) -> bool {
        let Some(token) = self.next() else {
            return false;
        };
        match token {
            TokenTree::Group(group) => self.consume_group(group),
            TokenTree::Punct(punct) => self.consume_punct(punct),
            TokenTree::Literal(lit) => self.push_back_literal(lit),
            token => self.push_back_token(token),
        };
        self.push_back_str(" ");
        true
    }

    fn create_subcontext(&mut self, tokens: TokenStream) -> ParseContext {
        let context = ParseContext::new(format!("{}{}", self.id, self.subgroup_index), tokens);
        self.subgroup_index += 1;
        context
    }

    fn consume_group(&mut self, group: Group) {
        let inner = group.stream();

        let mut contens = self.create_subcontext(inner);
        contens.parse();

        match group.delimiter() {
            Delimiter::Parenthesis => {
                self.push_back_str(format!("({})", contens.string_val).as_str())
            }
            Delimiter::Brace => {
                self.push_back_str(format!("{{{{ {} }}}}", contens.string_val).as_str())
            }
            Delimiter::Bracket => self.push_back_str(format!("[{}]", contens.string_val).as_str()),
            Delimiter::None => self.push_back_str(contens.string_val.as_str()),
        }

        self.merge_args(contens);
    }

    fn consume_punct(&mut self, punct: Punct) {
        match punct.as_char() {
            '#' => self.consume_hash_substitution(punct),
            _ => self.consume_punct_series(punct),
        }
    }

    fn consume_punct_series(&mut self, inital: Punct) {
        let mut result: String = format!("{}", inital.as_char());
        let mut current = inital;
        while let Spacing::Joint = current.spacing() {
            if let Some(TokenTree::Punct(punct)) = self.peek_next() {
                if punct.as_char() == '#' {
                    break;
                }
            }

            let Some(TokenTree::Punct(punct)) = self.next() else {
                panic!("Spacing::Joint should guarantee a Punct here");
            };
            result.push(punct.as_char());
            current = punct;
        }
        self.push_back_str(result.as_str());
    }

    fn consume_hash_substitution(&mut self, hash: Punct) {
        let Some(token) = self.peek_next() else {
            self.consume_punct_series(hash);
            return;
        };
        let token = token.clone();
        match token {
            // ## escape: emit a literal '#' followed by any immediately-following ident
            TokenTree::Punct(ref p) if p.as_char() == '#' => {
                let _ = self.next(); // consume the second '#'
                let mut literal = "#".to_string();
                if let Some(TokenTree::Ident(ident)) = self.peek_next().cloned() {
                    let _ = self.next();
                    literal.push_str(&ident.to_string());
                }
                // Push as a static fragment — no trailing space, handled by caller's push_back_str
                self.push_back_str(&literal);
            }
            // #![doc = "..."] inner doc comment (//! or /*! */)
            TokenTree::Punct(ref p) if p.as_char() == '!' => {
                let _ = self.next(); // consume '!'
                if let Some(TokenTree::Group(g)) = self.peek_next().cloned() {
                    if let Some(text) = Self::parse_doc_group(&g) {
                        let _ = self.next(); // consume the group
                        self.push_back_comment(&text);
                        return;
                    }
                }
                self.push_back_str("#");
                self.consume_punct_series(Punct::new('!', Spacing::Alone));
            }
            TokenTree::Group(group) => {
                let _ = self.next();
                self.consume_group_hash_substitution(group)
            }
            TokenTree::Ident(ident) => {
                let _ = self.next();
                self.consume_ident_hash_substitution(ident)
            }
            TokenTree::Literal(lit) => self.consume_literal_hash_substitution(lit),
            _ => self.consume_punct_series(hash),
        }
    }

    fn consume_group_hash_substitution(&mut self, group: Group) {
        // Only paren groups can be repetition; brace/bracket fall through to expr substitution
        if group.delimiter() != Delimiter::Parenthesis {
            // #[doc = "..."] outer doc comment (/// or /** */)
            if let Some(text) = Self::parse_doc_group(&group) {
                self.push_back_comment(&text);
                return;
            }
            let name = self.create_expr_substitution(group.stream());
            self.push_back_str(format!("{{{}}}", name).as_str());
            return;
        }

        // Peek ahead to decide: repetition or plain expr substitution
        let separator: Option<String> = match self.peek_next().cloned() {
            Some(TokenTree::Punct(ref p)) if p.as_char() == '*' => {
                let _ = self.next(); // consume '*'
                Some(String::new())
            }
            Some(TokenTree::Punct(ref p)) if p.as_char() != '*' => {
                let sep_char = p.as_char();
                let _ = self.next(); // consume separator punct
                // Expect '*' next
                match self.peek_next() {
                    Some(TokenTree::Punct(ref star)) if star.as_char() == '*' => {
                        let _ = self.next(); // consume '*'
                        Some(sep_char.to_string())
                    }
                    _ => {
                        // Not a repetition — treat as plain expr substitution
                        None
                    }
                }
            }
            Some(TokenTree::Literal(ref lit)) => {
                let lit_str = lit.to_string();
                // Check if it looks like a string literal (starts with '"')
                if lit_str.starts_with('"') {
                    let lit_clone = lit.clone();
                    let _ = self.next(); // consume the literal
                    // Expect '*' next
                    match self.peek_next() {
                        Some(TokenTree::Punct(ref star)) if star.as_char() == '*' => {
                            let _ = self.next(); // consume '*'
                            // Parse the string literal value
                            if let Ok(Lit::Str(s)) = parse_str::<Lit>(&lit_clone.to_string()) {
                                Some(s.value())
                            } else {
                                Some(lit_clone.to_string())
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        match separator {
            Some(sep) => self.consume_repetition(group, sep),
            None => {
                let name = self.create_expr_substitution(group.stream());
                self.push_back_str(format!("{{{}}}", name).as_str());
            }
        }
    }

    fn consume_repetition(&mut self, group: Group, separator: String) {
        // Parse the inner body of the repetition group into its own sub-context
        let sub_id = format!("{}{}", self.id, self.subgroup_index);
        self.subgroup_index += 1;

        let mut sub = ParseContext::new(sub_id.clone(), group.stream());
        sub.parse();

        // The iterator variable is the first ident substitution found in the body
        let iter_var = sub
            .ident_substituions
            .iter()
            .next()
            .cloned()
            .unwrap_or_else(|| Ident::new("__item", Span::call_site()));

        // Build the inner format string and inner expr substitutions
        let inner_format = sub.string_val.trim().to_string();
        let inner_subs: Vec<ExprSubstitution> = sub
            .expr_substituions
            .into_iter()
            .filter_map(|s| match s {
                Substitution::Expr(e) => Some(e),
                Substitution::Repetition(_) => None,
            })
            .collect();

        let name = format!("_expr_sub_{}_{}", self.id, self.expr_substituions.len());
        self.expr_substituions.push(Substitution::Repetition(RepetitionSubstitution {
            name: name.clone(),
            iter_var,
            inner_format,
            inner_subs,
            separator,
        }));
        self.push_back_str(format!("{{{}}}", name).as_str());
    }

    fn consume_ident_hash_substitution(&mut self, ident: Ident) {
        self.push_back_str(format!("{{{}}}", ident).as_str());
        self.ident_substituions.insert(ident.clone());
    }

    fn consume_literal_hash_substitution(&mut self, lit: Literal) {
        // Parse a raw string literal
        let Ok(Lit::Str(lit_str)) = parse_str::<Lit>(lit.to_string().as_str()) else {
            return;
        };
        // Consume the string literal
        let _consumed = self.next();
        let literal = lit_str.value();

        let mut components: Vec<&str> = literal.split('#').collect();

        if let Some(first) = components.first() {
            if !first.is_empty() {
                self.push_back_literal_component(first.to_string());
            }
            components.remove(0);
        }

        while !components.is_empty() {
            let lit = components.remove(0);
            self.consume_hashed_literal_component(lit.to_string())
        }

        // let literal = literal.replace("{", "{{");
        // let literal = literal.replace("}", "}}");
        // self.push_back_str(literal.as_str());
    }

    fn push_back_literal_component(&mut self, lit: String) {
        let lit = lit.replace("{", "{{").replace("}", "}}");
        self.push_back_str(lit.as_str());
    }

    /// If `group` is `[ doc = "..." ]`, return the decoded literal value
    /// (still including the lexer's leading space). Returns None for any other
    /// attribute shape (e.g. `#[derive(..)]`, `#[doc(hidden)]`) so those fall
    /// through to the existing expr-substitution behavior.
    fn parse_doc_group(group: &Group) -> Option<String> {
        if group.delimiter() != Delimiter::Bracket {
            return None;
        }
        let mut it = group.stream().into_iter();
        match it.next() {
            Some(TokenTree::Ident(id)) if id == "doc" => {}
            _ => return None,
        }
        match it.next() {
            Some(TokenTree::Punct(p)) if p.as_char() == '=' => {}
            _ => return None,
        }
        let lit = match it.next() {
            Some(TokenTree::Literal(l)) => l,
            _ => return None,
        };
        if it.next().is_some() {
            return None; // exactly 3 tokens
        }
        match parse_str::<Lit>(&lit.to_string()) {
            Ok(Lit::Str(s)) => Some(s.value()),
            _ => None,
        }
    }

    /// Trim trailing spaces from `string_val`; if the result is non-empty and
    /// doesn't end in `\n`, append one. Guarantees a comment starts on its own
    /// line (`consume_next` appends a trailing space after every token).
    fn ensure_newline(&mut self) {
        while self.string_val.ends_with(' ') {
            self.string_val.pop();
        }
        if !self.string_val.is_empty() && !self.string_val.ends_with('\n') {
            self.string_val.push('\n');
        }
    }

    /// Emit a TypeScript comment from decoded doc text. Default is `//` line
    /// comment(s); a leading `doc` marker selects a JSDoc block. Always starts
    /// and ends on its own line so following code is never commented out.
    fn push_back_comment(&mut self, raw: &str) {
        self.ensure_newline();
        match strip_jsdoc_marker(raw) {
            Some(content) => self.push_back_jsdoc(content),
            None => self.push_back_line_comment(raw),
        }
        self.string_val.push('\n');
    }

    fn push_back_line_comment(&mut self, raw: &str) {
        for (i, line) in raw.split('\n').enumerate() {
            if i > 0 {
                self.string_val.push('\n');
            }
            self.string_val.push_str("// ");
            self.string_val.push_str(&escape_braces(clean_doc_line(line)));
        }
    }

    fn push_back_jsdoc(&mut self, raw: &str) {
        self.string_val.push_str("/**\n");
        for line in raw.split('\n') {
            let l = escape_terminator(&escape_braces(clean_doc_line(line)));
            if l.is_empty() {
                self.string_val.push_str(" *\n");
            } else {
                self.string_val.push_str(&format!(" * {}\n", l));
            }
        }
        self.string_val.push_str(" */");
    }

    fn consume_hashed_literal_component(&mut self, lit: String) {
        // If there are no characters in the literal string, we just
        // push back the # character
        let Some(first) = lit.chars().next() else {
            self.push_back_str("#");
            return;
        };

        let lit = match first {
            '{' => self.consume_hashed_literal_group(Delimiter::Brace, lit),
            '[' => self.consume_hashed_literal_group(Delimiter::Bracket, lit),
            '(' => self.consume_hashed_literal_group(Delimiter::Parenthesis, lit),
            c if is_ident_start_character(c) => self.consume_hashed_ident_group(lit),
            _ => lit,
        };

        let lit = lit.replace("{", "{{").replace("}", "}}");
        self.push_back_str(lit.as_str());
    }

    fn consume_hashed_ident_group(&mut self, lit: String) -> String {
        for (index, char) in lit.chars().enumerate() {
            if is_ident_character(char) {
                continue;
            }
            let Some((first, second)) = split_at_index(lit.as_str(), index) else {
                // TODO: should this be an error?
                self.push_back_str("#");
                return lit;
            };
            let ident = match parse_str::<Ident>(first) {
                Ok(ident) => ident,
                Err(_err) => {
                    // TODO: should this error propogate?
                    // eprintln!("Error parsing Ident: {:?}", err);
                    self.push_back_str("#");
                    return lit;
                }
            };
            self.consume_ident_hash_substitution(ident);
            return second.to_string();
        }
        match parse_str::<Ident>(lit.as_str()) {
            Ok(ident) => {
                self.consume_ident_hash_substitution(ident);
                return String::new();
            }
            Err(_) => {
                self.push_back_str("#");
            }
        };
        lit
    }

    fn consume_hashed_literal_group(&mut self, delimiter: Delimiter, lit: String) -> String {
        let mut depth: u32 = 0;
        for (index, char) in lit.chars().enumerate() {
            match delimiter.matches(char) {
                Some(DelimiterType::Open) => depth += 1,
                Some(DelimiterType::Close) => depth -= 1,
                None => {}
            }
            if depth == 0 {
                let Some((first, second)) = split_at_index(lit.as_str(), index + 1) else {
                    // TODO: should this be an error?
                    self.push_back_str("#");
                    return lit;
                };

                let group = match parse_str::<Group>(first) {
                    Ok(group) => group,
                    Err(_err) => {
                        // TODO: should this error propogate?
                        // eprintln!("Error parsing group: {:?}", err);
                        self.push_back_str("#");
                        return lit;
                    }
                };

                self.consume_group_hash_substitution(group);
                return second.to_string();
            }
        }
        lit
    }
}

enum DelimiterType {
    Open,
    Close,
}

trait MatchesDelimiter {
    fn matches(&self, c: char) -> Option<DelimiterType>;
}

impl MatchesDelimiter for Delimiter {
    fn matches(&self, c: char) -> Option<DelimiterType> {
        match self {
            Delimiter::Parenthesis => match c {
                '(' => Some(DelimiterType::Open),
                ')' => Some(DelimiterType::Close),
                _ => None,
            },
            Delimiter::Brace => match c {
                '{' => Some(DelimiterType::Open),
                '}' => Some(DelimiterType::Close),
                _ => None,
            },
            Delimiter::Bracket => match c {
                '[' => Some(DelimiterType::Open),
                ']' => Some(DelimiterType::Close),
                _ => None,
            },
            Delimiter::None => None,
        }
    }
}

fn split_at_index(s: &str, index: usize) -> Option<(&str, &str)> {
    if index > s.len() || !s.is_char_boundary(index) {
        // Index out of bounds or not at a character boundary
        return None;
    }

    let (first_part, second_part) = s.split_at(index);
    Some((first_part, second_part))
}

/// `Some(content)` when text opts into JSDoc via a leading `doc` marker at a
/// word boundary (`///doc`, `//!doc`, `/**doc ... */`); marker removed.
/// A normal `/// doc ...` has a leading space, so `strip_prefix("doc")` leaves
/// text " doc ..." and this returns None (it stays a line comment).
fn strip_jsdoc_marker(raw: &str) -> Option<&str> {
    let rest = raw.strip_prefix("doc")?;
    match rest.chars().next() {
        None => Some(rest),                         // exactly "doc"
        Some(c) if c.is_whitespace() => Some(rest), // "doc ...", "doc\n..."
        _ => None,                                  // e.g. "document" -> line comment
    }
}

/// Strip the single leading space the lexer adds, then any further indentation
/// and an optional JSDoc-style `* ` / `*` continuation prefix (common in
/// `/** ... */` blocks), and trim trailing whitespace.
fn clean_doc_line(line: &str) -> &str {
    let line = line.strip_prefix(' ').unwrap_or(line);
    let line = line.trim_start();
    let line = line
        .strip_prefix("* ")
        .or_else(|| line.strip_prefix('*'))
        .unwrap_or(line);
    line.trim_end()
}

/// Escape `{`/`}` so `format!` doesn't treat comment text as placeholders.
fn escape_braces(s: &str) -> String {
    s.replace('{', "{{").replace('}', "}}")
}

/// Replace any `*/` in JSDoc text with `* /` so the block can't terminate early.
fn escape_terminator(s: &str) -> String {
    s.replace("*/", "* /")
}

fn is_ident_character(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_ident_start_character(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
