//! Direct public-interface acceptance for M1. Case provenance: baseline RFC 0002
//! X1-X4, RFC 0003 L1-L17, RFC 0006 S1-S20, RFC 0010 Y1-Y5.
//! These are lexical/source tests, not Rill program execution or source checking.

use compiler::lexer::*;
use compiler::source::*;

fn snapshot(bytes: impl AsRef<[u8]>) -> Snapshot {
    Snapshot::capture("synthetic://m1", bytes.as_ref()).unwrap()
}

fn scan(text: &str) -> Lexed {
    lex(&snapshot(text))
}

fn spelling(token: &Token) -> &str {
    std::str::from_utf8(token.span.bytes()).unwrap()
}

fn ordinary(result: &Lexed) -> Vec<&Token> {
    result
        .tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Eof)
        .collect()
}

fn literal(token: &Token) -> &Literal {
    let TokenKind::Literal(value) = &token.kind else {
        panic!("expected literal")
    };
    value
}

fn decoded(value: &Literal) -> String {
    value.decoded.iter().map(|p| p.value).collect()
}

fn extent(span: &Span) -> (usize, usize) {
    (span.start(), span.end())
}

fn assert_error(text: &str, kind: DiagnosticKind, range: (usize, usize)) {
    let result = scan(text);
    assert_eq!(result.status, LexStatus::Invalid, "{text:?}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.kind == kind && extent(&d.span) == range),
        "{text:?}"
    );
}

#[test]
fn x1_x3_snapshots_spans_and_eof() {
    let mut input = b"\xef\xbb\xbflet".to_vec();
    let saved = snapshot(&input);
    let id = saved.id();
    input.fill(b'x');
    assert_eq!(saved.bytes(), b"\xef\xbb\xbflet");
    assert_eq!(saved.identity(), "synthetic://m1");
    assert_eq!(saved.clone().id(), id);
    assert_ne!(snapshot(&input).id(), id);
    assert_ne!(
        Snapshot::capture("different", saved.bytes()).unwrap().id(),
        id
    );
    assert_ne!(snapshot(saved.bytes()).id(), id);
    assert_eq!(saved.span(4, 3), Err(SourceError::InvalidRange));
    assert_eq!(saved.span(0, 7), Err(SourceError::InvalidRange));
    assert_eq!(
        saved.span(usize::MAX, usize::MAX),
        Err(SourceError::InvalidRange)
    );
    assert_eq!(extent(&saved.eof()), (6, 6));
    assert_eq!(saved.span(1, 1).unwrap().bytes(), b"");
    let result = lex(&saved);
    assert_eq!(result.status, LexStatus::Complete);
    assert_eq!(result.trivia[0].kind, TriviaKind::Bom);
    assert_eq!(extent(&result.trivia[0].span), (0, 3));
    assert_eq!(result.tokens[0].kind, TokenKind::Keyword("let"));
    assert_eq!(extent(&result.tokens[0].span), (3, 6));
    assert_eq!(result.tokens.last().unwrap().span, saved.eof());
    assert_eq!(saved.coordinate(3).unwrap().human(), Some((1, 2)));
}

