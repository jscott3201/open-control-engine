//! Recursive-descent parser for the CDL binding subset (`02` §6), over the [`Tok`] stream
//! produced by [`mod@crate::lex`].
//!
//! Operator precedence follows Modelica (low → high): range (`:`) < `or` < `and` < `not` <
//! relational (`> >= < <= == <>`) < additive (`+ -`) < multiplicative (`* /`) < unary sign
//! (`+ -`) < postfix subscript (`A[i]`) < primary. The range colon is the *outermost* binary
//! construct (MLS `simple_expression`), so `1:n-1` spans a whole arithmetic expression on
//! each side and `-1:2` is `(-1):2`. Placing the unary sign tighter than `*`/`/` keeps
//! `-a*b == -(a*b)` (because `(-a)*b == -(a*b)`) while also accepting a sign in factor
//! position (`2 * -3`, `3 - -2`), which real CXF-serialized binding strings contain; the
//! subscript binds tighter still, so `-A[i]` is `-(A[i])`. Relational operators are
//! non-associative (`a < b < c` is rejected) and live inside range operands. Exponentiation
//! (`^`) is **not** in the CDL binding subset (§6.1) and is rejected. Brace array literals
//! `{a, b, c}`, ranges, postfix indexing, and comprehensions (`{e for i in r}`, plus the
//! `sum(e for i in r)` reduction sugar) parse; `for` and `in` are **contextual** keywords —
//! recognized only after a brace element or a `sum` argument — so an identifier named `for`
//! anywhere else keeps parsing as an [`ExprAst::Ident`]. `[a,b;c,d]` matrix constructors are
//! deferred and array slicing (`A[a:b]`) is out of subset — each produces a typed
//! [`ExprError::Parse`], never a panic.

use std::sync::Arc;

