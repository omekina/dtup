use crate::{
    ParseErrorReport, StreamResult, StreamedError,
    lexer::{
        ErrorReports, ExtendedIdent, StreamItem, TokenizerStreamItem, auto_parser, def_yeet, err,
        opt_consume_any_ident, skip_tokens_no_push,
    },
    stream_utils::StreamPrepend,
    tokenizer::{GroupType, Span, Token},
};

pub(super) type NodeName = (String, Span);
pub(super) type NodeAddress = Option<(String, Span)>;

pub(super) fn deref_ident_to_node_name(
    name: (ExtendedIdent, &Span),
    require_letter_start: bool,
) -> (String, Vec<ParseErrorReport>) {
    let mut errors = Vec::new();
    let node_name = match name.0.req_node_name() {
        Ok(v) => v,
        Err((e, v)) => {
            let symbol = e.first().unwrap();
            let ptr = name.1.ptr.clone().offset(symbol);
            errors.push(err!(raw [{ InvalidNodeName, [
                ("invalid symbol for a node id", 1, ptr)
            ]}]));
            v
        }
    };
    if require_letter_start {
        match node_name.chars().next().unwrap() {
            'a'..='z' | 'A'..='Z' => {}
            _ => errors.push(err!(raw [{ InvalidNodeName, [
                ("node names must start with a letter", 1, name.1.ptr.clone()),
            ]}])),
        }
    }
    (node_name, errors)
}

pub(super) type NodeIdConsumeResult = Result<(NodeName, NodeAddress, ErrorReports), Option<Span>>;

pub(super) fn consume_node_id<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> StreamItem<NodeIdConsumeResult> {
    def_yeet!();
    let (name, mut errors) = match yeet_value!(consume_node_name(source)) {
        Ok(v) => v,
        Err(e) => return StreamResult::Ok(Err(e)),
    };
    let (address, e) = yeet_value!(consume_maybe_node_address(source));
    errors.extend(e);
    StreamResult::Ok(Ok((name, address, errors)))
}

pub(super) fn consume_node_name<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> StreamItem<Result<(NodeName, ErrorReports), Option<Span>>> {
    def_yeet!();
    let (ident, span) = match yeet_value!(opt_consume_any_ident(source)) {
        Ok(v) => v,
        Err(v) => return StreamResult::Ok(Err(v)),
    };
    let (ident, errors) = deref_ident_to_node_name((ident, &span), true);
    StreamResult::Ok(Ok(((ident, span), errors)))
}

fn consume_node_path_part<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    path: &mut Vec<((String, Span), Option<(String, Span)>)>,
    errors: &mut ErrorReports,
) -> StreamItem<bool> {
    def_yeet!();
    match yeet_value!(consume_node_id(source)) {
        Ok((id, addr, e)) => {
            errors.extend(e);
            path.push((id, addr));
            StreamResult::Ok(true)
        }
        Err(Some(e)) => {
            errors.push(err!(raw [{ UnexpectedToken, [
                ("this not seem like a valid node path part", e),
            ]}]));
            StreamResult::Ok(false)
        }
        Err(None) => unreachable!(),
    }
}

auto_parser!(skip_tokens_no_push skip_to_node_path_end, |t| {
    !matches!(t, &Token::GroupClosing(GroupType::Brace))
});

pub(super) fn consume_node_path<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    opening_brace: &Span,
) -> StreamItem<(
    Option<Vec<((String, Span), Option<(String, Span)>)>>,
    ErrorReports,
)> {
    def_yeet!();
    let mut path = Vec::new();
    let mut part_before = true;
    let mut errors = Vec::new();
    loop {
        match source.next() {
            Some(v) => {
                let v = yeet_value!(v.map_err(|e| StreamedError::ShouldEnd(e)));
                match v.token {
                    Token::GroupClosing(GroupType::Brace) => {
                        return StreamResult::Ok((Some(path), errors));
                    }
                    Token::Comment(_) => {
                        errors.push(err!(raw [{
                            UnexpectedWhitespace, [("node paths can't contain comments", v.span)]
                        }]));
                        skip_to_node_path_end(source);
                        return StreamResult::Ok((None, errors));
                    }
                    Token::Whitespace(_) => {
                        errors.push(err!(raw [{
                            UnexpectedWhitespace, [("node paths can't contain whitespace", v.span)]
                        }]));
                        skip_to_node_path_end(source);
                        return StreamResult::Ok((None, errors));
                    }
                    Token::Slash => {
                        if !part_before {
                            errors.push(err!(raw [{ InvalidNodePath, [
                                ("before this, there is an empty segment", v.span),
                            ]}]));
                        }
                        part_before = false;
                    }
                    _ => {
                        source.push(StreamResult::Ok(v));
                        if !yeet_value!(consume_node_path_part(source, &mut path, &mut errors)) {
                            skip_to_node_path_end(source);
                            return StreamResult::Ok((None, errors));
                        } else {
                            part_before = true;
                        }
                    }
                }
            }
            None => {
                errors.push(err!(raw [{ UnexpectedEof, [
                    ("this is unclosed until eof", opening_brace.clone()),
                ]}]));
                return StreamResult::Ok((None, errors));
            }
        }
    }
}

pub(super) fn consume_maybe_node_address<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> StreamItem<(Option<(String, Span)>, ErrorReports)> {
    def_yeet!();
    let mut errors = Vec::new();
    let at = loop {
        match source.next() {
            Some(v) => {
                let v = yeet_value!(v.map_err(|e| StreamedError::ShouldEnd(e)));
                match v.token {
                    Token::At => break v.span,
                    Token::Whitespace(_) | Token::Comment(_) => {
                        errors.push(err!(raw [{ UnexpectedWhitespace, [
                            ("a `@` must follow right after a node name (no whitespaces)", v.span),
                        ]}]))
                    }
                    _ => {
                        source.push(StreamResult::Ok(v));
                        return StreamResult::Ok((None, Vec::default()));
                    }
                }
            }
            None => return StreamResult::Ok((None, Vec::default())),
        }
    };
    loop {
        match source.next() {
            Some(v) => {
                let v = yeet_value!(v.map_err(|e| StreamedError::ShouldEnd(e)));
                match v.token {
                    Token::Whitespace(_) | Token::Comment(_) => {
                        errors.push(err!(raw [{ UnexpectedWhitespace, [
                            ("node address must follow right after `@` (no whitespace)", v.span),
                        ]}]))
                    }
                    _ => {
                        source.push(StreamResult::Ok(v));
                        break;
                    }
                }
            }
            None => {
                errors.push(err!(raw [{ UnexpectedEof, [
                    ("expected a node address after this, but found eof", at),
                ]}]));
                return StreamResult::Ok((None, errors));
            }
        }
    }
    let (address, errors) = match yeet_value!(consume_node_address(source)) {
        Ok(v) => v,
        Err(Some(e)) => {
            errors.push(err!(raw [{ UnexpectedToken, [
                ("this implies a successive node address", at),
                ("this, however, is not a valid node address", e),
            ]}]));
            return StreamResult::Ok((None, errors));
        }
        Err(None) => unreachable!(),
    };
    StreamResult::Ok((Some(address), errors))
}

pub(super) fn consume_node_address<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
) -> StreamItem<Result<((String, Span), ErrorReports), Option<Span>>> {
    def_yeet!();
    let (ident, span) = match yeet_value!(opt_consume_any_ident(source)) {
        Ok(v) => v,
        Err(v) => return StreamResult::Ok(Err(v)),
    };
    let (ident, errors) = deref_ident_to_node_name((ident, &span), false);
    StreamResult::Ok(Ok(((ident, span), errors)))
}