#[test]
fn x2_l15_strict_utf8_every_mode_and_first_invalid_extent() {
    // Includes overlong, surrogate, above-max, isolated continuation, valid
    // incomplete prefixes, and invalid following bytes at each sequence depth.
    let failures: &[(&[u8], usize, Option<usize>)] = &[
        (&[0xc0, 0xaf], 1, Some(1)),
        (&[0x80], 1, Some(1)),
        (&[0xed, 0xa0, 0x80], 1, Some(1)),
        (&[0xf4, 0x90, 0x80, 0x80], 1, Some(1)),
        (&[0xe1, 0x80, b'A'], 2, Some(2)),
        (&[0xf0, 0x90, 0x80, b'A'], 3, Some(3)),
        (&[0xe0, 0x80, 0x80], 1, Some(1)),
        (&[0xf5, 0x80], 1, Some(1)),
        (&[0xc2], 1, None),
        (&[0xe1, 0x80], 2, None),
        (&[0xf0, 0x90, 0x80], 3, None),
    ];
    for prefix in [b"".as_slice(), b"//", b"/*", b"\"", b"r#\"", b"`", b"`${"] {
        for &(bad, length, error_length) in failures {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(bad);
            let result = lex(&snapshot(&bytes));
            assert_eq!(result.status, LexStatus::EncodingRejected);
            assert!(result.tokens.is_empty() && result.trivia.is_empty());
            assert_eq!(result.diagnostics.len(), 1);
            let error = &result.diagnostics[0];
            assert_eq!(error.kind, DiagnosticKind::InvalidUtf8 { error_length });
            assert_eq!(error.kind.code(), "R0001");
            assert_eq!(extent(&error.span), (prefix.len(), prefix.len() + length));
            assert!(error.span.coordinates().is_none());
        }
    }
    let valid = lex(&snapshot([0xf4, 0x8f, 0xbf, 0xbf]));
    assert_ne!(valid.status, LexStatus::EncodingRejected); // U+10FFFF is a scalar.
}

#[test]
fn l1_l3_l17_identifiers_keywords_and_exact_spelling() {
    let text = "letter _value value2 _ functionality function Function café é e\u{301} i32 std NaN inf self super";
    let result = scan(text);
    assert_eq!(result.status, LexStatus::Complete);
    let tokens = ordinary(&result);
    assert_eq!(
        tokens.iter().map(|t| spelling(t)).collect::<Vec<_>>(),
        text.split(' ').collect::<Vec<_>>()
    );
    for (i, token) in tokens.iter().enumerate() {
        assert_eq!(
            token.kind,
            match i {
                3 => TokenKind::Placeholder,
                5 => TokenKind::Keyword("function"),
                _ => TokenKind::Identifier,
            }
        );
    }
    assert_ne!(tokens[8].span.bytes(), tokens[9].span.bytes());
    let keywords = "as break continue else enum expect export false for function guard if implement import let match mut of once private return true type where while";
    let result = scan(keywords);
    assert_eq!(result.status, LexStatus::Complete);
    assert_eq!(ordinary(&result).len(), keywords.split(' ').count());
    for (token, expected) in ordinary(&result).iter().zip(keywords.split(' ')) {
        assert!(matches!(token.kind, TokenKind::Keyword(k) if k == expected));
    }
}

#[test]
fn unicode17_membership_not_host_classification() {
    // Unicode 17 additions: Tolong Siki, Beria Erfe and CJK Extension J.
    for c in [
        '\u{11db0}',
        '\u{11ddb}',
        '\u{16ea0}',
        '\u{16ed3}',
        '\u{323b0}',
        '\u{33479}',
        '\u{31350}',
        '℘',
        'é',
    ] {
        let result = scan(&c.to_string());
        assert_eq!(result.status, LexStatus::Complete, "U+{:X}", c as u32);
        assert_eq!(result.tokens[0].kind, TokenKind::Identifier);
    }
    for c in [
        '\u{301}',
        '\u{30}',
        '\u{200c}',
        '\u{200d}',
        '\u{feff}',
        '\u{1f642}',
        '\u{11daf}',
        '\u{11ddc}',
        '\u{16eb9}',
        '\u{16ed4}',
        '\u{3347a}',
    ] {
        // U+FEFF only has its special trivia role at byte zero.
        let result = scan(&format!(" {c}"));
        if c == '0' {
            assert!(matches!(result.tokens[0].kind, TokenKind::Number(_)));
        } else {
            assert_eq!(result.tokens[0].kind, TokenKind::Invalid);
        }
    }
    for c in ['\u{301}', '0', '_', '\u{200c}', '\u{200d}', '\u{11de0}'] {
        let result = scan(&format!("a{c}"));
        assert_eq!(result.status, LexStatus::Complete);
        assert_eq!(ordinary(&result).len(), 1);
    }
}

