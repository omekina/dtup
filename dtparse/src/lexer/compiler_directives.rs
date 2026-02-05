use crate::lexer::expressions::consume_label_reference;
use crate::lexer::node::{NodeAddress, NodeName, consume_node_id};
use crate::lexer::{
    NumericLiteral, Reference, deref_attribute_name, skip_possible, skip_to_statement_end,
    try_token_after,
};
use crate::result::{StreamResult, StreamedError};
use crate::tokenizer::{GenericLiteralType, SpanToken};
use crate::{
    lexer::{
        ErrorReports, LexerToken, StreamItem, TokenizerStreamItem, def_yeet, err,
        opt_consume_any_ident,
    },
    stream_utils::StreamPrepend,
    tokenizer::{GenericLiteral, GroupType, LiteralToken, Span, Token},
};

enum CompilerDirectiveType {
    DtsHeader,
    Include,
    OmitIfNoRef,
    Bits,
    DeleteNode,
    DeleteProperty,
    Memreserve,
}

impl std::str::FromStr for CompilerDirectiveType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dts-v1" => Ok(CompilerDirectiveType::DtsHeader),
            "include" => Ok(CompilerDirectiveType::Include),
            "omit-if-no-ref" => Ok(CompilerDirectiveType::OmitIfNoRef),
            "bits" => Ok(CompilerDirectiveType::Bits),
            "delete-node" => Ok(CompilerDirectiveType::DeleteNode),
            "delete-property" => Ok(CompilerDirectiveType::DeleteProperty),
            "memreserve" => Ok(CompilerDirectiveType::Memreserve),
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
    DeleteNode {
        delete_node: Span,
        target: NodeTarget,
    },
    DeleteProperty {
        delete_property: Span,
        target: (String, Span),
    },
    Memreserve {
        memreserve: Span,
        address: NumericLiteral,
        length: NumericLiteral,
    },
}

#[derive(Debug)]
pub enum NodeTarget {
    Node((NodeName, NodeAddress)),
    Reference(Reference),
}