use crate::lex::{Tok, lex, parse_err};
use crate::{BinOp, Builtin, BuiltinConst, ExprAst, ExprError, UnOp};

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

    /// `unary : [add_op] unary | postfix` — a unary `+`/`-` binds tighter than `*`/`/`, so a
    /// sign is accepted in factor position (`2 * -3`, `3 - -2`). Numerically `-a*b == -(a*b)`
    /// since `(-a)*b == -(a*b)`. The operand is a [`Parser::parse_postfix`] production, so a
    /// subscript binds tighter than the sign: `-A[i]` is `-(A[i])`.
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
            _ => self.parse_postfix(),
        }
    }

    /// `postfix : primary ("[" subscript ("," subscript)* "]")*` — zero or more subscript
    /// groups after a primary, so chains like `A[1][2]` parse naturally as nested
    /// [`ExprAst::Index`] nodes. Grammatically each subscript is a full range expression
    /// ([`Parser::parse_range`]), but a subscript that parses *to* a range is array slicing
    /// (`A[a:b]`) — out of subset, rejected here with a typed error rather than left to
    /// surface as a confusing evaluation-time shape problem. Comma-separated subscripts
    /// build one node with several indices; the evaluator enforces the 1-D limit so the
    /// rejection can name the multi-dimensional deferral. `A[]` is malformed. A `[` in
    /// *primary* position never reaches here — [`Parser::parse_primary`] rejects it as the
    /// deferred matrix constructor.
    fn parse_postfix(&mut self) -> Result<ExprAst, ExprError> {
        let mut base = self.parse_primary()?;
        while self.peek() == Some(&Tok::LBracket) {
            self.pos += 1;
            if self.peek() == Some(&Tok::RBracket) {
                return Err(parse_err(
                    "empty subscript: 'A[]' needs at least one index expression",
                ));
            }
            let mut indices = Vec::new();
            loop {
                let index = self.parse_range()?;
                if matches!(index, ExprAst::Range { .. }) {
                    return Err(parse_err(
                        "array slicing 'A[a:b]' is not supported (a subscript must be a \
                         scalar index)",
                    ));
                }
                indices.push(index);
                match self.peek() {
                    Some(Tok::Comma) => self.pos += 1,
                    _ => break,
                }
            }
            self.expect(&Tok::RBracket, "']' to close an array subscript")?;
            base = ExprAst::Index {
                base: Box::new(base),
                indices,
            };
        }
        Ok(base)
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

    /// `array_literal : "{" [ range ("," range)* ] "}"` or the comprehension
    /// `"{" range for_clauses "}"` — the opening `{` is already consumed. Elements are full
    /// range expressions (`{1:3, 5}` is legal); no trailing comma (Modelica). `{}` parses to
    /// an empty [`ExprAst::ArrayLit`] and is rejected at evaluation
    /// ([`ExprError::EmptyArray`]), where the distinction from a legal empty range can be
    /// reported precisely.
    ///
    /// The contextual keyword `for` after the **first** element re-interprets the brace: the
    /// already-parsed expression is the comprehension BODY and `for i in r` clauses follow.
    /// After a comma-separated element the same `for` is a typed error — a comprehension has
    /// exactly one body, so `{1, 2 for i in 1:3}` is malformed, and `{for i in 1:3}` has no
    /// body at all (its `for` parses as an identifier, and the clause tokens after it fail
    /// the close-brace expectation).
    fn parse_array_literal(&mut self) -> Result<ExprAst, ExprError> {
        if self.peek() == Some(&Tok::RBrace) {
            self.pos += 1;
            return Ok(ExprAst::ArrayLit(Vec::new()));
        }
        let first = self.parse_range()?;
        if self.peek_keyword("for") {
            self.pos += 1; // consume the contextual 'for'
            let iters = self.parse_comprehension_iterators()?;
            self.expect(&Tok::RBrace, "'}' to close an array comprehension")?;
            return Ok(ExprAst::Comprehension {
                body: Box::new(first),
                iters,
            });
        }
        let mut elems = vec![first];
        while self.peek() == Some(&Tok::Comma) {
            self.pos += 1;
            elems.push(self.parse_range()?);
            if self.peek_keyword("for") {
                return Err(parse_err(
                    "a comprehension takes a single body expression before 'for' \
                     (list elements cannot be mixed with a for-clause)",
                ));
            }
        }
        self.expect(&Tok::RBrace, "'}' to close an array literal")?;
        Ok(ExprAst::ArrayLit(elems))
    }

    /// `for_clauses : iterator ("," iterator)*` where `iterator : IDENT "in" range` — the
    /// leading contextual `for` is already consumed. The iteration source is a full range
    /// expression. Comma-separated clauses PARSE into one node (the reserved multi-iterator
    /// surface); evaluation rejects more than one clause naming the deferral.
    fn parse_comprehension_iterators(&mut self) -> Result<Vec<(Arc<str>, ExprAst)>, ExprError> {
        let mut iters = Vec::new();
        loop {
            let name = self.expect_iterator_name()?;
            if !self.peek_keyword("in") {
                return Err(parse_err(
                    "expected 'in' after the comprehension iterator name",
                ));
            }
            self.pos += 1;
            let source = self.parse_range()?;
            iters.push((name, source));
            match self.peek() {
                Some(Tok::Comma) => self.pos += 1,
                _ => break,
            }
        }
        Ok(iters)
    }

    /// Consume the iterator name of a `for` clause: a **plain** identifier — one the grammar
    /// could resolve back to an [`ExprAst::Ident`] in the body. Keyword literals
    /// (`true`/`and`/…), the contextual `for`/`in`, dotted qualified names, and the named
    /// constants (`pi`/`eps`/`small`/`inf`) all parse to non-`Ident` nodes in body position,
    /// so a binding under any of those names would be silently unreachable — each is rejected
    /// with a typed error instead.
    fn expect_iterator_name(&mut self) -> Result<Arc<str>, ExprError> {
        match self.bump() {
            Some(Tok::Name(n)) => {
                if matches!(
                    n.as_str(),
                    "true" | "false" | "and" | "or" | "not" | "for" | "in"
                ) {
                    return Err(parse_err(format!(
                        "'{n}' is a keyword and cannot name a comprehension iterator"
                    )));
                }
                if n.contains('.') {
                    return Err(parse_err(format!(
                        "a comprehension iterator must be a plain identifier, \
                         found qualified name '{n}'"
                    )));
                }
                if recognize_const(&n).is_some() {
                    return Err(parse_err(format!(
                        "'{n}' is a named constant and cannot name a comprehension iterator"
                    )));
                }
                Ok(Arc::from(n.as_str()))
            }
            Some(other) => Err(parse_err(format!(
                "expected a comprehension iterator name, found {other:?}"
            ))),
            None => Err(parse_err("expected a comprehension iterator name")),
        }
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
    ///
    /// The contextual keyword `for` after an argument is the Modelica reduction expression
    /// `f(e for i in r)`. Ratified scope: only `sum` takes the sugar, and only as its single
    /// first argument — the already-parsed expression becomes the comprehension BODY and the
    /// [`ExprAst::Comprehension`] becomes `sum`'s argument (the existing `sum` built-in then
    /// receives an array; no new built-in). Any other placement is a typed error naming the
    /// sum-only support.
    fn parse_call(&mut self, name: &str) -> Result<ExprAst, ExprError> {
        let mut args = Vec::new();
        if self.peek() != Some(&Tok::RParen) {
            loop {
                let mut arg = self.parse_range()?;
                if self.peek_keyword("for") {
                    if name != "sum" {
                        return Err(parse_err(format!(
                            "'{name}' does not take a reduction expression \
                             ('e for i in r' is only supported in 'sum')"
                        )));
                    }
                    if !args.is_empty() {
                        return Err(parse_err(
                            "'sum' takes a reduction expression as its only argument",
                        ));
                    }
                    self.pos += 1; // consume the contextual 'for'
                    let iters = self.parse_comprehension_iterators()?;
                    arg = ExprAst::Comprehension {
                        body: Box::new(arg),
                        iters,
                    };
                }
                args.push(arg);
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