#[test]
fn l5_l7_l12_l14_trivia_nested_comments_and_physical_endings() {
    let result = scan("/* outer /* inner */ end */name / * name\r// quote \" and /* text\r\n\n");
    assert_eq!(result.status, LexStatus::Complete);
    assert_eq!(result.trivia[0].kind, TriviaKind::BlockComment);
    assert_eq!(
        result.trivia[0].span.bytes(),
        b"/* outer /* inner */ end */"
    );
    assert_eq!(
        ordinary(&result)
            .iter()
            .map(|t| spelling(t))
            .collect::<Vec<_>>(),
        ["name", "/", "*", "name"]
    );
    let newlines: Vec<_> = result
        .trivia
        .iter()
        .filter(|t| t.kind == TriviaKind::Newline)
        .map(|t| t.span.bytes())
        .collect();
    assert_eq!(newlines, [b"\r".as_slice(), b"\r\n", b"\n"]);
    let line = result
        .trivia
        .iter()
        .find(|t| t.kind == TriviaKind::LineComment)
        .unwrap();
    assert_eq!(line.span.bytes(), b"// quote \" and /* text");
    assert_error("/* a /* b */", DiagnosticKind::UnclosedComment, (0, 12));
    assert_eq!(scan("// EOF").status, LexStatus::Complete);
    assert_eq!(scan("/* ` \" // */").status, LexStatus::Complete);
}

#[test]
fn l8_l11_longest_punctuation_and_no_parser_views() {
    let result = scan(
        "&mut & mut &mutvalue Box<Box<i32>>=value left > /* comment */ > right value? /* gap */ -1i32",
    );
    assert_eq!(result.status, LexStatus::Complete);
    let tokens = ordinary(&result);
    assert_eq!(
        tokens.iter().map(|t| spelling(t)).collect::<Vec<_>>(),
        [
            "&", "mut", "&", "mut", "&", "mutvalue", "Box", "<", "Box", "<", "i32", ">>=", "value",
            "left", ">", ">", "right", "value", "?", "-", "1i32"
        ]
    );
    assert_eq!(tokens[11].kind, TokenKind::Punctuation(">>="));
    let punctuation = "<<= >>= ... :: => == != <= >= && || << >> += -= *= /= %= &= |= ^= |> ( ) { } [ ] , : ; . ? = + - * / % ! ~ & | ^ < >";
    let result = scan(punctuation);
    assert_eq!(result.status, LexStatus::Complete);
    assert_eq!(ordinary(&result).len(), punctuation.split(' ').count());
    for (token, expected) in ordinary(&result).iter().zip(punctuation.split(' ')) {
        assert!(matches!(token.kind, TokenKind::Punctuation(p) if p == expected));
    }
    let result = scan("++ -- / * < <");
    assert_eq!(
        ordinary(&result)
            .iter()
            .map(|t| spelling(t))
            .collect::<Vec<_>>(),
        ["+", "+", "-", "-", "/", "*", "<", "<"]
    );
}

#[test]
fn l13_l16_unknown_scalars_are_whole_original_units() {
    let result = scan("a\u{feff}@#🙂\u{2028}\\");
    assert_eq!(result.status, LexStatus::Invalid);
    let expected = [(1, 4), (4, 5), (5, 6), (6, 10), (10, 13), (13, 14)];
    assert_eq!(result.diagnostics.len(), expected.len());
    for (error, expected) in result.diagnostics.iter().zip(expected) {
        assert_eq!(error.kind, DiagnosticKind::InvalidScalar);
        assert_eq!(error.kind.code(), "R0001");
        assert_eq!(extent(&error.span), expected);
    }
}

