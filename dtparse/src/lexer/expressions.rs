use crate::{
    StreamResult, StreamedError,
    lexer::{
        ArithmeticOperation, BitwiseOperation, Expression, LexerItem, LogicalOperation,
        MultiErrorItem, RelationalOperation, TokenizerStreamItem, err,
    },
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
        ReportInlineMessage,
    },
    result::{Errors, ParseErrorReport},
    stream_utils::StreamPrepend,
    tokenizer::Span,
};

enum BinaryOperation {
    ArithmeticOperator(ArithmeticOperation),
    RelationalOperator(RelationalOperation),
    LogicalOperator(LogicalOperation),
    BitwiseOperator(BitwiseOperation),
}

trait BinaryOperationPriority {
    fn priority(&self) -> u8;
}

impl BinaryOperationPriority for ArithmeticOperation {
    fn priority(&self) -> u8 {
        match self {
            Self::Multiplication | Self::Division | Self::Modulo => 0,
            Self::Addition | Self::Subtraction => 1,
        }
    }
}

impl BinaryOperationPriority for BitwiseOperation {
    fn priority(&self) -> u8 {
        match self {
            Self::RightShift | Self::LeftShift => 2,
            Self::And => 3,
            Self::Xor => 4,
            Self::Or => 5,
        }
    }
}

impl BinaryOperationPriority for RelationalOperation {
    fn priority(&self) -> u8 {
        match self {
            _ => 6,
        }
    }
}

impl BinaryOperationPriority for LogicalOperation {
    fn priority(&self) -> u8 {
        match self {
            Self::And => 7,
            Self::Or => 8,
        }
    }
}

struct BinaryOperationParsingItem {
    operation: BinaryOperation,
    operator_span: Span,
    left: Expression,
}

#[derive(Default)]
struct BinaryOperationParsingStack {
    stack: Vec<BinaryOperationParsingItem>,
}

impl BinaryOperationParsingStack {
    fn push(
        &mut self,
        operation: BinaryOperation,
        span: Span,
        middle: Expression,
    ) {
    }
}

fn consume_expression<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    start: Span,
) -> MultiErrorItem<LexerItem> {
    let mut parens = vec![start];
    macro_rules! unclosed_parens {
        () => {
            parens.iter().map(|v| {
                err!(raw [{ UnmatchedDelimiter, [
                    ("this parenthesis remains unclosed", v.clone())
                ]}])
            }).collect::<Vec<_>>()
        };
    }
    macro_rules! req_next {
        () => {
            loop {
                match source.next() {
                    Some(StreamResult::Ok(v)) => break v,
                    Some(StreamResult::IoError(e)) => return StreamResult::IoError(e),
                    Some(StreamResult::ProcessingError(StreamedError::CanContinue(e))) => {
                        errors.push(e);
                    }
                    Some(StreamResult::ProcessingError(StreamedError::ShouldEnd(e))) => {
                        return StreamResult::ProcessingError(StreamedError::ShouldEnd(vec![e]));
                    }
                    None => return StreamResult::ProcessingError(
                        StreamedError::ShouldEnd(unclosed_parens!())
                    );
                }
            }
        };
    }

    let paren_depth = 1;
    let stack = BinaryOperationParsingStack::default();
    loop {
        break;
    }

    if paren_depth > 0 {
        return StreamResult::ProcessingError(StreamedError::ShouldEnd(unclosed_parens!()));
    }
    todo!();
}
