use crate::errors::Errors;
use crate::lexer::skip_possible;
use crate::report::{
    PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
    ReportInlineMessage,
};
use crate::result::{ParseErrorReport, StreamResult, StreamedError};
use crate::tokenizer::{GenericLiteralType, SpanToken};
use crate::{
    lexer::{
        ErrorReports, LexerToken, StreamItem, TokenizerStreamItem, auto_parser, def_try_yield, err,
        opt_consume_any_ident, skip_opt,
    },
    stream_utils::StreamPrepend,
    tokenizer::{GenericLiteral, GroupType, LiteralToken, Span, Token},
};

enum CompilerDirectiveType {
    DtsHeader,
    Include,
    OmitIfNoRef,
    Bits,
}

impl std::str::FromStr for CompilerDirectiveType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dts-v1" => Ok(CompilerDirectiveType::DtsHeader),
            "include" => Ok(CompilerDirectiveType::Include),
            "omit-if-no-ref" => Ok(CompilerDirectiveType::OmitIfNoRef),
            "bits" => Ok(CompilerDirectiveType::Bits),
            _ => Err(()),
        }
    }
}

#[derive(Debug)]
pub enum CompilerDirective {
    DtsHeader(Span),
    Include {
        include: Span,
        target: (String, Span),
    },
    Bits {
        bits: Span,
        size: (String, Span),
    },
    OmitIfNoRef(Span),
}

pub fn consume_compiler_directive_or_root_node<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    slash: Span,
) -> StreamItem<(Option<LexerToken>, ErrorReports)> {
    def_try_yield!(single_err source);
    let mut errors = Vec::new();
    loop {
        let ident = try_yield!(slash);
        match ident.token {
            Token::Literal(LiteralToken::Ident(GenericLiteral { content, .. })) => {
                return consume_compiler_directive_arguments(source, slash, (content, ident.span))
                    .map(|(token, mut res_errors)| {
                        res_errors.extend(errors.into_iter());
                        (token.map(|v| LexerToken::CompilerDirective(v)), res_errors)
                    });
            }
            Token::GroupOpening(GroupType::Brace) => {
                return StreamResult::Ok((
                    Some(LexerToken::RootNodeStart {
                        slash,
                        opening_delimiter: ident.span,
                    }),
                    Vec::default(),
                ));
            }
            Token::Whitespace(_) => {
                errors.push(err!(raw [{ UnexpectedWhitespace, [
                    ("this implies a following compiler directive", slash.clone()),
                    ("compiler directives can't contain whitespaces", ident.span),
                ]}]));
            }
            Token::Comment(_) => {
                errors.push(err!(raw [{ UnexpectedWhitespace, [
                    ("this implies a following compiler directive", slash.clone()),
                    ("compiler directives can't contain comments", ident.span),
                ]}]));
            }
            _ => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("this implies a following compiler directive", slash.clone()),
                    (
                        "this is unexpected, expected an ident or a node opening (`{`) here",
                        ident.span
                    ),
                ]}]));
                return StreamResult::Ok((None, errors));
            }
        }
    }
}

pub fn consume_compiler_directive<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    slash: Span,
) -> StreamItem<(Option<CompilerDirective>, ErrorReports)> {
    def_try_yield!(single_err source);
    let mut errors = Vec::new();
    loop {
        let ident = try_yield!(slash);
        match ident.token {
            Token::Literal(LiteralToken::Ident(GenericLiteral { content, .. })) => {
                return consume_compiler_directive_arguments(source, slash, (content, ident.span))
                    .map(|(token, mut res_errors)| {
                        res_errors.extend(errors.into_iter());
                        (token, res_errors)
                    });
            }
            Token::Whitespace(_) => {
                errors.push(err!(raw [{ UnexpectedWhitespace, [
                    ("this implies a following compiler directive", slash.clone()),
                    ("compiler directives can't contain whitespaces", ident.span),
                ]}]));
            }
            Token::Comment(_) => {
                errors.push(err!(raw [{ UnexpectedWhitespace, [
                    ("this implies a following compiler directive", slash.clone()),
                    ("compiler directives can't contain comments", ident.span),
                ]}]));
            }
            _ => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("this implies a following compiler directive", slash.clone()),
                    (
                        "this is unexpected",
                        ident.span
                    ),
                ]}]));
                return StreamResult::Ok((None, errors));
            }
        }
    }
}