#[test]
fn s1_s6_s7_numbers_preserve_spelling_base_suffix_and_magnitude() {
    let text = "0xff32 0xff32u32 0b10_01u8 0o77i16 01 1_234 1.0f32 1e-2f32 1E+2 1f64 128i8 999999999999999999999999999999999999999999999999u128";
    let result = scan(text);
    assert_eq!(result.status, LexStatus::Complete);
    let tokens = ordinary(&result);
    assert_eq!(
        tokens.iter().map(|t| spelling(t)).collect::<Vec<_>>(),
        text.split(' ').collect::<Vec<_>>()
    );
    let expected = [
        (16, NumberForm::Integer, None),
        (16, NumberForm::Integer, Some("u32")),
        (2, NumberForm::Integer, Some("u8")),
        (8, NumberForm::Integer, Some("i16")),
        (10, NumberForm::DecimalDigits, None),
        (10, NumberForm::DecimalDigits, None),
        (10, NumberForm::Float, Some("f32")),
        (10, NumberForm::Float, Some("f32")),
        (10, NumberForm::Float, None),
        (10, NumberForm::Float, Some("f64")),
        (10, NumberForm::Integer, Some("i8")),
        (10, NumberForm::Integer, Some("u128")),
    ];
    for (token, (base, form, suffix)) in tokens.iter().zip(expected) {
        assert_eq!(
            token.kind,
            TokenKind::Number(Number {
                base,
                form,
                suffix,
                valid: true
            })
        );
    }
    let result = scan("-(128i8) 1e-2f32 - 1i32");
    assert_eq!(result.status, LexStatus::Complete);
    assert_eq!(
        ordinary(&result)
            .iter()
            .map(|t| spelling(t))
            .collect::<Vec<_>>(),
        ["-", "(", "128i8", ")", "1e-2f32", "-", "1i32"]
    );
}

#[test]
fn numeric_suffix_and_separator_boundaries() {
    for suffix in [
        "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128",
    ] {
        for (body, base) in [("0", 10), ("0xF_F", 16), ("0b1_0", 2), ("0o7_0", 8)] {
            let result = scan(&format!("{body}{suffix}"));
            assert_eq!(result.status, LexStatus::Complete);
            assert_eq!(
                result.tokens[0].kind,
                TokenKind::Number(Number {
                    base,
                    form: NumberForm::Integer,
                    suffix: Some(suffix),
                    valid: true
                })
            );
        }
    }
    for text in [
        "1i256",
        "1u0",
        "1F32",
        "1.2_f32",
        "1.2e+_2",
        "0O7",
        "0B10",
        "0x1u8tail",
    ] {
        assert_error(text, DiagnosticKind::InvalidNumber, (0, text.len()));
    }
}

#[test]
fn s2_s3_invalid_numeric_candidates_never_salvage_prefixes() {
    for text in [
        "123name",
        "1i32tail",
        "1é",
        "0b102",
        "0Xff",
        "1e+",
        "1__2",
        "1_f32",
        "1.2u8",
        "0x",
        "0b_1",
        "0o8",
        "0x1f32tail",
        "1_",
        "1e_2",
        "1e2_",
        "0b1f32",
        "1.0_",
        "0x_1",
    ] {
        let result = scan(text);
        assert_eq!(result.status, LexStatus::Invalid, "{text}");
        assert_eq!(ordinary(&result).len(), 1, "{text}");
        assert_eq!(spelling(&result.tokens[0]), text);
        assert!(matches!(
            result.tokens[0].kind,
            TokenKind::Number(Number { valid: false, .. })
        ));
        assert_eq!(result.diagnostics[0].kind, DiagnosticKind::InvalidNumber);
        assert_eq!(result.diagnostics[0].kind.code(), "R0002");
        assert_eq!(extent(&result.diagnostics[0].span), (0, text.len()));
    }
}

#[test]
fn s4_s5_numeric_dots_and_trivia_boundaries() {
    let result = scan(".5 1. 1..2 1...rest 1.0f32.field 1/*gap*/name 1e -2");
    assert_eq!(
        ordinary(&result)
            .iter()
            .map(|t| spelling(t))
            .collect::<Vec<_>>(),
        [
            ".", "5", "1", ".", "1", ".", ".", "2", "1", "...", "rest", "1.0f32", ".", "field",
            "1", "name", "1e", "-", "2"
        ]
    );
    assert_eq!(result.diagnostics.len(), 1);
    assert_eq!(result.diagnostics[0].span.bytes(), b"1e");
    assert_eq!(
        scan(".5 1. 1..2 1...rest 1.0f32.field 1._2 0x1f64f64").status,
        LexStatus::Complete
    );
}

