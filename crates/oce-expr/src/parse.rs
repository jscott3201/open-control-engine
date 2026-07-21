//! Lexer + recursive-descent parser for the CDL binding subset (`02` §6).
//!
//! Operator precedence follows Modelica (low → high): range (`:`) < `or` < `and` < `not` <
//! relational (`> >= < <= == <>`) < additive (`+ -`) < multiplicative (`* /`) < unary sign
//! (`+ -`) < primary. The range colon is the *outermost* binary construct (MLS
//! `simple_expression`), so `1:n-1` spans a whole arithmetic expression on each side and
//! `-1:2` is `(-1):2`. Placing the unary sign tighter than `*`/`/` keeps `-a*b == -(a*b)`
//! (because `(-a)*b == -(a*b)`) while also accepting a sign in factor position (`2 * -3`,
//! `3 - -2`), which real CXF-serialized binding strings contain. Relational operators are
//! non-associative (`a < b < c` is rejected) and live inside range operands. Exponentiation
//! (`^`) is **not** in the CDL binding subset (§6.1) and is rejected. Brace array literals
//! `{a, b, c}` and ranges parse; comprehensions, `A[i]` indexing, and `[a,b;c,d]` matrix
//! constructors are deferred and produce a typed [`ExprError::Parse`], never a panic.

use std::sync::Arc;

use crate::{BinOp, Builtin, BuiltinConst, ExprAst, ExprError, UnOp};

/// A lexical token. `Real` carries an `f64`, so the enum is `PartialEq` but not `Eq`.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    /// An integer literal (no `.`/exponent).
    Int(i64),
    /// A real literal (had a `.` or an exponent).
    Real(f64),
    /// A string literal (quotes stripped, simple escapes resolved).
    Str(Arc<str>),
    /// An identifier or dotted qualified name (also `true`/`false`/`and`/`or`/`not`).
    Name(String),
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `==`
    EqEq,
    /// `<>`
    Ne,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `,`
    Comma,
    /// `{` — opens a brace array literal.
    LBrace,
    /// `}` — closes a brace array literal.
    RBrace,
    /// `:` — the range separator.
    Colon,
    /// `[` — matrix/indexing punctuation; rejected with a typed error (deferred).
    LBracket,
    /// `]` — matrix/indexing punctuation; rejected with a typed error (deferred).
    RBracket,
}

fn parse_err(msg: impl Into<String>) -> ExprError {
    ExprError::Parse(msg.into())
}

/// Tokenize `text`, rejecting characters outside the scalar subset with a typed error.
fn lex(text: &str) -> Result<Vec<Tok>, ExprError> {
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            ' ' | '\t' | '\r' | '\n' => i += 1,
            '0'..='9' => {
                let (tok, next) = lex_number(&bytes, i)?;
                out.push(tok);
                i = next;
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                let (tok, next) = lex_name(&bytes, i);
                out.push(tok);
                i = next;
            }
            '"' => {
                let (tok, next) = lex_string(&bytes, i)?;
                out.push(tok);
                i = next;
            }
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            '/' => {
                out.push(Tok::Slash);
                i += 1;
            }
            '>' => {
                if bytes.get(i + 1) == Some(&'=') {
                    out.push(Tok::Ge);
                    i += 2;
                } else {
                    out.push(Tok::Gt);
                    i += 1;
                }
            }
            '<' => match bytes.get(i + 1) {
                Some('=') => {
                    out.push(Tok::Le);
                    i += 2;
                }
                Some('>') => {
                    out.push(Tok::Ne);
                    i += 2;
                }
                _ => {
                    out.push(Tok::Lt);
                    i += 1;
                }
            },
            '=' => {
                if bytes.get(i + 1) == Some(&'=') {
                    out.push(Tok::EqEq);
                    i += 2;
                } else {
                    return Err(parse_err(
                        "bare '=' is a binding, not a value expression (use '==' for equality)",
                    ));
                }
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '{' => {
                out.push(Tok::LBrace);
                i += 1;
            }
            '}' => {
                out.push(Tok::RBrace);
                i += 1;
            }
            ':' => {
                out.push(Tok::Colon);
                i += 1;
            }
            '[' => {
                out.push(Tok::LBracket);
                i += 1;
            }
            ']' => {
                out.push(Tok::RBracket);
                i += 1;
            }
            other => return Err(parse_err(format!("unexpected character {other:?}"))),
        }
    }
    Ok(out)
}

