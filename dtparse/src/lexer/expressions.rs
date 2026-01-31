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

trait MergeBinaryOperation {
    fn merge(self, left: Expression, right: Expression) -> Expression;
}

trait BinaryOperationPriority {
    fn priority(&self) -> u8;
}

macro_rules! autodef {
    (merge_binop $for: ident [$($variant: ident),*$(,)?]) => {
        impl MergeBinaryOperation for $for {
            fn merge(self, left: Expression, right: Expression) -> Expression {
                let (left, right) = (Box::new(left), Box::new(right));
                match self {
                    $(Self::$variant => Expression::$for {
                        left, right, operator: $for::$variant
                    }),*
                }
            }
        }
    };

    (prio_binop $for: ident [
        $($($variant: ident)|+ => $priority: literal),*$(,)?$(_ => $def_prio: literal)?
    ]) => {
        impl BinaryOperationPriority for $for {
            fn priority(&self) -> u8 {
                match self {
                    $($(Self::$variant)|+ => $priority,)*
                    $(_ => $def_prio)*
                }
            }
        }
    };
}

autodef!(merge_binop ArithmeticOperation [
    Addition, Subtraction, Multiplication, Division, Modulo
]);
autodef!(merge_binop RelationalOperation [
    Equal, NotEqual, LessThan, GreaterThan, LessOrEqual, GreaterOrEqual,
]);
autodef!(merge_binop LogicalOperation [
    Or, And,
]);
autodef!(merge_binop BitwiseOperation [
    And, Or, Xor, LeftShift, RightShift,
]);

impl MergeBinaryOperation for BinaryOperation {
    fn merge(self, left: Expression, right: Expression) -> Expression {
        macro_rules! autodef {
            ($($v: ident),*) => { match self { $(Self::$v(v) => v.merge(left, right)),* } };
        }
        autodef!(
            ArithmeticOperator,
            RelationalOperator,
            LogicalOperator,
            BitwiseOperator
        )
    }
}

autodef!(prio_binop ArithmeticOperation [
    Multiplication | Division | Modulo => 0,
    Addition | Subtraction => 1
]);
autodef!(prio_binop BitwiseOperation [
    RightShift | LeftShift => 2,
    And => 3,
    Xor => 4,
    Or => 5
]);
autodef!(prio_binop RelationalOperation [
    _ => 6
]);
autodef!(prio_binop LogicalOperation [
    And => 7,
    Or => 8
]);

impl BinaryOperationPriority for BinaryOperation {
    fn priority(&self) -> u8 {
        macro_rules! autodef {
            ($($v: ident),*) => { match self { $(Self::$v(v) => v.priority()),* } };
        }
        autodef!(
            LogicalOperator,
            BitwiseOperator,
            ArithmeticOperator,
            RelationalOperator
        )
    }
}

struct BinaryOperationParsingItem {
    operation: BinaryOperation,
    operator_span: Span,
    left: Expression,
}

impl BinaryOperationParsingItem {
    fn new(operator: (BinaryOperation, Span), left: Expression) -> Self {
        Self {
            operation: operator.0,
            operator_span: operator.1,
            left,
        }
    }
}

#[derive(Default)]
struct BinaryOperationParsingStack {
    stack: Vec<BinaryOperationParsingItem>,
}

impl BinaryOperationParsingStack {
    fn push(&mut self, operation: BinaryOperation, span: Span, middle: Expression) {
        match self.stack.last() {
            Some(v) => {
                let left_priority = v.operation.priority();
                let right_priority = operation.priority();
                if left_priority > right_priority {
                    let right = BinaryOperationParsingItem::new((operation, span), middle);
                    self.stack.push(right);
                } else {
                    let left = self.merge(middle);
                    self.stack
                        .push(BinaryOperationParsingItem::new((operation, span), left));
                }
            }
            None => {
                self.stack
                    .push(BinaryOperationParsingItem::new((operation, span), middle));
            }
        }
    }

    fn merge(&mut self, right: Expression) -> Expression {
        let mut tmp = right;
        while let Some(v) = self.stack.pop() {
            tmp = v.operation.merge(v.left, tmp);
        }
        tmp
    }
}

#[cfg(test)]
mod binary_operation_parsing {
    use super::*;
    use crate::lexer::NumericLiteral;

    macro_rules! token {
        (num_lit $num: literal) => {
            Expression::NumericLiteral((NumericLiteral::Decimal($num.to_string()), Span::default()))
        };
    }

    #[test]
    fn left_to_right() {
        let tokens = [
            (
                token!(num_lit 10),
                BinaryOperation::ArithmeticOperator(ArithmeticOperation::Addition),
            ),
            (
                token!(num_lit 2),
                BinaryOperation::ArithmeticOperator(ArithmeticOperation::Addition),
            ),
        ];

        let mut stack = BinaryOperationParsingStack::default();
        for token in tokens {
            stack.push(token.1, Span::default(), token.0);
        }

        assert_eq!(
            stack.merge(token!(num_lit 3)),
            Expression::ArithmeticOperation {
                left: Box::new(Expression::ArithmeticOperation {
                    left: Box::new(token!(num_lit 10)),
                    right: Box::new(token!(num_lit 2)),
                    operator: ArithmeticOperation::Addition,
                }),
                right: Box::new(token!(num_lit 3)),
                operator: ArithmeticOperation::Addition,
            }
        );
    }

    #[test]
    fn order_override() {
        let tokens = [
            (
                token!(num_lit 10),
                BinaryOperation::ArithmeticOperator(ArithmeticOperation::Addition),
            ),
            (
                token!(num_lit 2),
                BinaryOperation::ArithmeticOperator(ArithmeticOperation::Multiplication),
            ),
        ];

        let mut stack = BinaryOperationParsingStack::default();
        for token in tokens {
            stack.push(token.1, Span::default(), token.0);
        }

        assert_eq!(
            stack.merge(token!(num_lit 3)),
            Expression::ArithmeticOperation {
                left: Box::new(token!(num_lit 10)),
                right: Box::new(Expression::ArithmeticOperation {
                    left: Box::new(token!(num_lit 2)),
                    right: Box::new(token!(num_lit 3)),
                    operator: ArithmeticOperation::Multiplication,
                }),
                operator: ArithmeticOperation::Addition,
            }
        );
    }

    #[test]
    fn order_follow() {
        let tokens = [
            (
                token!(num_lit 10),
                BinaryOperation::ArithmeticOperator(ArithmeticOperation::Multiplication),
            ),
            (
                token!(num_lit 2),
                BinaryOperation::ArithmeticOperator(ArithmeticOperation::Addition),
            ),
        ];

        let mut stack = BinaryOperationParsingStack::default();
        for token in tokens {
            stack.push(token.1, Span::default(), token.0);
        }

        assert_eq!(
            stack.merge(token!(num_lit 3)),
            Expression::ArithmeticOperation {
                left: Box::new(Expression::ArithmeticOperation {
                    left: Box::new(token!(num_lit 10)),
                    right: Box::new(token!(num_lit 2)),
                    operator: ArithmeticOperation::Multiplication,
                }),
                right: Box::new(token!(num_lit 3)),
                operator: ArithmeticOperation::Addition,
            }
        );
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