#[test]
fn s8_s11_raw_delimiters_partial_closers_and_no_interpolation() {
    let result = scan("r##\"a\"#b\"## r#\"x\"## r \"x\" \"${name}\" r\"${name}\"");
    assert_eq!(result.status, LexStatus::Invalid); // The extra # after r#"x"#.
    let tokens = ordinary(&result);
    let raw = literal(tokens[0]);
    assert_eq!(raw.mode, LiteralMode::Raw { hashes: 2 });
    assert_eq!(decoded(raw), "a\"#b");
    assert!(raw.valid && raw.terminated);
    assert_eq!(tokens[1].span.bytes(), b"r#\"x\"#");
    assert_eq!(tokens[2].kind, TokenKind::Invalid);
    assert_eq!(tokens[3].kind, TokenKind::Identifier);
    assert_eq!(decoded(literal(tokens[4])), "x");
    assert_eq!(decoded(literal(tokens[5])), "${name}");
    assert_eq!(decoded(literal(tokens[6])), "${name}");
    for text in ["\"\"", "r\"\"", "r####\"\"####"] {
        let result = scan(text);
        assert_eq!(result.status, LexStatus::Complete);
        assert!(literal(&result.tokens[0]).decoded.is_empty());
    }
    let result = scan("r\"\\n/* ${ }\"");
    assert_eq!(decoded(literal(&result.tokens[0])), "\\n/* ${ }");
}