/// Lex a Modelica `UNSIGNED_NUMBER`: integer part, optional `.frac`, optional `e[+|-]exp`.
/// Integer iff it has neither a fractional part nor an exponent.
fn lex_number(bytes: &[char], start: usize) -> Result<(Tok, usize), ExprError> {
    let mut i = start;
    let mut is_real = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == '.' {
        is_real = true;
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < bytes.len() && (bytes[i] == 'e' || bytes[i] == 'E') {
        is_real = true;
        i += 1;
        if i < bytes.len() && (bytes[i] == '+' || bytes[i] == '-') {
            i += 1;
        }
        let exp_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return Err(parse_err("malformed number: exponent has no digits"));
        }
    }
    let text: String = bytes[start..i].iter().collect();
    let tok = if is_real {
        Tok::Real(
            text.parse::<f64>()
                .map_err(|_| parse_err(format!("malformed real literal {text:?}")))?,
        )
    } else {
        Tok::Int(
            text.parse::<i64>()
                .map_err(|_| parse_err(format!("integer literal out of range {text:?}")))?,
        )
    };
    Ok((tok, i))
}

/// Lex an identifier or dotted qualified name (`Modelica.Constants.pi`). A `.` continues the
/// name only when followed by an identifier start; `.` before a digit cannot occur (numbers
/// never start with `.`), so there is no ambiguity with member/array syntax.
fn lex_name(bytes: &[char], start: usize) -> (Tok, usize) {
    let mut i = start;
    loop {
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == '_') {
            i += 1;
        }
        if i + 1 < bytes.len()
            && bytes[i] == '.'
            && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == '_')
        {
            i += 1; // consume '.', continue the qualified name
        } else {
            break;
        }
    }
    let text: String = bytes[start..i].iter().collect();
    (Tok::Name(text), i)
}

/// Lex a `"..."` string literal, resolving the `\"`, `\\`, `\n`, `\t` escapes.
fn lex_string(bytes: &[char], start: usize) -> Result<(Tok, usize), ExprError> {
    let mut i = start + 1; // skip opening quote
    let mut s = String::new();
    while i < bytes.len() {
        match bytes[i] {
            '"' => return Ok((Tok::Str(Arc::from(s.as_str())), i + 1)),
            '\\' => {
                i += 1;
                match bytes.get(i) {
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some(other) => s.push(*other),
                    None => return Err(parse_err("string ends with a dangling backslash")),
                }
                i += 1;
            }
            other => {
                s.push(other);
                i += 1;
            }
        }
    }
    Err(parse_err("unterminated string literal"))
}