pub fn consume_compiler_directive_arguments<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    slash: Span,
    ident: (String, Span),
) -> StreamItem<(Option<CompilerDirective>, ErrorReports)> {
    def_try_yield!(
        custom_single_err source, "expected `/` before eof to finish this compiler directive"
    );
    let mut errors = Vec::new();
    let mut ident_inner = ident.0;
    let ident_ptr = ident.1.ptr;
    let mut ident_span = ident.1.span;
    match opt_consume_any_ident(source) {
        Some(v) => {
            ident_span += ident_span;
            ident_inner.push_str(&v.0.req_attribute_name());
        }
        None => {}
    };
    let end = loop {
        let end = try_yield!(slash);
        match end.token {
            Token::Slash => break end,
            Token::Whitespace(_) => {
                errors.push(err!(raw [{ UnexpectedWhitespace, [
                    ("compiler directive started here", slash.clone()),
                    ("compiler directives can't contain whitespaces", end.span.clone()),
                ]}]));
            }
            Token::Comment(_) => {
                errors.push(err!(raw [{ UnexpectedWhitespace, [
                    ("compiler directive started here", slash.clone()),
                    ("compiler directives can't contain comments", end.span.clone()),
                ]}]));
            }
            _ => {
                errors.push(err!(raw [{ UnexpectedToken, [
                    ("compiler directive started here", slash.clone()),
                    (
                        "this is unexpected in a compiler directive, expected a slash here",
                        end.span.clone()
                    ),
                ]}]));
            }
        }
    };
    let ident_span = Span {
        span: ident_span,
        ptr: ident_ptr,
    };
    let Ok(directive_type): Result<CompilerDirectiveType, _> = ident_inner.parse() else {
        errors.push(err!(raw [{ UnknownCompilerDirective, [
            ("this is an unknown compiler directive", ident_span),
        ]}]));
        return StreamResult::Ok((None, errors));
    };
    def_try_yield!(errs_only single source);
    match directive_type {
        CompilerDirectiveType::Include => {
            skip_possible(source);
            match try_yield_errs!() {
                Some(SpanToken {
                    token: Token::Literal(LiteralToken::String(v)),
                    span,
                }) => StreamResult::Ok((
                    Some(CompilerDirective::Include {
                        include: ident_span,
                        target: (v, span),
                    }),
                    errors,
                )),
                Some(v) => {
                    errors.push(err!(raw [{ UnexpectedToken, [
                        ("includes require a string argument", ident_span),
                        ("hence, expected a string literal here", v.span),
                    ]}]));
                    StreamResult::Ok((None, errors))
                }
                None => {
                    errors.push(err!(raw [{ UnexpectedToken, [
                        ("includes require a string argument", ident_span),
                        ("hence, expected a string literal after this", end.span),
                    ]}]));
                    StreamResult::Ok((None, errors))
                }
            }
        }
        CompilerDirectiveType::DtsHeader => {
            skip_possible(source);
            match try_yield_errs!() {
                Some(SpanToken {
                    span,
                    token: Token::Semicolon,
                }) => StreamResult::Ok((Some(CompilerDirective::DtsHeader(span)), errors)),
                Some(SpanToken { span, token }) => {
                    source.push(StreamResult::Ok(SpanToken {
                        span: span.clone(),
                        token,
                    }));
                    errors.push(err!(raw [{ UnexpectedToken, [
                        ("dts-v1 directive requires a trailing semicolon", ident_span),
                        ("expected a semicolon here", span),
                    ]}]));
                    StreamResult::Ok((None, errors))
                }
                None => {
                    errors.push(err!(raw [{ UnexpectedEof, [
                        ("dts-v1 directive requires a trailing semicolon", ident_span),
                        ("after this", end.span),
                    ]}]));
                    StreamResult::Ok((None, errors))
                }
            }
        }
        CompilerDirectiveType::OmitIfNoRef => {
            StreamResult::Ok((Some(CompilerDirective::OmitIfNoRef(ident_span)), errors))
        }
        CompilerDirectiveType::Bits => {
            skip_possible(source);
            match try_yield_errs!() {
                Some(SpanToken {
                    span,
                    token:
                        Token::Literal(LiteralToken::Ident(GenericLiteral {
                            content,
                            of_type: GenericLiteralType::DecimalNumeric,
                        })),
                }) => StreamResult::Ok((
                    Some(CompilerDirective::Bits {
                        bits: ident_span,
                        size: (content, span),
                    }),
                    errors,
                )),
                Some(SpanToken {
                    span,
                    token:
                        Token::Literal(LiteralToken::Ident(GenericLiteral {
                            of_type: GenericLiteralType::HexadecimalNumeric { .. },
                            ..
                        })),
                }) => {
                    errors.push(err!(raw [{ UnexpectedToken, [
                        ("bits directives require a decimal numeric literal argument", ident_span),
                        ("this is not a decimal numeric", span),
                    ]}]));
                    StreamResult::Ok((None, errors))
                }
                Some(v) => {
                    errors.push(err!(raw [{ UnexpectedToken, [
                        ("bits directives require a decimal numeric literal argument", ident_span),
                        ("this is unexpected", v.span.clone()),
                    ]}]));
                    source.push(StreamResult::Ok(v));
                    StreamResult::Ok((None, errors))
                }
                None => {
                    errors.push(err!(raw [{ UnexpectedToken, [
                        ("bits directives require a decimal numeric literal argument", ident_span),
                        ("hence, expected a decimal numeric literal after this", end.span),
                    ]}]));
                    StreamResult::Ok((None, errors))
                }
            }
        }
    }
}