#[test]
fn s15_s17_fixed_escapes_scalar_counts_and_provenance() {
    let result = scan(r#""\x414" '\u{1F642}' "\n\r\t\0\\\'\"\u{10ffff}""#);
    assert_eq!(result.status, LexStatus::Complete);
    let first = literal(&result.tokens[0]);
    assert_eq!(decoded(first), "A4");
    assert_eq!(extent(&first.decoded[0].source), (1, 5));
    assert_eq!(extent(&first.decoded[1].source), (5, 6));
    assert_eq!(decoded(literal(&result.tokens[1])), "🙂");
    assert_eq!(
        decoded(literal(&result.tokens[2])),
        "\n\r\t\0\\'\"\u{10ffff}"
    );
    for text in [r"'\x414'", r"'e\u{301}'", "''", "'ab'"] {
        assert_error(text, DiagnosticKind::CharacterScalarCount, (0, text.len()));
    }
    for text in [
        r#""\q""#,
        r#""\x80""#,
        r#""\u{}""#,
        r"'\u{D800}'",
        r#""\u{110000}""#,
        r#""\u{0000000}""#,
        r#""\x4""#,
        r#""\u{41""#,
        r#""\$""#,
        r#""\`""#,
        r#""\u{1_2}""#,
    ] {
        let result = scan(text);
        assert_eq!(result.status, LexStatus::Invalid, "{text}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::InvalidEscape)
        );
        assert!(!literal(&result.tokens[0]).valid);
        assert!(literal(&result.tokens[0]).terminated);
    }
}

#[test]
fn decoded_scalar_provenance_retains_multibyte_source() {
    let result = scan("\"é🙂\" '🙂' '\\u{0}' '\\u{ffff}'");
    assert_eq!(result.status, LexStatus::Complete);
    let value = literal(&result.tokens[0]);
    assert_eq!(decoded(value), "é🙂");
    assert_eq!(extent(&value.decoded[0].source), (1, 3));
    assert_eq!(extent(&value.decoded[1].source), (3, 7));
    assert_eq!(decoded(literal(&result.tokens[1])), "🙂");
    assert_eq!(decoded(literal(&result.tokens[2])), "\0");
    assert_eq!(decoded(literal(&result.tokens[3])), "\u{ffff}");
    assert!(value.decoded[1].source.snapshot().coordinate(4).is_none());
}

#[test]
fn s18_control_admission_in_all_content_modes() {
    for c in [
        '\0', '\u{1}', '\u{b}', '\u{c}', '\u{1f}', '\u{7f}', '\u{85}', '\u{9f}',
    ] {
        for text in [
            format!("//{c}"),
            format!("/*{c}*/"),
            format!("r\"{c}\""),
            format!("\"{c}\""),
            format!("'{c}'"),
            format!("`{c}`"),
        ] {
            let result = scan(&text);
            assert_eq!(result.status, LexStatus::Invalid);
            let in_comment = text.starts_with('/');
            let expected = if in_comment {
                DiagnosticKind::ForbiddenCommentControl
            } else {
                DiagnosticKind::ForbiddenControl
            };
            let error = result
                .diagnostics
                .iter()
                .find(|d| d.kind == expected)
                .unwrap();
            assert_eq!(error.span.bytes(), c.to_string().as_bytes());
            assert_eq!(
                error.kind.code(),
                if in_comment { "R0001" } else { "R0002" }
            );
        }
    }
    for text in [
        "//\t\u{2028}\u{2029}\u{feff}",
        "r\"\t\r\n\u{2028}\u{2029}\u{feff}\"",
        r#""\0\x7f\u{85}""#,
    ] {
        assert_eq!(scan(text).status, LexStatus::Complete);
    }
}

#[test]
fn s19_y5_physical_content_and_original_byte_mapping() {
    let result = scan("r\"a\r\nb\rc\nd\" `\r\n\r\\r`");
    assert_eq!(result.status, LexStatus::Complete);
    let value = literal(&result.tokens[0]);
    assert_eq!(decoded(value), "a\nb\nc\nd");
    assert_eq!(extent(&value.decoded[1].source), (3, 5));
    assert_eq!(extent(&value.decoded[3].source), (6, 7));
    let value = literal(&result.tokens[2]);
    assert_eq!(decoded(value), "\n\n\r");
    assert_eq!(value.decoded[0].source.bytes(), b"\r\n");
    assert_eq!(value.decoded[1].source.bytes(), b"\r");
    assert_eq!(value.decoded[2].source.bytes(), b"\\r");
    assert_eq!(extent(&result.tokens[0].span), (0, 11));
}

#[test]
fn quoted_unclosed_recovery_stops_before_physical_ending() {
    for ending in ["\r", "\n", "\r\n"] {
        for prefix in ["\"abc", "'a", "\"abc\\"] {
            let text = format!("{prefix}{ending}let");
            let result = scan(&text);
            assert_eq!(result.status, LexStatus::Invalid);
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.kind == DiagnosticKind::UnclosedLiteral
                        && extent(&d.span) == (0, prefix.len()))
            );
            assert!(!literal(&result.tokens[0]).terminated);
            assert_eq!(result.trivia[0].kind, TriviaKind::Newline);
            assert_eq!(result.trivia[0].span.bytes(), ending.as_bytes());
            assert_eq!(result.tokens[1].kind, TokenKind::Keyword("let"));
        }
    }
    assert_error("\"abc", DiagnosticKind::UnclosedLiteral, (0, 4));
}

#[test]
fn s12_templates_holes_nested_modes_and_brace_ownership() {
    let text = "`hi ${Point { x: \"}\", y: r#\"}\"#, z: `nested ${v}` /* } */ // }\n }} bye`";
    let result = scan(text);
    assert_eq!(result.status, LexStatus::Complete);
    let kinds: Vec<_> = result.tokens.iter().map(|t| &t.kind).collect();
    assert_eq!(
        kinds.iter().filter(|&&k| *k == TokenKind::HoleOpen).count(),
        2
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|&&k| *k == TokenKind::HoleClose)
            .count(),
        2
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|&&k| *k == TokenKind::TemplateOpen)
            .count(),
        2
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|&&k| *k == TokenKind::TemplateClose)
            .count(),
        2
    );
    let closes: Vec<_> = result
        .tokens
        .iter()
        .filter(|t| t.kind == TokenKind::HoleClose)
        .map(|t| t.span.start())
        .collect();
    assert_eq!(
        closes,
        [
            text.find("v}").unwrap() + 1,
            text.find("}} bye").unwrap() + 1
        ]
    );
    // Empty holes and mismatched parentheses remain parser-owned.
    for text in ["``", "`${}`", "`${(]}`", "`${x:stuff}`"] {
        assert_eq!(scan(text).status, LexStatus::Complete);
    }
}

