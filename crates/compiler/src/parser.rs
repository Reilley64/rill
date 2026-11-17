//! Strict, source-preserving parser for the two version-1 syntax annex artifacts.
//! No recovery, name/type-directed retries, semantic checking or editor contract.

use crate::lexer::{LexStatus, Lexed, LiteralMode, TokenKind};
use crate::source::{Snapshot, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryPoint {
    Module,
    Expression,
    Type,
    Pattern,
}

/// Implementation budgets, not language limits. Raising nesting above 64 does
/// not remove the host-stack safety ceiling. Work includes discarded lookahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseLimits {
    pub max_nesting: usize,
    pub max_work: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_nesting: 64,
            max_work: 1_000_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Lexical,
    InputMismatch,
    Syntax { expected: &'static str },
    ResourceLimit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseFailure {
    pub kind: FailureKind,
    pub span: Span,
}

type Parsed<T> = Result<T, ParseFailure>;
pub type NodeId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Module,
    Name,
    QualifiedName,
    NamedType,
    TypeArguments,
    ReferenceType { mutable: bool },
    CallableType { mode: Option<&'static str> },
    TupleType,
    GroupType,
    ValueReference,
    Literal,
    Group,
    Bracket,
    Binary(&'static str),
    Prefix(&'static str),
    Call,
    Index,
    Field,
    Construction,
    InitializerField { shorthand: bool },
    Spread,
    Propagate,
    Wrap,
    Template,
    TemplateText,
    TemplateHole,
    Wildcard,
    NamePattern,
    PatternPath,
    RecordPattern,
    PayloadPattern,
    BracketPattern,
    GroupPattern,
    PatternField { shorthand: bool },
    NegatedPattern,
    Import,
    ImportGroup,
    ImportItem,
    Export,
    Function,
    Implementation,
    Signature,
    GenericParameters { implementation: bool },
    GenericParameter,
    ValueParameters,
    Parameter,
    Requirements,
    Requirement,
    Record,
    RecordShape,
    RecordField { private: bool },
    Enum,
    EnumVariant,
    TuplePayload,
    RecordPayload,
    Block,
    Binding { mutable: bool },
    GuardBinding,
    Return,
    Break,
    Continue,
    Guard,
    While,
    For,
    IfStatement,
    IfExpression,
    ExpressionStatement,
    Assignment(&'static str),
    Expect,
    Arrow,
    ArrowHead,
    ArrowParameters,
    ArrowParameter,
    Match,
    MatchArm,
    MatchGuard,
    Pipe,
    PipeStep { propagate: bool },
    PipeArguments,
    PipePlaceholder,
}

/// A flat arena avoids recursive tree destruction, even for long left chains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    pub span: Span,
    pub children: Vec<NodeId>,
}

/// An original significant token, or one exact parser-owned subspan of it.
/// Consult `lexed.tokens[token]` for its unsplit lexical kind and literal data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenView {
    pub token: usize,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Syntax {
    pub snapshot: Snapshot,
    pub lexed: Lexed,
    pub nodes: Vec<Node>,
    pub root: NodeId,
    pub views: Vec<TokenView>,
}

/// Parse exactly the selected input grammar and EOF. The lexical result must
/// come from this immutable snapshot. Failure never exposes an accepted prefix.
pub fn parse(
    snapshot: &Snapshot,
    lexed: &Lexed,
    entry: EntryPoint,
    limits: ParseLimits,
) -> Parsed<Syntax> {
    if lexed.status != LexStatus::Complete || !lexed.diagnostics.is_empty() {
        return Err(ParseFailure {
            kind: FailureKind::Lexical,
            span: snapshot.eof(),
        });
    }
    let mut parser = Parser {
        snapshot,
        lexed,
        limits,
        work: 0,
        depth: 0,
        at: 0,
        offset: 0,
        end: 0,
        delimiters: 0,
        guard_delimiter: None,
        input_validated: false,
        nodes: Vec::new(),
        views: Vec::new(),
    };
    parser.validate_input()?;
    parser.input_validated = true;
    let root = match entry {
        EntryPoint::Module => parser.module()?,
        EntryPoint::Expression => parser.expression()?,
        EntryPoint::Type => parser.ty()?,
        EntryPoint::Pattern => parser.pattern()?,
    };
    if !parser.eof() {
        return parser.error("end of input");
    }
    parser.bump()?;
    Ok(Syntax {
        snapshot: snapshot.clone(),
        lexed: lexed.clone(),
        nodes: parser.nodes,
        root,
        views: parser.views,
    })
}

#[derive(Clone, Copy)]
struct Checkpoint {
    at: usize,
    offset: usize,
    end: usize,
    nodes: usize,
    views: usize,
}

struct Parser<'a> {
    snapshot: &'a Snapshot,
    lexed: &'a Lexed,
    limits: ParseLimits,
    work: usize,
    depth: usize,
    delimiters: usize,
    guard_delimiter: Option<usize>,
    input_validated: bool,
    at: usize,
    offset: usize,
    end: usize,
    nodes: Vec<Node>,
    views: Vec<TokenView>,
}

impl Parser<'_> {
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Parsed<T>) -> Parsed<T> {
        self.charge()?;
        if self.depth >= self.limits.max_nesting.min(64) {
            return Err(ParseFailure {
                kind: FailureKind::ResourceLimit,
                span: self.span(),
            });
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    fn delimited<T>(&mut self, f: impl FnOnce(&mut Self) -> Parsed<T>) -> Parsed<T> {
        self.delimiters += 1;
        let result = f(self);
        self.delimiters -= 1;
        result
    }

    fn is(&self, spelling: &str) -> bool {
        match self.lexed.tokens[self.at].kind {
            TokenKind::Keyword(s) | TokenKind::Punctuation(s) => &s[self.offset..] == spelling,
            _ => false,
        }
    }

    fn eat(&mut self, spelling: &str) -> Parsed<bool> {
        if !self.is(spelling) {
            return Ok(false);
        }
        self.bump()?;
        Ok(true)
    }

    fn require(&mut self, spelling: &'static str) -> Parsed<()> {
        if self.eat(spelling)? {
            Ok(())
        } else {
            self.error(spelling)
        }
    }

    fn identifier(&self) -> bool {
        self.lexed.tokens[self.at].kind == TokenKind::Identifier
    }

    fn name(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        if !self.identifier() {
            return self.error("identifier");
        }
        self.bump()?;
        self.node(start, NodeKind::Name, Vec::new())
    }

    fn path(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let mut names = vec![self.name()?];
        while self.is("::")
            && self
                .lexed
                .tokens
                .get(self.at + 1)
                .is_some_and(|t| t.kind == TokenKind::Identifier)
        {
            self.bump()?;
            names.push(self.name()?);
        }
        self.node(start, NodeKind::QualifiedName, names)
    }

    fn closer(&self, close: &str) -> bool {
        self.is(close)
            || (close == ">"
                && matches!(self.lexed.tokens[self.at].kind, TokenKind::Punctuation(s) if s[self.offset..].starts_with('>')))
    }

    fn close(&mut self, close: &'static str) -> Parsed<()> {
        if close == ">" {
            self.leading('>')
        } else {
            self.require(close)
        }
    }

    // The only token views are established angle closers and type ampersands.
    fn leading(&mut self, character: char) -> Parsed<()> {
        let TokenKind::Punctuation(s) = self.lexed.tokens[self.at].kind else {
            return self.error("type punctuation");
        };
        if !s[self.offset..].starts_with(character) {
            return self.error("type punctuation");
        }
        self.charge()?;
        let start = self.start();
        self.end = start + 1;
        self.views.push(TokenView {
            token: self.at,
            span: self.snapshot.span(start, self.end).unwrap(),
        });
        self.offset += 1;
        if self.offset == s.len() {
            self.at += 1;
            self.offset = 0;
        }
        Ok(())
    }

    fn list(
        &mut self,
        close: &'static str,
        separator: &'static str,
        empty: bool,
        item: fn(&mut Self) -> Parsed<NodeId>,
    ) -> Parsed<Vec<NodeId>> {
        let mut items = Vec::new();
        if empty && self.closer(close) {
            self.close(close)?;
            return Ok(items);
        }
        loop {
            items.push(item(self)?);
            if !self.eat(separator)? || self.closer(close) {
                break;
            }
        }
        self.close(close)?;
        Ok(items)
    }

    fn type_arguments(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        self.require("<")?;
        let args = self.list(">", ",", false, Self::ty)?;
        self.node(start, NodeKind::TypeArguments, args)
    }

    fn ty(&mut self) -> Parsed<NodeId> {
        self.nested(Self::type_inner)
    }

    fn type_inner(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        if self.is("&") || self.is("&&") {
            self.leading('&')?;
            let mutable = self.eat("mut")?;
            let inner = self.ty()?;
            return self.node(start, NodeKind::ReferenceType { mutable }, vec![inner]);
        }
        if self.eat("[")? {
            let elements = self.list("]", ",", true, Self::ty)?;
            return self.node(start, NodeKind::TupleType, elements);
        }
        let mode = if self.eat("mut")? {
            Some("mut")
        } else if self.eat("once")? {
            Some("once")
        } else {
            None
        };
        if self.eat("(")? {
            let mut params = Vec::new();
            let mut comma = false;
            if !self.is(")") {
                loop {
                    params.push(self.ty()?);
                    if !self.eat(",")? {
                        break;
                    }
                    comma = true;
                    if self.is(")") {
                        break;
                    }
                }
            }
            self.require(")")?;
            if self.eat("=>")? {
                params.push(self.ty()?);
                return self.node(start, NodeKind::CallableType { mode }, params);
            }
            if mode.is_some() || params.len() != 1 || comma {
                return self.error("callable arrow or single grouped type");
            }
            return self.node(start, NodeKind::GroupType, params);
        }
        if mode.is_some() {
            return self.error("callable parameter list");
        }
        let children = self.applied_path()?;
        self.node(start, NodeKind::NamedType, children)
    }

    fn applied_path(&mut self) -> Parsed<Vec<NodeId>> {
        let mut children = vec![self.path()?];
        if self.is("<") {
            children.push(self.type_arguments()?);
        }
        if self.eat(".")? {
            children.push(self.name()?);
        }
        Ok(children)
    }

    fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            at: self.at,
            offset: self.offset,
            end: self.end,
            nodes: self.nodes.len(),
            views: self.views.len(),
        }
    }

    fn restore(&mut self, checkpoint: Checkpoint) {
        self.at = checkpoint.at;
        self.offset = checkpoint.offset;
        self.end = checkpoint.end;
        self.nodes.truncate(checkpoint.nodes);
        self.views.truncate(checkpoint.views);
        // Work is deliberately NOT restored: failed lookahead is not free.
    }

    fn value_reference(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let mut children = vec![self.path()?];
        if self.is("<") {
            let checkpoint = self.checkpoint();
            match self.type_arguments() {
                Ok(arguments) if self.generic_follow() => children.push(arguments),
                Err(error) if error.kind == FailureKind::ResourceLimit => return Err(error),
                _ => self.restore(checkpoint),
            }
        }
        self.node(start, NodeKind::ValueReference, children)
    }

    fn generic_follow(&self) -> bool {
        // An equals residue from >= or >>= must not manufacture instantiation
        // followed by assignment, even though a standalone '=' is in FOLLOW.
        if self.offset > 0 && self.is("=") {
            return false;
        }
        self.eof()
            || self.lexed.tokens[self.at].kind == TokenKind::HoleClose
            || [
                "(", "{", ".", ";", ",", ")", "]", "}", ":", "=", "=>", "?", "|>", "else",
            ]
            .iter()
            .any(|s| self.is(s))
    }

    fn expression(&mut self) -> Parsed<NodeId> {
        self.nested(Self::expression_inner)
    }

    fn expression_inner(&mut self) -> Parsed<NodeId> {
        if let Some(arrow) = self.arrow()? {
            return Ok(arrow);
        }
        self.wrapping()
    }

    fn arrow_parameter(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let mut children = vec![self.parameter_name()?];
        if self.eat(":")? {
            children.push(self.ty()?);
        }
        self.node(start, NodeKind::ArrowParameter, children)
    }

    fn arrow_head(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        self.require("(")?;
        let parameters = self.list(")", ",", true, Self::arrow_parameter)?;
        let mut children = vec![self.node(start, NodeKind::ArrowParameters, parameters)?];
        if self.eat(":")? {
            children.push(self.ty()?);
        }
        // The head span does not include its following arrow delimiter.
        let head = self.node(start, NodeKind::ArrowHead, children)?;
        self.require("=>")?;
        Ok(head)
    }

    fn arrow(&mut self) -> Parsed<Option<NodeId>> {
        if !self.is("(") || self.guard_delimiter == Some(self.delimiters) {
            return Ok(None);
        }
        let checkpoint = self.checkpoint();
        let head = match self.arrow_head() {
            Ok(head) => head,
            Err(error) if error.kind == FailureKind::ResourceLimit => return Err(error),
            Err(_) => {
                self.restore(checkpoint);
                return Ok(None);
            }
        };
        // The marker commits. A malformed body NEVER retries grouping.
        let body = if self.is("{") {
            self.block()?
        } else {
            self.expression()?
        };
        let start = self.nodes[head].span.start();
        Ok(Some(self.node(start, NodeKind::Arrow, vec![head, body])?))
    }

    fn wrapping(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let left = self.pipe()?;
        if self.eat("?")? {
            let right = if let Some(arrow) = self.arrow()? {
                arrow
            } else {
                self.pipe()?
            };
            return self.node(start, NodeKind::Wrap, vec![left, right]);
        }
        Ok(left)
    }

    fn pipe(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let mut input = self.binary(0)?;
        while self.eat("|>")? {
            let step = self.pipe_step()?;
            input = self.node(start, NodeKind::Pipe, vec![input, step])?;
        }
        Ok(input)
    }

    fn pipe_step(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let callable = if self.eat("(")? {
            let arrow = self.delimited(|p| p.arrow())?.ok_or_else(|| ParseFailure {
                kind: FailureKind::Syntax {
                    expected: "parenthesized arrow pipe step",
                },
                span: self.span(),
            })?;
            self.require(")")?;
            self.node(start, NodeKind::Group, vec![arrow])?
        } else {
            let mut callable = self.value_reference()?;
            while self.eat(".")? {
                let field = self.name()?;
                callable = self.node(start, NodeKind::Field, vec![callable, field])?;
            }
            callable
        };
        let mut children = vec![callable];
        if self.is("(") {
            children.push(self.delimited(Self::pipe_arguments)?);
        }
        let propagate = self.postfix_question();
        if propagate {
            self.bump()?;
        }
        self.node(start, NodeKind::PipeStep { propagate }, children)
    }

    fn pipe_arguments(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        self.require("(")?;
        let mut arguments = Vec::new();
        let mut placeholder = false;
        if !self.is(")") {
            loop {
                if self.lexed.tokens[self.at].kind == TokenKind::Placeholder {
                    if placeholder {
                        return self.error("at most one direct pipe placeholder");
                    }
                    placeholder = true;
                    let start = self.start();
                    self.bump()?;
                    arguments.push(self.node(start, NodeKind::PipePlaceholder, Vec::new())?);
                } else {
                    arguments.push(self.expression()?);
                }
                if !self.eat(",")? || self.is(")") {
                    break;
                }
            }
        }
        self.require(")")?;
        self.node(start, NodeKind::PipeArguments, arguments)
    }

    fn expression_start(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Identifier
                | TokenKind::Number(_)
                | TokenKind::TemplateOpen
                | TokenKind::Literal(crate::lexer::Literal {
                    mode: LiteralMode::String | LiteralMode::Character | LiteralMode::Raw { .. },
                    ..
                })
                | TokenKind::Keyword("true" | "false" | "if" | "match")
                | TokenKind::Punctuation("(" | "[" | "!" | "-" | "~" | "&" | "*")
        )
    }

    fn postfix_question(&self) -> bool {
        self.is("?") && !Self::expression_start(&self.lexed.tokens[self.at + 1].kind)
    }

    fn binary_operator(&self) -> Option<(&'static str, u8)> {
        let TokenKind::Punctuation(s) = self.lexed.tokens[self.at].kind else {
            return None;
        };
        if self.offset != 0 {
            return None;
        }
        let precedence = match s {
            "||" => 0,
            "&&" => 1,
            "|" => 2,
            "^" => 3,
            "&" => 4,
            "==" | "!=" => 5,
            "<" | "<=" | ">" | ">=" => 6,
            "<<" | ">>" => 7,
            "+" | "-" => 8,
            "*" | "/" | "%" => 9,
            _ => return None,
        };
        Some((s, precedence))
    }

    fn binary(&mut self, minimum: u8) -> Parsed<NodeId> {
        let start = self.start();
        let mut left = self.prefix()?;
        while let Some((operator, precedence)) = self.binary_operator() {
            if precedence < minimum {
                break;
            }
            self.bump()?;
            let right = self.nested(|p| p.binary(precedence + 1))?;
            left = self.node(start, NodeKind::Binary(operator), vec![left, right])?;
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        for operator in ["!", "-", "~", "&", "*"] {
            if self.eat(operator)? {
                let operator = if operator == "&" && self.eat("mut")? {
                    "&mut"
                } else {
                    operator
                };
                let operand = self.nested(Self::prefix)?;
                return self.node(start, NodeKind::Prefix(operator), vec![operand]);
            }
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let mut value = self.primary()?;
        loop {
            if self.eat("(")? {
                let mut children = vec![value];
                children.extend(self.delimited(|p| p.list(")", ",", true, Self::expression))?);
                value = self.node(start, NodeKind::Call, children)?;
            } else if self.eat("[")? {
                let index = self.delimited(Self::expression)?;
                self.require("]")?;
                value = self.node(start, NodeKind::Index, vec![value, index])?;
            } else if self.eat(".")? {
                let name = self.name()?;
                value = self.node(start, NodeKind::Field, vec![value, name])?;
            } else if self.postfix_question() {
                self.bump()?;
                value = self.node(start, NodeKind::Propagate, vec![value])?;
            } else {
                break;
            }
        }
        Ok(value)
    }

    fn ordinary_literal(&self) -> bool {
        matches!(
            &self.lexed.tokens[self.at].kind,
            TokenKind::Number(_)
                | TokenKind::Keyword("true" | "false")
                | TokenKind::Literal(crate::lexer::Literal {
                    mode: LiteralMode::String | LiteralMode::Character | LiteralMode::Raw { .. },
                    ..
                })
        )
    }

    fn primary(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        if self.ordinary_literal() {
            self.bump()?;
            return self.node(start, NodeKind::Literal, Vec::new());
        }
        if self.lexed.tokens[self.at].kind == TokenKind::TemplateOpen {
            return self.template(false);
        }
        if self.identifier() {
            let mut path = self.value_reference()?;
            if self.eat(".")? {
                let name = self.name()?;
                path = self.node(start, NodeKind::Field, vec![path, name])?;
            }
            if self.is("{") {
                return self.delimited(|p| p.construction(path));
            }
            return Ok(path);
        }
        if self.eat("(")? {
            let expression = self.delimited(Self::expression)?;
            self.require(")")?;
            return self.node(start, NodeKind::Group, vec![expression]);
        }
        if self.eat("[")? {
            let elements = self.delimited(|p| p.list("]", ",", true, Self::expression))?;
            return self.node(start, NodeKind::Bracket, elements);
        }
        if self.eat("if")? {
            self.require("(")?;
            let condition = self.delimited(Self::expression)?;
            self.require(")")?;
            let yes = self.expression()?;
            self.require("else")?;
            let no = self.expression()?;
            return self.node(start, NodeKind::IfExpression, vec![condition, yes, no]);
        }
        if self.eat("match")? {
            self.require("(")?;
            let scrutinee = self.delimited(Self::expression)?;
            self.require(")")?;
            self.require("{")?;
            let mut children = vec![scrutinee];
            children.extend(self.delimited(|p| p.list("}", ",", true, Self::match_arm))?);
            return self.node(start, NodeKind::Match, children);
        }
        self.error("expression")
    }

    fn match_arm(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let mut children = vec![self.pattern()?];
        if self.eat("if")? {
            let guard_start = self.start();
            let previous = self.guard_delimiter;
            self.guard_delimiter = Some(self.delimiters);
            let result = self.expression();
            self.guard_delimiter = previous;
            let guard = result?;
            children.push(self.node(guard_start, NodeKind::MatchGuard, vec![guard])?);
        }
        self.require("=>")?;
        children.push(if self.is("{") {
            self.block()?
        } else {
            self.expression()?
        });
        self.node(start, NodeKind::MatchArm, children)
    }

    fn construction(&mut self, path: NodeId) -> Parsed<NodeId> {
        let start = self.nodes[path].span.start();
        self.require("{")?;
        let mut children = vec![path];
        if self.eat("...")? {
            let spread_start = self.views.last().unwrap().span.start();
            let value = self.expression()?;
            children.push(self.node(spread_start, NodeKind::Spread, vec![value])?);
            if !self.is("}") {
                self.require(",")?;
            }
        }
        if !self.is("}") {
            loop {
                let field_start = self.start();
                let mut field = vec![self.name()?];
                let shorthand = !self.eat(":")?;
                if !shorthand {
                    field.push(self.expression()?);
                }
                children.push(self.node(
                    field_start,
                    NodeKind::InitializerField { shorthand },
                    field,
                )?);
                if !self.eat(",")? || self.is("}") {
                    break;
                }
            }
        }
        self.require("}")?;
        self.node(start, NodeKind::Construction, children)
    }

    fn template(&mut self, static_only: bool) -> Parsed<NodeId> {
        let start = self.start();
        self.bump()?; // M1 TemplateOpen
        let mut pieces = Vec::new();
        loop {
            let piece_start = self.start();
            match &self.lexed.tokens[self.at].kind {
                TokenKind::TemplateClose => {
                    self.bump()?;
                    break;
                }
                TokenKind::Literal(literal) if literal.mode == LiteralMode::TemplateText => {
                    self.bump()?;
                    pieces.push(self.node(piece_start, NodeKind::TemplateText, Vec::new())?);
                }
                TokenKind::HoleOpen if !static_only => {
                    self.bump()?;
                    let expression = self.delimited(Self::expression)?;
                    if self.lexed.tokens[self.at].kind != TokenKind::HoleClose {
                        return self.error("template hole end");
                    }
                    self.bump()?;
                    pieces.push(self.node(
                        piece_start,
                        NodeKind::TemplateHole,
                        vec![expression],
                    )?);
                }
                _ => return self.error("template text or closing delimiter"),
            }
        }
        self.node(start, NodeKind::Template, pieces)
    }

    fn pattern(&mut self) -> Parsed<NodeId> {
        self.nested(Self::pattern_inner)
    }

    fn pattern_inner(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        if self.lexed.tokens[self.at].kind == TokenKind::Placeholder {
            self.bump()?;
            return self.node(start, NodeKind::Wildcard, Vec::new());
        }
        if self.eat("-")? {
            let magnitude = self.pattern_magnitude()?;
            return self.node(start, NodeKind::NegatedPattern, vec![magnitude]);
        }
        if self.ordinary_literal() {
            self.bump()?;
            return self.node(start, NodeKind::Literal, Vec::new());
        }
        if self.lexed.tokens[self.at].kind == TokenKind::TemplateOpen {
            return self.template(true);
        }
        if self.eat("[")? {
            let items = self.list("]", ",", true, Self::pattern)?;
            return self.node(start, NodeKind::BracketPattern, items);
        }
        if self.eat("(")? {
            let inner = self.pattern()?;
            self.require(")")?;
            return self.node(start, NodeKind::GroupPattern, vec![inner]);
        }
        let mut children = Vec::new();
        if self.identifier() {
            let path = self.applied_path()?;
            children.push(self.node(start, NodeKind::PatternPath, path)?);
        }
        if self.eat("{")? {
            children.extend(self.list("}", ",", true, Self::pattern_field)?);
            return self.node(start, NodeKind::RecordPattern, children);
        }
        if children.is_empty() {
            return self.error("pattern");
        }
        if self.eat("(")? {
            children.extend(self.list(")", ",", true, Self::pattern)?);
            return self.node(start, NodeKind::PayloadPattern, children);
        }
        self.node(start, NodeKind::NamePattern, children)
    }

    fn pattern_field(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let mut children = vec![self.name()?];
        let shorthand = !self.eat(":")?;
        if !shorthand {
            children.push(self.pattern()?);
        }
        self.node(start, NodeKind::PatternField { shorthand }, children)
    }

    fn pattern_magnitude(&mut self) -> Parsed<NodeId> {
        self.nested(|p| {
            let start = p.start();
            if p.eat("(")? {
                let inner = p.pattern_magnitude()?;
                p.require(")")?;
                return p.node(start, NodeKind::GroupPattern, vec![inner]);
            }
            if !matches!(p.lexed.tokens[p.at].kind, TokenKind::Number(_)) {
                return p.error("numeric pattern magnitude");
            }
            p.bump()?;
            p.node(start, NodeKind::Literal, Vec::new())
        })
    }

    fn module(&mut self) -> Parsed<NodeId> {
        let mut items = Vec::new();
        while !self.eof() {
            items.push(self.item(true)?);
        }
        self.end = self.snapshot.bytes().len();
        self.node(0, NodeKind::Module, items)
    }

    fn item(&mut self, module: bool) -> Parsed<NodeId> {
        let start = self.start();
        if module && self.eat("export")? {
            if !["import", "function", "type", "enum"]
                .iter()
                .any(|s| self.is(s))
            {
                return self.error("exportable declaration");
            }
            let declaration = self.item(true)?;
            return self.node(start, NodeKind::Export, vec![declaration]);
        }
        if module && self.is("import") {
            return self.import();
        }
        if self.is("function") {
            return self.function(false, module);
        }
        if module && self.is("implement") {
            return self.function(true, module);
        }
        if self.is("type") {
            return self.record();
        }
        if self.is("enum") {
            return self.enumeration();
        }
        if module && self.eat("expect")? {
            let expression = self.expression()?;
            self.require(";")?;
            return self.node(start, NodeKind::Expect, vec![expression]);
        }
        self.statement()
    }

    fn import(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        self.require("import")?;
        let mut children = vec![self.path()?];
        if self.eat("::")? {
            let group_start = self.start();
            self.require("{")?;
            let items = self.list("}", ",", false, Self::import_item)?;
            children.push(self.node(group_start, NodeKind::ImportGroup, items)?);
        } else if self.eat("as")? {
            children.push(self.name()?);
        }
        self.require(";")?;
        self.node(start, NodeKind::Import, children)
    }

    fn import_item(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let mut children = vec![self.name()?];
        if self.eat("as")? {
            children.push(self.name()?);
        }
        self.node(start, NodeKind::ImportItem, children)
    }

    fn generic_parameters(&mut self, implementation: bool) -> Parsed<NodeId> {
        let start = self.start();
        self.require("<")?;
        let mut parameters = Vec::new();
        loop {
            let parameter_start = self.start();
            let mut children = vec![self.name()?];
            if !implementation && self.eat("=")? {
                children.push(self.ty()?);
            }
            parameters.push(self.node(parameter_start, NodeKind::GenericParameter, children)?);
            if !self.eat(",")? || self.closer(">") {
                break;
            }
        }
        self.close(">")?;
        self.node(
            start,
            NodeKind::GenericParameters { implementation },
            parameters,
        )
    }

    fn parameter_name(&mut self) -> Parsed<NodeId> {
        if self.lexed.tokens[self.at].kind == TokenKind::Placeholder {
            let start = self.start();
            self.bump()?;
            self.node(start, NodeKind::Wildcard, Vec::new())
        } else {
            self.name()
        }
    }

    fn value_parameter(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let name = self.parameter_name()?;
        self.require(":")?;
        let ty = self.ty()?;
        self.node(start, NodeKind::Parameter, vec![name, ty])
    }

    fn value_parameters(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        self.require("(")?;
        let parameters = self.list(")", ",", true, Self::value_parameter)?;
        self.node(start, NodeKind::ValueParameters, parameters)
    }

    fn requirements(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        self.require("where")?;
        let mut requirements = Vec::new();
        loop {
            let requirement_start = self.start();
            let mut children = vec![self.path()?];
            self.require("(")?;
            children.extend(self.list(")", ",", true, Self::ty)?);
            requirements.push(self.node(requirement_start, NodeKind::Requirement, children)?);
            if !self.eat(",")? || self.is("{") || self.is(";") {
                break;
            }
        }
        self.node(start, NodeKind::Requirements, requirements)
    }

    fn function(&mut self, implementation: bool, module: bool) -> Parsed<NodeId> {
        let start = self.start();
        self.bump()?;
        let mut signature = Vec::new();
        if implementation && self.is("<") {
            signature.push(self.generic_parameters(true)?);
        }
        signature.push(if implementation {
            self.path()?
        } else {
            self.name()?
        });
        if self.is("<") {
            signature.push(if implementation {
                self.type_arguments()?
            } else {
                self.generic_parameters(false)?
            });
        }
        signature.push(self.value_parameters()?);
        self.require(":")?;
        signature.push(self.ty()?);
        if self.is("where") {
            signature.push(self.requirements()?);
        }
        let signature = self.node(start, NodeKind::Signature, signature)?;
        let mut children = vec![signature];
        if module && !implementation && self.eat(";")? {
            return self.node(start, NodeKind::Function, children);
        }
        children.push(self.block()?);
        self.node(
            start,
            if implementation {
                NodeKind::Implementation
            } else {
                NodeKind::Function
            },
            children,
        )
    }

    fn record(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        self.require("type")?;
        let mut children = vec![self.name()?];
        if self.is("<") {
            children.push(self.generic_parameters(false)?);
        }
        self.require("=")?;
        let shape_start = self.start();
        self.require("{")?;
        let fields = self.list("}", ";", true, Self::record_field)?;
        children.push(self.node(shape_start, NodeKind::RecordShape, fields)?);
        self.eat(";")?;
        self.node(start, NodeKind::Record, children)
    }

    fn field(&mut self, allow_private: bool) -> Parsed<NodeId> {
        let start = self.start();
        let private = allow_private && self.eat("private")?;
        let name = self.name()?;
        self.require(":")?;
        let ty = self.ty()?;
        self.node(start, NodeKind::RecordField { private }, vec![name, ty])
    }

    fn record_field(&mut self) -> Parsed<NodeId> {
        self.field(true)
    }
    fn payload_field(&mut self) -> Parsed<NodeId> {
        self.field(false)
    }

    fn enumeration(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        self.require("enum")?;
        let mut children = vec![self.name()?];
        if self.is("<") {
            children.push(self.generic_parameters(false)?);
        }
        self.require("{")?;
        children.extend(self.list("}", ",", true, Self::enum_variant)?);
        self.eat(";")?;
        self.node(start, NodeKind::Enum, children)
    }

    fn enum_variant(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        let mut children = vec![self.name()?];
        let payload_start = self.start();
        if self.eat("(")? {
            let types = self.list(")", ",", false, Self::ty)?;
            children.push(self.node(payload_start, NodeKind::TuplePayload, types)?);
        } else if self.eat("{")? {
            let fields = self.list("}", ";", true, Self::payload_field)?;
            children.push(self.node(payload_start, NodeKind::RecordPayload, fields)?);
        }
        self.node(start, NodeKind::EnumVariant, children)
    }

    fn block(&mut self) -> Parsed<NodeId> {
        self.nested(|p| {
            p.delimited(|p| {
                let start = p.start();
                p.require("{")?;
                let mut items = Vec::new();
                while !p.is("}") {
                    if p.eof() {
                        return p.error("}");
                    }
                    items.push(p.item(false)?);
                }
                p.bump()?;
                p.node(start, NodeKind::Block, items)
            })
        })
    }

    fn binding(&mut self, guard: bool) -> Parsed<NodeId> {
        let start = self.start();
        self.require("let")?;
        let mutable = !guard && self.eat("mut")?;
        let mut children = vec![self.pattern()?];
        if self.eat(":")? {
            children.push(self.ty()?);
        }
        self.require("=")?;
        children.push(self.expression()?);
        if !guard {
            self.require(";")?;
        }
        self.node(
            start,
            if guard {
                NodeKind::GuardBinding
            } else {
                NodeKind::Binding { mutable }
            },
            children,
        )
    }

    fn if_statement(&mut self) -> Parsed<NodeId> {
        self.nested(|p| {
            let start = p.start();
            p.require("if")?;
            p.require("(")?;
            let mut children = vec![p.expression()?];
            p.require(")")?;
            children.push(p.block()?);
            if p.eat("else")? {
                children.push(if p.is("if") {
                    p.if_statement()?
                } else {
                    p.block()?
                });
            }
            p.node(start, NodeKind::IfStatement, children)
        })
    }

    fn assignment_operator(&self) -> Option<&'static str> {
        match self.lexed.tokens[self.at].kind {
            TokenKind::Punctuation(
                s @ ("=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>="),
            ) if self.offset == 0 || &s[self.offset..] == "=" => {
                Some(if self.offset == 0 { s } else { "=" })
            }
            _ => None,
        }
    }

    fn statement(&mut self) -> Parsed<NodeId> {
        let start = self.start();
        if self.is("{") {
            return self.block();
        }
        if self.is("if") {
            return self.if_statement();
        }
        if self.is("let") {
            return self.binding(false);
        }
        if self.is("expect") {
            return self.error("module-level expectation");
        }
        if self.eat("return")? {
            let children = if self.is(";") {
                Vec::new()
            } else {
                vec![self.expression()?]
            };
            self.require(";")?;
            return self.node(start, NodeKind::Return, children);
        }
        for (keyword, kind) in [("break", NodeKind::Break), ("continue", NodeKind::Continue)] {
            if self.eat(keyword)? {
                self.require(";")?;
                return self.node(start, kind, Vec::new());
            }
        }
        if self.eat("guard")? {
            let mut children = vec![if self.is("let") {
                self.binding(true)?
            } else {
                self.expression()?
            }];
            self.require("else")?;
            if self.eat("(")? {
                children.push(self.name()?);
                self.require(")")?;
            }
            children.push(self.block()?);
            return self.node(start, NodeKind::Guard, children);
        }
        if self.eat("while")? {
            self.require("(")?;
            let condition = self.expression()?;
            self.require(")")?;
            let body = self.block()?;
            return self.node(start, NodeKind::While, vec![condition, body]);
        }
        if self.eat("for")? {
            self.require("(")?;
            self.require("let")?;
            let pattern = self.pattern()?;
            self.require("of")?;
            let expression = self.expression()?;
            self.require(")")?;
            let body = self.block()?;
            return self.node(start, NodeKind::For, vec![pattern, expression, body]);
        }
        let left = self.expression()?;
        let mut children = vec![left];
        let kind = if let Some(operator) = self.assignment_operator() {
            self.bump()?;
            children.push(self.expression()?);
            NodeKind::Assignment(operator)
        } else {
            NodeKind::ExpressionStatement
        };
        self.require(";")?;
        self.node(start, kind, children)
    }

    fn validate_input(&mut self) -> Parsed<()> {
        // Merge the two ordered M1 streams; every original byte belongs to one
        // token or trivia span, and there is exactly one final EOF token.
        let mut token = 0;
        let mut trivia = 0;
        let mut end = 0;
        loop {
            self.charge()?;
            let t = self.lexed.tokens.get(token);
            let r = self.lexed.trivia.get(trivia);
            let span = match (t, r) {
                (Some(t), Some(r)) if r.span.start() < t.span.start() => {
                    trivia += 1;
                    &r.span
                }
                (Some(t), _) => {
                    token += 1;
                    if t.kind == TokenKind::Eof {
                        if token != self.lexed.tokens.len()
                            || r.is_some()
                            || t.span != self.snapshot.eof()
                            || end != self.snapshot.bytes().len()
                        {
                            return self.mismatch();
                        }
                        return Ok(());
                    }
                    &t.span
                }
                (None, _) => return self.mismatch(),
            };
            if span.snapshot().id() != self.snapshot.id()
                || span.start() != end
                || span.end() <= end
            {
                return self.mismatch();
            }
            end = span.end();
        }
    }

    fn mismatch<T>(&self) -> Parsed<T> {
        Err(ParseFailure {
            kind: FailureKind::InputMismatch,
            span: self.snapshot.eof(),
        })
    }

    fn span(&self) -> Span {
        let original = &self.lexed.tokens[self.at].span;
        self.snapshot
            .span(original.start() + self.offset, original.end())
            .unwrap()
    }

    fn start(&self) -> usize {
        self.span().start()
    }

    fn charge(&mut self) -> Parsed<()> {
        if self.work >= self.limits.max_work {
            return Err(ParseFailure {
                kind: FailureKind::ResourceLimit,
                span: if self.input_validated {
                    self.span()
                } else {
                    self.snapshot.eof()
                },
            });
        }
        self.work += 1;
        Ok(())
    }

    fn error<T>(&self, expected: &'static str) -> Parsed<T> {
        Err(ParseFailure {
            kind: FailureKind::Syntax { expected },
            span: self.span(),
        })
    }

    fn eof(&self) -> bool {
        self.lexed.tokens[self.at].kind == TokenKind::Eof
    }

    fn bump(&mut self) -> Parsed<()> {
        self.charge()?;
        let span = self.span();
        self.end = span.end();
        self.views.push(TokenView {
            token: self.at,
            span,
        });
        if !self.eof() {
            self.at += 1;
        }
        self.offset = 0;
        Ok(())
    }

    fn node(&mut self, start: usize, kind: NodeKind, children: Vec<NodeId>) -> Parsed<NodeId> {
        self.charge()?;
        let id = self.nodes.len();
        self.nodes.push(Node {
            kind,
            span: self.snapshot.span(start, self.end.max(start)).unwrap(),
            children,
        });
        Ok(id)
    }
}