pub fn consume_compiler_directive_or_root_node<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    slash: Span,
) -> StreamItem<(Option<LexerToken>, ErrorReports)> {
    def_yeet!(require next from source => single with message default);
    let mut errors = Vec::new();
    loop {
        let ident = req_next!(slash);
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
    def_yeet!(require next from source => single with message default);
    let mut errors = Vec::new();
    loop {
        let ident = req_next!(slash);
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
    def_yeet!(require next from source => single with message
        "expected `/` before eof to finish this compiler directive"
    );
    def_yeet!();
    let mut errors = Vec::new();
    let mut ident_inner = ident.0;
    let ident_ptr = ident.1.ptr;
    let mut ident_span = ident.1.span;
    match yeet_value!(opt_consume_any_ident(source)) {
        Ok(v) => {
            ident_span += v.1.span;
            ident_inner.push_str(&v.0.req_attribute_name());
        }
        Err(_) => {}
    };
    let end = loop {
        let end = req_next!(slash);
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
    def_yeet!(optionally get next from source => single);
    match directive_type {
        CompilerDirectiveType::Include => {
            skip_possible(source);
            match try_next!() {
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
            match try_next!() {
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
            match try_next!() {
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
        CompilerDirectiveType::DeleteNode => {
            compiler_directive_delete_node(source, errors, ident_span, end)
        }
        CompilerDirectiveType::DeleteProperty => {
            skip_possible(source);
            let attribute = match yeet_value!(opt_consume_any_ident(source)) {
                Ok(v) => v,
                Err(Some(e)) => {
                    errors.push(err!(raw [{ UnexpectedToken, [
                        ("this directive requires an attribute name", ident_span),
                        ("expected a valid attribute name here", e),
                    ]}]));
                    return StreamResult::Ok((None, errors));
                }
                Err(None) => {
                    errors.push(err!(raw [{ UnexpectedToken, [(
                        "this directive requires an attribute name as an argument, but got eof",
                        ident_span
                    )]}]));
                    return StreamResult::Ok((None, errors));
                }
            };
            let (attribute_ident, e, _) = deref_attribute_name((attribute.0, &attribute.1), true);
            if let Some(e) = e {
                errors.extend(e);
            }
            skip_possible(source);
            let (_, error) = yeet_value!(try_token_after(
                source,
                |_| false,
                |t| matches!(t, Token::Semicolon),
                "expected a semicolon after the node id argument for this directive, got eof",
                &ident_span,
                "expected a semicolon after the node id argument for this directive",
                "this was unexpected, perhaps put a semicolon before?",
            ));
            if let Some(error) = error {
                errors.push(error);
            }
            StreamResult::Ok((
                Some(CompilerDirective::DeleteProperty {
                    delete_property: ident_span,
                    target: (attribute_ident, attribute.1),
                }),
                errors,
            ))
        }
        CompilerDirectiveType::Memreserve => {
            macro_rules! req {
                (@nonfatal_error $type: literal, $span: expr, $res_type: ident, $v: expr) => {{
                    errors.push(err!(raw [{ InvalidNumericLiteral, [(
                        concat!($type, " must contain a prefix").to_string(), $span.clone()
                    )]}]));
                    NumericLiteral::$res_type(($v, $span))
                }};

                ($type: literal, $v: expr) => {
                    match $v.token {
                        Token::Literal(LiteralToken::Ident(GenericLiteral {
                            content,
                            of_type: GenericLiteralType::HexadecimalNumeric { prefix: false },
                        })) => req!(@nonfatal_error $type, $v.span, Hexadecimal, content),
                        Token::Literal(LiteralToken::Ident(GenericLiteral {
                            content,
                            of_type: GenericLiteralType::HexadecimalNumeric { prefix: true },
                        })) => NumericLiteral::Hexadecimal((content, $v.span)),
                        Token::Literal(LiteralToken::Ident(GenericLiteral {
                            content,
                            of_type: GenericLiteralType::DecimalNumeric,
                        })) => NumericLiteral::Decimal((content, $v.span)),
                        _ => {
                            errors.push(err!(raw [{ UnexpectedToken, [(
                                concat!("expected ", $type, " here").to_string(), $v.span
                            )]}]));
                            skip_to_statement_end(source);
                            return StreamResult::Ok((None, errors));
                        }
                    }
                };
            }
            skip_possible(source);
            let address = match try_next!() {
                Some(v) => req!("address hexadecimal literal", v),
                None => {
                    errors.push(err!(raw [{ UnexpectedEof, [(
                        "a numeric literal (an address) was expected as an argument to this",
                        ident_span
                    )]}]));
                    return StreamResult::Ok((None, errors));
                }
            };
            skip_possible(source);
            let length = match try_next!() {
                Some(v) => req!("length hexadecimal literal", v),
                None => {
                    errors.push(err!(raw [{ UnexpectedEof, [(
                        "a numeric literal (a length) was expected as an argument to this",
                        ident_span
                    )]}]));
                    return StreamResult::Ok((None, errors));
                }
            };
            let (_, e) = yeet_value!(try_token_after(
                source,
                |t| matches!(t, &(Token::Whitespace(_) | Token::Comment(_))),
                |t| matches!(t, &Token::Semicolon),
                "expected a semicolon before eof to end this directive",
                &ident_span,
                "after this directive's arguments",
                "unexpected token here, expected a semicolon",
            ));
            if let Some(e) = e {
                errors.push(e);
            }
            StreamResult::Ok((
                Some(CompilerDirective::Memreserve {
                    memreserve: ident_span,
                    address,
                    length,
                }),
                errors,
            ))
        }
    }
}

pub fn compiler_directive_delete_node<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    mut errors: ErrorReports,
    ident_span: Span,
    end: SpanToken,
) -> StreamItem<(Option<CompilerDirective>, ErrorReports)> {
    def_yeet!();
    skip_possible(source);
    let Some(token) = source.next() else {
        errors.push(err!(raw [{ UnexpectedEof, [
            (
                "expected a node name or a label as an argument for this directive, but got eof",
                ident_span
            ),
            ("after this", end.span),
        ]}]));
        return StreamResult::Ok((None, errors));
    };
    let token = yeet_value!(token.map_err(|e| StreamedError::ShouldEnd(e)));
    let target = match token.token {
        Token::Ampersand => {
            NodeTarget::Reference(match consume_label_reference(source, token.span) {
                StreamResult::Ok(v) => v,
                StreamResult::IoError(e) => return StreamResult::IoError(e),
                StreamResult::ProcessingError(StreamedError::ShouldEnd(mut e)) => {
                    let last = e.pop().unwrap();
                    return StreamResult::ProcessingError(StreamedError::ShouldEnd(last));
                }
                StreamResult::ProcessingError(StreamedError::CanContinue(e)) => {
                    errors.extend(e);
                    return StreamResult::Ok((None, errors));
                }
            })
        }
        _ => {
            source.push(StreamResult::Ok(token));
            let (name, address, e) = match yeet_value!(consume_node_id(source)) {
                Ok(v) => v,
                Err(Some(e)) => {
                    errors.push(err!(raw [{ UnexpectedToken, [
                        (
                            "expected a node name (or a `&`) as an argument for this directive",
                            ident_span
                        ),
                        ("this is not a valid node name", e),
                    ]}]));
                    return StreamResult::Ok((None, errors));
                }
                Err(None) => {
                    errors.push(err!(raw [{ UnexpectedEof, [
                        ("expected a node name as an argument for this directive", ident_span),
                        ("after this, but encountered eof", end.span),
                    ]}]));
                    return StreamResult::Ok((None, errors));
                }
            };
            errors.extend(e);
            NodeTarget::Node((name, address))
        }
    };
    skip_possible(source);
    let (_, error) = yeet_value!(try_token_after(
        source,
        |_| false,
        |t| matches!(t, Token::Semicolon),
        "expected a semicolon after the node id argument for this directive, got eof",
        &ident_span,
        "expected a semicolon after the node id argument for this directive",
        "this was unexpected, perhaps put a semicolon before?",
    ));
    if let Some(error) = error {
        errors.push(error);
    }
    StreamResult::Ok((
        Some(CompilerDirective::DeleteNode {
            delete_node: ident_span,
            target,
        }),
        errors,
    ))
}