#[test]
fn s13_s14_template_escape_order_and_shared_provenance() {
    let result = scan(r"`\${name} \\${name} \` \$`");
    assert_eq!(result.status, LexStatus::Invalid); // \$ is not a general dollar escape.
    let value = literal(&result.tokens[1]);
    assert_eq!(decoded(value), "${name} \\");
    assert_eq!(value.decoded[0].source, value.decoded[1].source);
    assert_eq!(value.decoded[0].source.bytes(), b"\\${");
    assert_eq!(result.tokens[2].kind, TokenKind::HoleOpen);
    assert_eq!(result.tokens[3].kind, TokenKind::Identifier);
    assert_eq!(result.tokens[4].kind, TokenKind::HoleClose);
    let result = scan(r"`\${name}` `\\${name}`");
    assert_eq!(result.status, LexStatus::Complete);
}

#[test]
fn s20_unfinished_modes_have_no_invented_closer() {
    for (text, expected) in [
        ("r#\"abc", DiagnosticKind::UnclosedLiteral),
        ("`abc", DiagnosticKind::UnclosedTemplate),
        ("`${abc", DiagnosticKind::UnclosedHole),
        ("`${r\"}", DiagnosticKind::UnclosedLiteral),
        ("`${/*}", DiagnosticKind::UnclosedComment),
        ("`${`nested", DiagnosticKind::UnclosedTemplate),
    ] {
        let result = scan(text);
        assert_eq!(result.status, LexStatus::Invalid);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| d.kind == expected && d.span.end() == text.len())
        );
        assert!(
            !result
                .tokens
                .iter()
                .any(|t| matches!(t.kind, TokenKind::TemplateClose | TokenKind::HoleClose))
        );
        assert_eq!(result.tokens.last().unwrap().kind, TokenKind::Eof);
        assert_eq!(
            extent(&result.tokens.last().unwrap().span),
            (text.len(), text.len())
        );
    }
}

#[test]
fn y1_y4_original_byte_scalar_and_encoded_coordinates() {
    let saved = snapshot("A\té🙂\r\nB");
    for (byte, human, utf8, utf16, utf32) in [
        (2, (1, 3), (0, 2), (0, 2), (0, 2)),
        (4, (1, 4), (0, 4), (0, 3), (0, 3)),
        (8, (1, 5), (0, 8), (0, 5), (0, 4)),
        (10, (2, 1), (1, 0), (1, 0), (1, 0)),
        (11, (2, 2), (1, 1), (1, 1), (1, 1)),
    ] {
        let coordinate = saved.coordinate(byte).unwrap();
        assert_eq!(coordinate.human(), Some(human));
        assert_eq!(coordinate.encoded_u32(Encoding::Utf8), Some(utf8));
        assert_eq!(coordinate.encoded_u32(Encoding::Utf16), Some(utf16));
        assert_eq!(coordinate.encoded_u32(Encoding::Utf32), Some(utf32));
    }
    for byte in [3, 5, 6, 7, 9, 12, usize::MAX] {
        assert_eq!(saved.coordinate(byte), None);
    }
    assert!(saved.span(2, 9).unwrap().coordinates().is_none());
    assert!(saved.span(3, 10).unwrap().coordinates().is_none());
    assert!(saved.span(2, 10).unwrap().coordinates().is_some());
    assert_eq!(
        saved.span(8, 10).unwrap().encoded_u32(Encoding::Utf16),
        Some(((0, 5), (1, 0)))
    );
    assert_eq!(saved.span(8, 9).unwrap().encoded_u32(Encoding::Utf8), None);
    assert_eq!(
        snapshot(b"A\rB").coordinate(2).unwrap().human(),
        Some((2, 1))
    );
    let invalid = snapshot(b"\xff\nB");
    assert!(invalid.coordinate(1).is_none());
    assert_eq!(
        invalid.coordinate(2).unwrap().encoded_u32(Encoding::Utf16),
        Some((1, 0))
    );
    assert_eq!(snapshot(b"").coordinate(0).unwrap().human(), Some((1, 1)));
    assert_eq!(
        snapshot(b"\r\n").coordinate(2).unwrap().human(),
        Some((2, 1))
    );
    assert_eq!(
        snapshot("e\u{301}").coordinate(3).unwrap().human(),
        Some((1, 3))
    );
    let too_large = Coordinate {
        line: usize::MAX,
        scalar: usize::MAX,
        utf8: usize::MAX,
        utf16: usize::MAX,
    };
    assert_eq!(too_large.human(), None);
    assert_eq!(too_large.encoded_u32(Encoding::Utf16), None);
}