/// A recursive-descent parser over the lexed token stream.
struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    /// Consume a token equal to `want` or report a parse error mentioning `ctx`.
    fn expect(&mut self, want: &Tok, ctx: &str) -> Result<(), ExprError> {
        if self.peek() == Some(want) {
            self.pos += 1;
            Ok(())
        } else {
            Err(parse_err(format!("expected {ctx}")))
        }
    }

    /// `simple_expression : logical_expression [":" logical_expression [":" logical_expression]]`
    /// (MLS). The range colon is the loosest binary construct — looser than `or` — so each
    /// operand is a full `parse_or` expression and `-1:2` parses as `(-1):2`. Non-nesting:
    /// after `a:b:c` a further `:` is left unconsumed and rejected by the caller.
    fn parse_range(&mut self) -> Result<ExprAst, ExprError> {
        let first = self.parse_or()?;
        if self.peek() != Some(&Tok::Colon) {
            return Ok(first);
        }
        self.pos += 1;
        let second = self.parse_or()?;
        if self.peek() != Some(&Tok::Colon) {
            return Ok(ExprAst::Range {
                start: Box::new(first),
                step: None,
                stop: Box::new(second),
            });
        }
        self.pos += 1;
        let third = self.parse_or()?;
        Ok(ExprAst::Range {
            start: Box::new(first),
            step: Some(Box::new(second)),
            stop: Box::new(third),
        })
    }

    /// `logical_expression : logical_term ("or" logical_term)*`
    fn parse_or(&mut self) -> Result<ExprAst, ExprError> {
        let mut lhs = self.parse_and()?;
        while self.peek_keyword("or") {
            self.pos += 1;
            let rhs = self.parse_and()?;
            lhs = ExprAst::Binary(BinOp::Or, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// `logical_term : logical_factor ("and" logical_factor)*`
    fn parse_and(&mut self) -> Result<ExprAst, ExprError> {
        let mut lhs = self.parse_not()?;
        while self.peek_keyword("and") {
            self.pos += 1;
            let rhs = self.parse_not()?;
            lhs = ExprAst::Binary(BinOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// `logical_factor : ["not"] relation`
    fn parse_not(&mut self) -> Result<ExprAst, ExprError> {
        if self.peek_keyword("not") {
            self.pos += 1;
            let operand = self.parse_relation()?;
            Ok(ExprAst::Unary(UnOp::Not, Box::new(operand)))
        } else {
            self.parse_relation()
        }
    }

    /// `relation : arithmetic_expression [rel_op arithmetic_expression]` (non-associative).
    fn parse_relation(&mut self) -> Result<ExprAst, ExprError> {
        let lhs = self.parse_add()?;
        let op = match self.peek() {
            Some(Tok::Gt) => BinOp::Gt,
            Some(Tok::Ge) => BinOp::Ge,
            Some(Tok::Lt) => BinOp::Lt,
            Some(Tok::Le) => BinOp::Le,
            Some(Tok::EqEq) => BinOp::Eq,
            Some(Tok::Ne) => BinOp::Ne,
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.parse_add()?;
        Ok(ExprAst::Binary(op, Box::new(lhs), Box::new(rhs)))
    }

    /// `arithmetic_expression : term (add_op term)*` (left-associative). The unary sign is
    /// handled one level down in [`Parser::parse_unary`], so both `-a*b` (== `-(a*b)`) and
    /// `2 * -3` (a sign after a binary operator) parse.
    fn parse_add(&mut self) -> Result<ExprAst, ExprError> {
        let mut lhs = self.parse_term()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_term()?;
            lhs = ExprAst::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// `term : unary (mul_op unary)*`
    fn parse_term(&mut self) -> Result<ExprAst, ExprError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_unary()?;
            lhs = ExprAst::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// `unary : [add_op] unary | primary` — a unary `+`/`-` binds tighter than `*`/`/`, so a
    /// sign is accepted in factor position (`2 * -3`, `3 - -2`). Numerically `-a*b == -(a*b)`
    /// since `(-a)*b == -(a*b)`.
    fn parse_unary(&mut self) -> Result<ExprAst, ExprError> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.pos += 1;
                Ok(ExprAst::Unary(UnOp::Neg, Box::new(self.parse_unary()?)))
            }
            Some(Tok::Plus) => {
                self.pos += 1;
                self.parse_unary()
            }
            _ => self.parse_primary(),
        }
    }

    /// `primary : NUMBER | STRING | "(" expr ")" | "{" array_args "}" | name [ "(" args ")" ]
    /// | true | false`. A parenthesized inner expression is a full range expression, so
    /// `(1:3)` parses. A leading `[` is the Modelica matrix constructor — deferred, with an
    /// explicit typed rejection rather than a stray-token fall-through.
    fn parse_primary(&mut self) -> Result<ExprAst, ExprError> {
        match self.bump() {
            Some(Tok::Int(v)) => Ok(ExprAst::Int(v)),
            Some(Tok::Real(v)) => Ok(ExprAst::Real(v)),
            Some(Tok::Str(s)) => Ok(ExprAst::Str(s)),
            Some(Tok::LParen) => {
                let inner = self.parse_range()?;
                self.expect(&Tok::RParen, "')' to close a parenthesized expression")?;
                Ok(inner)
            }
            Some(Tok::LBrace) => self.parse_array_literal(),
            Some(Tok::LBracket) => Err(parse_err("matrix constructor [a,b;c,d] is not supported")),
            Some(Tok::Name(n)) => self.parse_name(n),
            Some(other) => Err(parse_err(format!("unexpected token {other:?}"))),
            None => Err(parse_err("unexpected end of expression")),
        }
    }

    /// `array_literal : "{" [ range ("," range)* ] "}"` — the opening `{` is already consumed.
    /// Elements are full range expressions (`{1:3, 5}` is legal); no trailing comma
    /// (Modelica). `{}` parses to an empty [`ExprAst::ArrayLit`] and is rejected at
    /// evaluation ([`ExprError::EmptyArray`]), where the distinction from a legal empty
    /// range can be reported precisely.
    fn parse_array_literal(&mut self) -> Result<ExprAst, ExprError> {
        let mut elems = Vec::new();
        if self.peek() != Some(&Tok::RBrace) {
            loop {
                elems.push(self.parse_range()?);
                match self.peek() {
                    Some(Tok::Comma) => self.pos += 1,
                    _ => break,
                }
            }
        }
        self.expect(&Tok::RBrace, "'}' to close an array literal")?;
        Ok(ExprAst::ArrayLit(elems))
    }

    /// Resolve a `Name` token in primary position: keyword literal, function call, named
    /// constant, plain identifier, or a deferred/illegal qualified name.
    fn parse_name(&mut self, n: String) -> Result<ExprAst, ExprError> {
        match n.as_str() {
            "true" => return Ok(ExprAst::Bool(true)),
            "false" => return Ok(ExprAst::Bool(false)),
            "and" | "or" | "not" => {
                return Err(parse_err(format!("operator '{n}' has no left operand")));
            }
            _ => {}
        }
        if self.peek() == Some(&Tok::LParen) {
            self.pos += 1;
            return self.parse_call(&n);
        }
        if let Some(c) = recognize_const(&n) {
            return Ok(ExprAst::Const(c));
        }
        if n.contains('.') {
            return Ok(ExprAst::EnumRef(Arc::from(n.as_str())));
        }
        Ok(ExprAst::Ident(Arc::from(n.as_str())))
    }

    /// Parse a comma-separated argument list (the opening `(` is already consumed), then resolve
    /// the call against the §7.7.2 built-in table with an arity check. Arguments are full range
    /// expressions, so `sum(1:3)` consumes the range as one argument.
    fn parse_call(&mut self, name: &str) -> Result<ExprAst, ExprError> {
        let mut args = Vec::new();
        if self.peek() != Some(&Tok::RParen) {
            loop {
                args.push(self.parse_range()?);
                match self.peek() {
                    Some(Tok::Comma) => self.pos += 1,
                    _ => break,
                }
            }
        }
        self.expect(&Tok::RParen, "')' to close an argument list")?;
        let builtin = resolve_builtin(name, args.len())?;
        Ok(ExprAst::Call(builtin, args))
    }

    fn peek_keyword(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Name(n)) if n == kw)
    }
}

/// Recognize `pi`/`eps`/`small` either bare or qualified as `…Constants.<name>`, plus the retained
/// `inf` compatibility alias. A qualified name whose final package segment is not `Constants` is
/// not a constant.
fn recognize_const(name: &str) -> Option<BuiltinConst> {
    let mut it = name.rsplitn(2, '.');
    let leaf = it.next()?;
    let c = match leaf {
        "pi" => BuiltinConst::Pi,
        "eps" => BuiltinConst::Eps,
        "small" => BuiltinConst::Small,
        "inf" => BuiltinConst::Inf,
        _ => return None,
    };
    match it.next() {
        None => Some(c),
        Some(prefix) if prefix == "Constants" || prefix.ends_with(".Constants") => Some(c),
        Some(_) => None,
    }
}

/// Map a function name + argument count to a [`Builtin`], or a typed error. `min`/`max`
/// dispatch on arity (one argument → array reduction, two → scalar); a `fill` with more than
/// one extent would build a 2-D array and is reported as deferred. Everything outside the
/// table is [`ExprError::UnsupportedFunction`] (R9 closed-world).
fn resolve_builtin(name: &str, argc: usize) -> Result<Builtin, ExprError> {
    let arity_err = |n: &str, want: &str| parse_err(format!("'{n}' expects {want}, got {argc}"));
    match name {
        "abs" | "sign" | "sqrt" | "floor" | "ceil" | "integer" => {
            if argc != 1 {
                return Err(arity_err(name, "1 argument"));
            }
            Ok(match name {
                "abs" => Builtin::Abs,
                "sign" => Builtin::Sign,
                "sqrt" => Builtin::Sqrt,
                "floor" => Builtin::Floor,
                "ceil" => Builtin::Ceil,
                _ => Builtin::Integer,
            })
        }
        "div" | "mod" | "rem" => {
            if argc != 2 {
                return Err(arity_err(name, "2 arguments"));
            }
            Ok(match name {
                "div" => Builtin::Div,
                "mod" => Builtin::Mod,
                _ => Builtin::Rem,
            })
        }
        "min" | "max" => match argc {
            1 => Ok(if name == "min" {
                Builtin::MinArr
            } else {
                Builtin::MaxArr
            }),
            2 => Ok(if name == "min" {
                Builtin::MinScalar
            } else {
                Builtin::MaxScalar
            }),
            _ => Err(arity_err(name, "1 array argument or 2 scalar arguments")),
        },
        "sum" => {
            if argc != 1 {
                return Err(arity_err(name, "1 argument"));
            }
            Ok(Builtin::Sum)
        }
        "size" => match argc {
            1 | 2 => Ok(Builtin::Size),
            _ => Err(arity_err(name, "1 or 2 arguments")),
        },
        "fill" => match argc {
            2 => Ok(Builtin::Fill),
            n if n >= 3 => Err(parse_err(
                "'fill' with more than one extent builds a 2-D or higher array, which is \
                 deferred (only 1-D 'fill(x, n)' is supported)",
            )),
            _ => Err(arity_err(name, "2 arguments")),
        },
        "cat" => {
            if argc < 3 {
                return Err(arity_err(name, "at least 3 arguments: cat(k, A, B, ...)"));
            }
            Ok(Builtin::Cat)
        }
        other => Err(ExprError::UnsupportedFunction(other.to_string())),
    }
}

/// Parse opaque CDL binding text into an [`ExprAst`]. The full token stream must be consumed;
/// trailing tokens are a parse error.
pub(crate) fn parse(text: &str) -> Result<ExprAst, ExprError> {
    let toks = lex(text)?;
    if toks.is_empty() {
        return Err(parse_err("empty expression"));
    }
    let mut parser = Parser { toks, pos: 0 };
    let ast = parser.parse_range()?;
    if parser.pos != parser.toks.len() {
        return Err(parse_err(format!(
            "unexpected trailing input after a complete expression: {:?}",
            parser.toks[parser.pos]
        )));
    }
    Ok(ast)
}
