//! Original-byte lexer for the confirmed M1 policies in RFCs 0002/0003/0006/0010.
//! Template nesting uses an explicit stack, never Rust call-stack recursion.

use crate::source::{Snapshot, Span};
use crate::unicode;

const KEYWORDS: &[&str] = &[
    "as",
    "break",
    "continue",
    "else",
    "enum",
    "expect",
    "export",
    "false",
    "for",
    "function",
    "guard",
    "if",
    "implement",
    "import",
    "let",
    "match",
    "mut",
    "of",
    "once",
    "private",
    "return",
    "true",
    "type",
    "where",
    "while",
];
const PUNCTUATION: &[&str] = &[
    "<<=", ">>=", "...", "::", "=>", "==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "+=", "-=",
    "*=", "/=", "%=", "&=", "|=", "^=", "|>", "(", ")", "{", "}", "[", "]", ",", ":", ";", ".",
    "?", "=", "+", "-", "*", "/", "%", "!", "~", "&", "|", "^", "<", ">",
];
const INTEGER_SUFFIXES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LexStatus {
    Complete,
    Invalid,
    EncodingRejected,
    /// A host counter could not represent more nesting; not a source error.
    Incomplete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticKind {
    InvalidUtf8 { error_length: Option<usize> },
    InvalidScalar,
    UnclosedComment,
    UnclosedLiteral,
    UnclosedTemplate,
    UnclosedHole,
    InvalidNumber,
    InvalidEscape,
    ForbiddenControl,
    ForbiddenCommentControl,
    CharacterScalarCount,
}

impl DiagnosticKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidNumber
            | Self::InvalidEscape
            | Self::ForbiddenControl
            | Self::CharacterScalarCount => "R0002",
            _ => "R0001",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Bom,
    Space,
    Newline,
    LineComment,
    BlockComment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralMode {
    String,
    Character,
    Raw { hashes: usize },
    TemplateText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedScalar {
    pub value: char,
    pub source: Span,
}

/// Invalid contents remain invalid data, not a decoded accepted literal value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Literal {
    pub mode: LiteralMode,
    pub decoded: Vec<DecodedScalar>,
    pub valid: bool,
    pub terminated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberForm {
    DecimalDigits,
    Integer,
    Float,
}

/// The token span retains the full exact candidate; no magnitude is rounded or narrowed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Number {
    pub base: u32,
    pub form: NumberForm,
    pub suffix: Option<&'static str>,
    pub valid: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Identifier,
    Placeholder,
    Keyword(&'static str),
    Punctuation(&'static str),
    Number(Number),
    Literal(Literal),
    TemplateOpen,
    TemplateClose,
    HoleOpen,
    HoleClose,
    Invalid,
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lexed {
    pub status: LexStatus,
    pub tokens: Vec<Token>,
    pub trivia: Vec<Trivia>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn lex(snapshot: &Snapshot) -> Lexed {
    let text = match std::str::from_utf8(snapshot.bytes()) {
        Ok(text) => text,
        Err(error) => {
            let start = error.valid_up_to();
            let end = error
                .error_len()
                .map_or(snapshot.bytes().len(), |n| start + n);
            return Lexed {
                status: LexStatus::EncodingRejected,
                tokens: Vec::new(),
                trivia: Vec::new(),
                diagnostics: vec![Diagnostic {
                    kind: DiagnosticKind::InvalidUtf8 {
                        error_length: error.error_len(),
                    },
                    span: snapshot.span(start, end).unwrap(),
                }],
            };
        }
    };
    Scanner {
        snapshot,
        text,
        at: 0,
        frames: Vec::new(),
        result: Lexed {
            status: LexStatus::Complete,
            tokens: Vec::new(),
            trivia: Vec::new(),
            diagnostics: Vec::new(),
        },
    }
    .run()
}

#[derive(Clone, Copy)]
enum Frame {
    Template { start: usize },
    Hole { start: usize, depth: usize },
}

struct Scanner<'a> {
    snapshot: &'a Snapshot,
    text: &'a str,
    at: usize,
    frames: Vec<Frame>,
    result: Lexed,
}

fn forbidden(c: char) -> bool {
    (c <= '\u{1f}' && !matches!(c, '\t' | '\r' | '\n')) || ('\u{7f}'..='\u{9f}').contains(&c)
}

impl Scanner<'_> {
    fn span(&self, start: usize, end: usize) -> Span {
        self.snapshot.span(start, end).unwrap()
    }

    fn token(&mut self, start: usize, kind: TokenKind) {
        self.result.tokens.push(Token {
            kind,
            span: self.span(start, self.at),
        });
    }

    fn trivia(&mut self, start: usize, kind: TriviaKind) {
        self.result.trivia.push(Trivia {
            kind,
            span: self.span(start, self.at),
        });
    }

    fn error(&mut self, start: usize, end: usize, kind: DiagnosticKind) {
        self.result.diagnostics.push(Diagnostic {
            kind,
            span: self.span(start, end),
        });
    }

    fn rest(&self) -> &str {
        &self.text[self.at..]
    }

    fn next(&mut self) -> char {
        let c = self.rest().chars().next().unwrap();
        self.at += c.len_utf8();
        c
    }

    fn run(mut self) -> Lexed {
        while self.at < self.text.len() && self.result.status != LexStatus::Incomplete {
            let before = self.at;
            if matches!(self.frames.last(), Some(Frame::Template { .. })) {
                self.template();
            } else {
                self.code();
            }
            // Every path consumes at least one original byte or reports host failure.
            assert!(self.at > before || self.result.status == LexStatus::Incomplete);
        }
        if self.result.status != LexStatus::Incomplete {
            for frame in self.frames.clone().into_iter().rev() {
                let (start, kind) = match frame {
                    Frame::Template { start } => (start, DiagnosticKind::UnclosedTemplate),
                    Frame::Hole { start, .. } => (start, DiagnosticKind::UnclosedHole),
                };
                self.error(start, self.at, kind);
            }
            self.token(self.at, TokenKind::Eof);
            self.result.status = if self.result.diagnostics.is_empty() {
                LexStatus::Complete
            } else {
                LexStatus::Invalid
            };
        }
        self.result
    }

    fn code(&mut self) {
        let start = self.at;
        let c = self.rest().chars().next().unwrap();
        if start == 0 && c == '\u{feff}' {
            self.next();
            self.trivia(start, TriviaKind::Bom);
        } else if matches!(c, ' ' | '\t') {
            while self.rest().starts_with([' ', '\t']) {
                self.next();
            }
            self.trivia(start, TriviaKind::Space);
        } else if matches!(c, '\r' | '\n') {
            self.next();
            if c == '\r' && self.rest().starts_with('\n') {
                self.next();
            }
            self.trivia(start, TriviaKind::Newline);
        } else if self.rest().starts_with("//") {
            self.at += 2;
            while !self.rest().is_empty() && !self.rest().starts_with(['\r', '\n']) {
                self.comment_scalar();
            }
            self.trivia(start, TriviaKind::LineComment);
        } else if self.rest().starts_with("/*") {
            self.block_comment(start);
        } else if let Some(hashes) = self.raw_opener() {
            self.at += hashes + 2;
            self.literal(start, LiteralMode::Raw { hashes });
        } else if matches!(c, '\'' | '"') {
            self.next();
            self.literal(
                start,
                if c == '\'' {
                    LiteralMode::Character
                } else {
                    LiteralMode::String
                },
            );
        } else if c == '`' {
            self.next();
            self.frames.push(Frame::Template { start });
            self.token(start, TokenKind::TemplateOpen);
        } else if c.is_ascii_digit() {
            self.number(start);
        } else if unicode::start(c) {
            self.next();
            while self.rest().chars().next().is_some_and(unicode::continue_) {
                self.next();
            }
            let name = &self.text[start..self.at];
            let kind = if name == "_" {
                TokenKind::Placeholder
            } else if let Some(&keyword) = KEYWORDS.iter().find(|&&k| k == name) {
                TokenKind::Keyword(keyword)
            } else {
                TokenKind::Identifier
            };
            self.token(start, kind);
        } else {
            if let Some(Frame::Hole { depth, .. }) = self.frames.last_mut() {
                if c == '}' && *depth == 0 {
                    self.next();
                    self.frames.pop();
                    self.token(start, TokenKind::HoleClose);
                    return;
                }
                if c == '{' {
                    let Some(next) = depth.checked_add(1) else {
                        self.result.status = LexStatus::Incomplete;
                        return;
                    };
                    *depth = next;
                } else if c == '}' {
                    *depth -= 1;
                }
            }
            if let Some(&punctuation) = PUNCTUATION.iter().find(|&&p| self.rest().starts_with(p)) {
                self.at += punctuation.len();
                self.token(start, TokenKind::Punctuation(punctuation));
            } else {
                self.next();
                self.error(start, self.at, DiagnosticKind::InvalidScalar);
                self.token(start, TokenKind::Invalid);
            }
        }
    }

    fn comment_scalar(&mut self) {
        let start = self.at;
        if forbidden(self.next()) {
            self.error(start, self.at, DiagnosticKind::ForbiddenCommentControl);
        }
    }

    fn block_comment(&mut self, start: usize) {
        self.at += 2;
        let mut depth = 1usize;
        while self.at < self.text.len() {
            if self.rest().starts_with("/*") {
                let Some(next) = depth.checked_add(1) else {
                    self.result.status = LexStatus::Incomplete;
                    return;
                };
                depth = next;
                self.at += 2;
            } else if self.rest().starts_with("*/") {
                depth -= 1;
                self.at += 2;
                if depth == 0 {
                    break;
                }
            } else {
                self.comment_scalar();
            }
        }
        if depth != 0 {
            self.error(start, self.at, DiagnosticKind::UnclosedComment);
        }
        self.trivia(start, TriviaKind::BlockComment);
    }

    fn raw_opener(&self) -> Option<usize> {
        let rest = self.rest().as_bytes();
        if rest.first() != Some(&b'r') {
            return None;
        }
        let hashes = rest[1..].iter().take_while(|&&b| b == b'#').count();
        (rest.get(1 + hashes) == Some(&b'"')).then_some(hashes)
    }

    fn number(&mut self, start: usize) {
        if self.rest().starts_with("0b")
            || self.rest().starts_with("0o")
            || self.rest().starts_with("0x")
        {
            self.at += 2;
        } else {
            self.digit_run();
            if self.rest().starts_with('.')
                && self
                    .rest()
                    .as_bytes()
                    .get(1)
                    .is_some_and(u8::is_ascii_digit)
            {
                self.at += 1;
                self.digit_run();
            }
            if self.rest().starts_with(['e', 'E']) {
                self.at += 1;
                if self.rest().starts_with(['+', '-']) {
                    self.at += 1;
                }
                self.digit_run();
            }
        }
        while self.rest().chars().next().is_some_and(unicode::continue_) {
            self.next();
        }
        self.token(
            start,
            TokenKind::Number(validate_number(&self.text[start..self.at])),
        );
        if matches!(
            self.result.tokens.last().map(|t| &t.kind),
            Some(TokenKind::Number(Number { valid: false, .. }))
        ) {
            self.error(start, self.at, DiagnosticKind::InvalidNumber);
        }
    }

    fn digit_run(&mut self) {
        while self
            .rest()
            .starts_with(|c: char| c.is_ascii_digit() || c == '_')
        {
            self.at += 1;
        }
    }

    fn literal(&mut self, start: usize, mode: LiteralMode) {
        let errors = self.result.diagnostics.len();
        let mut decoded = Vec::new();
        let mut terminated = false;
        while self.at < self.text.len() {
            let close = match mode {
                LiteralMode::Raw { hashes } => {
                    self.rest().starts_with('"')
                        && self.rest().as_bytes()[1..]
                            .iter()
                            .take(hashes)
                            .take_while(|&&b| b == b'#')
                            .count()
                            == hashes
                }
                LiteralMode::Character => self.rest().starts_with('\''),
                _ => self.rest().starts_with('"'),
            };
            if close {
                self.at += match mode {
                    LiteralMode::Raw { hashes } => hashes + 1,
                    _ => 1,
                };
                terminated = true;
                break;
            }
            if !matches!(mode, LiteralMode::Raw { .. }) && self.rest().starts_with(['\r', '\n']) {
                break;
            }
            self.content(&mut decoded, mode);
        }
        if !terminated {
            self.error(start, self.at, DiagnosticKind::UnclosedLiteral);
        }
        if terminated && mode == LiteralMode::Character && decoded.len() != 1 {
            self.error(start, self.at, DiagnosticKind::CharacterScalarCount);
        }
        let valid = errors == self.result.diagnostics.len();
        self.token(
            start,
            TokenKind::Literal(Literal {
                mode,
                decoded,
                valid,
                terminated,
            }),
        );
    }

    fn template(&mut self) {
        let start = self.at;
        if self.rest().starts_with('`') {
            self.next();
            self.frames.pop();
            self.token(start, TokenKind::TemplateClose);
        } else if self.rest().starts_with("${") {
            self.at += 2;
            self.frames.push(Frame::Hole { start, depth: 0 });
            self.token(start, TokenKind::HoleOpen);
        } else {
            let errors = self.result.diagnostics.len();
            let mut decoded = Vec::new();
            while !self.rest().is_empty()
                && !self.rest().starts_with('`')
                && !self.rest().starts_with("${")
            {
                self.content(&mut decoded, LiteralMode::TemplateText);
            }
            let terminated = self.at < self.text.len();
            self.token(
                start,
                TokenKind::Literal(Literal {
                    mode: LiteralMode::TemplateText,
                    decoded,
                    valid: errors == self.result.diagnostics.len() && terminated,
                    terminated,
                }),
            );
        }
    }

    fn content(&mut self, decoded: &mut Vec<DecodedScalar>, mode: LiteralMode) {
        let start = self.at;
        let c = self.next();
        if c == '\\' && !matches!(mode, LiteralMode::Raw { .. }) {
            self.escape(start, decoded, mode == LiteralMode::TemplateText);
            return;
        }
        if forbidden(c) {
            self.error(start, self.at, DiagnosticKind::ForbiddenControl);
        }
        let value = if c == '\r' {
            if self.rest().starts_with('\n') {
                self.at += 1;
            }
            '\n'
        } else {
            c
        };
        decoded.push(DecodedScalar {
            value,
            source: self.span(start, self.at),
        });
    }

    fn escape(&mut self, start: usize, decoded: &mut Vec<DecodedScalar>, template: bool) {
        // A backslash cannot consume a physical ending in an ordinary quote.
        if self.rest().is_empty() || self.rest().starts_with(['\r', '\n']) {
            self.error(start, self.at, DiagnosticKind::InvalidEscape);
            return;
        }
        let c = self.next();
        let value = match c {
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            '0' => Some('\0'),
            '\\' => Some('\\'),
            '\'' => Some('\''),
            '"' => Some('"'),
            '`' if template => Some('`'),
            '$' if template && self.rest().starts_with('{') => {
                self.at += 1;
                let source = self.span(start, self.at);
                decoded.push(DecodedScalar {
                    value: '$',
                    source: source.clone(),
                });
                decoded.push(DecodedScalar { value: '{', source });
                return;
            }
            'x' => {
                let mut value = 0;
                let mut count = 0;
                while count < 2 {
                    let Some(digit) = self
                        .rest()
                        .chars()
                        .next()
                        .and_then(|c| c.to_digit(16).filter(|_| c.is_ascii()))
                    else {
                        break;
                    };
                    self.next();
                    value = value * 16 + digit;
                    count += 1;
                }
                if count == 2 && value <= 0x7f {
                    char::from_u32(value)
                } else {
                    None
                }
            }
            'u' if self.rest().starts_with('{') => {
                self.at += 1;
                let mut value = Some(0u32);
                let mut count = 0usize;
                while let Some(digit) = self
                    .rest()
                    .chars()
                    .next()
                    .and_then(|c| c.to_digit(16).filter(|_| c.is_ascii()))
                {
                    self.next();
                    value = value.and_then(|v| v.checked_mul(16)?.checked_add(digit));
                    count += 1;
                }
                let closed = self.rest().starts_with('}');
                if closed {
                    self.at += 1;
                }
                if closed && (1..=6).contains(&count) {
                    value.and_then(char::from_u32)
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(value) = value {
            decoded.push(DecodedScalar {
                value,
                source: self.span(start, self.at),
            });
        } else {
            self.error(start, self.at, DiagnosticKind::InvalidEscape);
        }
    }
}

/// Consume one complete digit/underscore sequence and validate separators.
fn digits(bytes: &[u8], at: &mut usize, base: u32) -> bool {
    let start = *at;
    let mut previous_digit = false;
    let mut valid = true;
    while let Some(&b) = bytes.get(*at) {
        if b == b'_' {
            valid &= previous_digit;
            previous_digit = false;
        } else if (b as char).is_digit(base) {
            previous_digit = true;
        } else {
            break;
        }
        *at += 1;
    }
    valid && previous_digit && *at > start
}

fn validate_number(text: &str) -> Number {
    let bytes = text.as_bytes();
    let base = if text.starts_with("0b") {
        2
    } else if text.starts_with("0o") {
        8
    } else if text.starts_with("0x") {
        16
    } else {
        10
    };
    let mut at = if base == 10 { 0 } else { 2 };
    let mut valid = digits(bytes, &mut at, base);
    let mut form = if base == 10 {
        NumberForm::DecimalDigits
    } else {
        NumberForm::Integer
    };
    if base == 10 {
        if bytes.get(at) == Some(&b'.') {
            form = NumberForm::Float;
            at += 1;
            valid &= digits(bytes, &mut at, 10);
        }
        if matches!(bytes.get(at), Some(b'e' | b'E')) {
            form = NumberForm::Float;
            at += 1;
            if matches!(bytes.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            valid &= digits(bytes, &mut at, 10);
        }
    }
    let tail = &text[at..];
    let suffix = INTEGER_SUFFIXES
        .iter()
        .copied()
        .find(|&s| s == tail)
        .or_else(|| ["f32", "f64"].into_iter().find(|&s| s == tail));
    if let Some(suffix) = suffix {
        if suffix.starts_with('f') {
            valid &= base == 10;
            form = NumberForm::Float;
        } else {
            valid &= form != NumberForm::Float;
            form = NumberForm::Integer;
        }
    } else {
        valid &= tail.is_empty();
    }
    Number {
        base,
        form,
        suffix,
        valid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_counter_failure_is_incomplete_not_empty_success() {
        // Inject a counter at the host boundary without allocating impossible input.
        let snapshot = Snapshot::capture("synthetic://counter", b"{").unwrap();
        let result = Scanner {
            snapshot: &snapshot,
            text: "{",
            at: 0,
            frames: vec![Frame::Hole {
                start: 0,
                depth: usize::MAX,
            }],
            result: Lexed {
                status: LexStatus::Complete,
                tokens: Vec::new(),
                trivia: Vec::new(),
                diagnostics: Vec::new(),
            },
        }
        .run();
        assert_eq!(result.status, LexStatus::Incomplete);
        assert!(result.diagnostics.is_empty()); // Not a fabricated source failure.
        assert!(result.tokens.is_empty()); // No invented EOF/completed token stream.
    }
}