fn assert_partition(result: &Lexed, length: usize) {
    if result.status == LexStatus::EncodingRejected {
        return;
    }
    let mut ranges: Vec<_> = result
        .tokens
        .iter()
        .filter(|t| t.kind != TokenKind::Eof)
        .map(|t| extent(&t.span))
        .chain(result.trivia.iter().map(|t| extent(&t.span)))
        .collect();
    ranges.sort_unstable();
    let mut next = 0;
    for (start, end) in ranges {
        assert_eq!(start, next);
        assert!(end > start && end <= length);
        next = end;
    }
    assert_eq!(next, length);
    assert_eq!(
        extent(&result.tokens.last().unwrap().span),
        (length, length)
    );
    for diagnostic in &result.diagnostics {
        assert!(diagnostic.span.end() <= length);
    }
}

#[test]
fn deterministic_progress_all_truncations_and_bounded_generated_bytes() {
    let inputs = [
        "\u{feff}let café = 1e-2f32; /* nest /* */ */",
        r###"r##"a"#b"## `text ${Point {x: "}"}} ${`inner ${x}`}`"###,
        r#""\u{10ffff}\x414\q" '\u{D800}'"#,
        "/* /* * / \r\n \0 */",
        "`${/* } */ '\\' } rest",
    ];
    for input in inputs {
        for end in 0..=input.len() {
            let saved = snapshot(&input.as_bytes()[..end]);
            let first = lex(&saved);
            assert_eq!(first, lex(&saved));
            assert_partition(&first, end);
        }
    }
    // Every single byte, every byte pair, and deterministic longer inputs. No
    // random seed, timeout-based success, parser, or external fuzz service.
    for a in 0..=255u8 {
        let saved = snapshot([a]);
        assert_partition(&lex(&saved), 1);
        for b in 0..=255u8 {
            let saved = snapshot([a, b]);
            let first = lex(&saved);
            assert_eq!(first, lex(&saved));
            assert_partition(&first, 2);
        }
    }
    let alphabet = b"r#\"'`$}{/*\\u012e+_-\r\n\t\x7f";
    let mut state = 7u32;
    for length in 0..128 {
        let bytes: Vec<_> = (0..length)
            .map(|_| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                alphabet[state as usize % alphabet.len()]
            })
            .collect();
        let saved = snapshot(bytes);
        let result = lex(&saved);
        assert_eq!(result, lex(&saved));
        assert_partition(&result, length);
    }
}

#[test]
fn deep_modes_hashes_and_comments_do_not_use_host_recursion() {
    let depth = 10_000;
    for text in [
        format!("{}{}", "`${".repeat(depth), "}`".repeat(depth)),
        format!("{}{}", "/*".repeat(depth), "*/".repeat(depth)),
        format!("r{}\"x\"{}", "#".repeat(depth), "#".repeat(depth)),
        format!("`${{{}{}}}`", "{".repeat(depth), "}".repeat(depth)),
    ] {
        let result = scan(&text);
        assert_eq!(result.status, LexStatus::Complete);
        assert_partition(&result, text.len());
    }
}
