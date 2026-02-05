use crate::{
    lexer::{
        ArithmeticOperation, BitwiseOperation, Expression, LogicalOperation, MultiErrorItem,
        NumericLiteral, Reference, RelationalOperation, TokenizerStreamItem, def_yeet, err,
        node::consume_node_path, skip_possible, warning,
    },
    report::{
        PrimitiveMainMessage, PrimitiveReport, PrimitiveReportMessage, PrimitiveReportSegment,
        ReportInlineMessage,
    },
    result::{ParseErrorReport, StreamResult, StreamedError, Warnings},
    stream_utils::StreamPrepend,
    tokenizer::{
        ArithmeticOperator, BitwiseOperator, GenericLiteral, GenericLiteralType, GroupType,
        LiteralToken, LogicalOperator, RelationalOperator, Span, Token,
    },
};

#[derive(Debug)]
enum BinaryOperation {
    Arithmetic(ArithmeticOperation),
    Relational(RelationalOperation),
    Logical(LogicalOperation),
    Bitwise(BitwiseOperation),
}

impl From<ArithmeticOperator> for BinaryOperation {
    fn from(value: ArithmeticOperator) -> Self {
        match value {
            ArithmeticOperator::Plus => Self::Arithmetic(ArithmeticOperation::Addition),
            ArithmeticOperator::Dash => Self::Arithmetic(ArithmeticOperation::Subtraction),
            ArithmeticOperator::Asterisk => Self::Arithmetic(ArithmeticOperation::Multiplication),
            ArithmeticOperator::Percent => Self::Arithmetic(ArithmeticOperation::Modulo),
        }
    }
}

impl From<RelationalOperator> for BinaryOperation {
    fn from(value: RelationalOperator) -> Self {
        match value {
            RelationalOperator::Equal => Self::Relational(RelationalOperation::Equal),
            RelationalOperator::NotEqual => Self::Relational(RelationalOperation::NotEqual),
            RelationalOperator::LessOrEqual => Self::Relational(RelationalOperation::LessOrEqual),
            RelationalOperator::GreaterOrEqual => {
                Self::Relational(RelationalOperation::GreaterOrEqual)
            }
        }
    }
}

impl From<LogicalOperator> for BinaryOperation {
    fn from(value: LogicalOperator) -> Self {
        match value {
            LogicalOperator::Or => Self::Logical(LogicalOperation::Or),
            LogicalOperator::And => Self::Logical(LogicalOperation::And),
            LogicalOperator::Not => panic!("invalid conversion of not operator"),
        }
    }
}

impl From<BitwiseOperator> for BinaryOperation {
    fn from(value: BitwiseOperator) -> Self {
        match value {
            BitwiseOperator::Or => Self::Bitwise(BitwiseOperation::Or),
            BitwiseOperator::Xor => Self::Bitwise(BitwiseOperation::Xor),
            BitwiseOperator::LeftShift => Self::Bitwise(BitwiseOperation::LeftShift),
            BitwiseOperator::RightShift => Self::Bitwise(BitwiseOperation::RightShift),
            BitwiseOperator::Not => panic!("invalid conversion of bitwise not operator"),
        }
    }
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
        autodef!(Arithmetic, Relational, Logical, Bitwise)
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
        autodef!(Logical, Bitwise, Arithmetic, Relational)
    }
}

#[derive(Debug)]
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

#[derive(Debug, Default)]
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

    fn last_operator_span(&self) -> Option<Span> {
        Some(self.stack.last()?.operator_span.clone())
    }
}

#[cfg(test)]
mod binary_operation_parsing {
    use super::*;
    use crate::lexer::NumericLiteral;

    macro_rules! token {
        (num_lit $num: literal) => {
            Expression::NumericLiteral(NumericLiteral::Hexadecimal((
                $num.to_string(),
                Span::default(),
            )))
        };
    }

    #[test]
    fn left_to_right() {
        let tokens = [
            (
                token!(num_lit 10),
                BinaryOperation::Arithmetic(ArithmeticOperation::Addition),
            ),
            (
                token!(num_lit 2),
                BinaryOperation::Arithmetic(ArithmeticOperation::Addition),
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
                BinaryOperation::Arithmetic(ArithmeticOperation::Addition),
            ),
            (
                token!(num_lit 2),
                BinaryOperation::Arithmetic(ArithmeticOperation::Multiplication),
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
                BinaryOperation::Arithmetic(ArithmeticOperation::Multiplication),
            ),
            (
                token!(num_lit 2),
                BinaryOperation::Arithmetic(ArithmeticOperation::Addition),
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

    #[test]
    fn merge_empty() {
        let mut stack = BinaryOperationParsingStack::default();
        assert_eq!(stack.merge(Expression::Invalid), Expression::Invalid);
    }
}

#[derive(Debug)]
enum TernaryParsingStaging {
    StagingThenExpression {
        if_operator: Span,
        if_expr: Expression,
        then_expr: BinaryOperationParsingStack,
    },
    StagingElseExpression {
        if_operator: Span,
        if_expr: Expression,
        then_expr: Expression,
        else_operator: Span,
        else_expr: BinaryOperationParsingStack,
    },
}

enum TernaryElseResult {
    Continue(TernaryParsingStaging),
    End {
        result: Expression,
        warnings: Vec<ParseErrorReport>,
    },
}

impl TernaryParsingStaging {
    fn new(if_operator: Span, if_expr: Expression) -> Self {
        Self::StagingThenExpression {
            if_operator,
            if_expr,
            then_expr: Default::default(),
        }
    }

    fn take_with(&mut self, with: Expression) -> Expression {
        match self {
            Self::StagingThenExpression { then_expr, .. } => then_expr.merge(with),
            Self::StagingElseExpression { else_expr, .. } => else_expr.merge(with),
        }
    }

    /// If this expression already has an else delimiter, this assumes that it is a delimiter for
    /// another ternary expression.
    fn else_delimiter(self, finish_with: Expression, else_operator: &Span) -> TernaryElseResult {
        match self {
            Self::StagingThenExpression {
                if_operator,
                if_expr,
                mut then_expr,
            } => {
                let then_expr = then_expr.merge(finish_with);
                TernaryElseResult::Continue(Self::StagingElseExpression {
                    if_operator,
                    if_expr,
                    then_expr,
                    else_operator: else_operator.clone(),
                    else_expr: Default::default(),
                })
            }
            Self::StagingElseExpression {
                if_operator,
                if_expr,
                then_expr,
                else_operator,
                else_expr,
            } => {
                let (result, warnings) = Self::finish(
                    if_operator,
                    if_expr,
                    then_expr,
                    else_operator,
                    else_expr,
                    finish_with,
                );
                TernaryElseResult::End { result, warnings }
            }
        }
    }

    /// Will produce warnings if the values are not nested parentheses or numeric literals
    fn finish(
        if_operator: Span,
        if_expr: Expression,
        then_expr: Expression,
        else_operator: Span,
        mut else_expr: BinaryOperationParsingStack,
        finish_with: Expression,
    ) -> (Expression, Vec<ParseErrorReport>) {
        let (if_expr, then_expr) = (Box::new(if_expr), Box::new(then_expr));
        let else_expr = Box::new(else_expr.merge(finish_with));
        let mut warnings = Vec::new();
        Self::check_nested(
            "the expression before this is not wrapped in parentheses",
            &if_operator,
            &if_expr,
        )
        .map(|w| warnings.push(w));
        Self::check_nested_after(&if_operator, &then_expr).map(|w| warnings.push(w));
        Self::check_nested_after(&else_operator, &else_expr).map(|w| warnings.push(w));
        (
            Expression::TernaryOperation {
                if_expr,
                then_expr,
                else_expr,
            },
            warnings,
        )
    }

    /// Checks if the expression is a numeric literal or a reference, otherwise generates a warning.
    fn check_nested(
        msg: &'static str,
        operator_span: &Span,
        expr: &Expression,
    ) -> Option<ParseErrorReport> {
        match expr {
            Expression::Group(_) | Expression::NumericLiteral(_) | Expression::Invalid => None,
            _ => Some(warning!({ UnenclosedNestedExpression, [
                (msg.to_string(), operator_span.clone()),
            ]})),
        }
    }

    fn check_nested_after(operator_span: &Span, expr: &Expression) -> Option<ParseErrorReport> {
        Self::check_nested(
            "the expression after this is not wrapped in parentheses",
            operator_span,
            expr,
        )
    }

    /// # Errors
    /// Returns an error if operator's span if this expression can't be finished yet because the if
    /// expression has not yet been finished.
    fn end(
        self,
        finish_with: Expression,
        finish_trigger: &Span,
    ) -> Result<(Expression, Vec<ParseErrorReport>), ParseErrorReport> {
        match self {
            Self::StagingThenExpression { if_operator, .. } => {
                Err(err!(raw [{ InvalidTernaryOperator, [
                    ("this ternary operator must contain an else expression", if_operator),
                    ("before here", finish_trigger.clone()),
                ]}]))
            }
            Self::StagingElseExpression {
                if_operator,
                if_expr,
                then_expr,
                else_operator,
                else_expr,
            } => Ok(Self::finish(
                if_operator,
                if_expr,
                then_expr,
                else_operator,
                else_expr,
                finish_with,
            )),
        }
    }

    fn push(&mut self, operator: BinaryOperation, span: Span, middle: Expression) {
        match self {
            Self::StagingThenExpression { then_expr, .. } => then_expr.push(operator, span, middle),
            Self::StagingElseExpression { else_expr, .. } => else_expr.push(operator, span, middle),
        }
    }
}

#[derive(Debug)]
enum UnaryOperation {
    BitwiseNot,
    LogicalNot,
}

#[derive(Debug)]
enum GroupParsingStackItem {
    Paren {
        left: Span,
        unary_operations: Vec<UnaryOperation>,
        content: BinaryOperationParsingStack,
    },
    Ternary {
        staging: TernaryParsingStaging,
    },
}

impl GroupParsingStackItem {
    fn take_with(&mut self, with: Expression) -> Expression {
        match self {
            Self::Paren { content, .. } => content.merge(with),
            Self::Ternary { staging } => staging.take_with(with),
        }
    }

    fn push(&mut self, expr: Expression, operator: BinaryOperation, operator_span: Span) {
        match self {
            Self::Paren { content, .. } => content.push(operator, operator_span, expr),
            Self::Ternary { staging } => staging.push(operator, operator_span, expr),
        }
    }
}

#[derive(Debug)]
struct GroupParsingStack {
    stack: Vec<GroupParsingStackItem>,
}

type WarningReports = Vec<ParseErrorReport>;
type ErrorReports = Vec<ParseErrorReport>;
type HasAllClosed = bool;

#[derive(Debug, PartialEq)]
enum GroupParsingStackPopResult {
    Continue(Expression),
    Ended(Expression),
}

fn nest_unary(inner: Expression, mut unary_operators: Vec<UnaryOperation>) -> Expression {
    let mut tmp = inner;
    while let Some(op) = unary_operators.pop() {
        match op {
            UnaryOperation::BitwiseNot => tmp = Expression::BitwiseNot(Box::new(tmp)),
            UnaryOperation::LogicalNot => tmp = Expression::LogicalNot(Box::new(tmp)),
        }
    }
    tmp
}

impl GroupParsingStack {
    fn new(start_paren: Span) -> Self {
        Self {
            stack: Vec::from([GroupParsingStackItem::Paren {
                left: start_paren,
                unary_operations: Vec::default(),
                content: Default::default(),
            }]),
        }
    }

    fn start_paren(&mut self, span: Span, with_unary_chain: Vec<UnaryOperation>) {
        self.stack.push(GroupParsingStackItem::Paren {
            left: span,
            unary_operations: with_unary_chain,
            content: Default::default(),
        });
    }

    fn start_ternary(&mut self, with: Expression, operator: Span) {
        let left = match self.stack.last_mut() {
            Some(v) => v.take_with(with),
            None => with,
        };
        self.stack.push(GroupParsingStackItem::Ternary {
            staging: TernaryParsingStaging::new(operator, left),
        });
    }

    /// # Errors
    /// Errors if there is an unclosed ternary operator before a parenthesis.
    /// Error structure is: (errors, all_closed)
    fn end_paren(
        &mut self,
        span: Span,
        with: Expression,
    ) -> (GroupParsingStackPopResult, WarningReports, ErrorReports) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut tmp = with;
        while let Some(v) = self.stack.pop() {
            tmp = match v {
                GroupParsingStackItem::Paren {
                    mut content,
                    unary_operations,
                    left: _,
                } => {
                    if self.stack.is_empty() {
                        let content = nest_unary(content.merge(tmp), unary_operations);
                        return (GroupParsingStackPopResult::Ended(content), warnings, errors);
                    } else {
                        let content = nest_unary(
                            Expression::Group(Box::new(content.merge(tmp))),
                            unary_operations,
                        );
                        return (
                            GroupParsingStackPopResult::Continue(content),
                            warnings,
                            errors,
                        );
                    }
                }
                GroupParsingStackItem::Ternary { staging } => {
                    let end = staging.end(tmp, &span);
                    match end {
                        Ok((r, w)) => {
                            warnings.extend(w.into_iter());
                            r
                        }
                        Err(e) => {
                            errors.push(e);
                            Expression::Invalid
                        }
                    }
                }
            };
        }
        panic!("can't pop a parenthesis on an empty stack");
    }

    fn ternary_else(
        &mut self,
        span: Span,
        with: Expression,
    ) -> Result<WarningReports, ParseErrorReport> {
        let mut result_warnings = Vec::new();
        let mut tmp = with;
        loop {
            match self.stack.last() {
                Some(GroupParsingStackItem::Paren { left, .. }) => {
                    return Err(err!(raw [{ InvalidTernaryOperator, [
                        ("there is no unclosed ternary operator since this", left.clone()),
                        ("what ternary operator are you refering to?", span),
                    ] }]));
                }
                Some(GroupParsingStackItem::Ternary { .. }) => {}
                None => panic!("can't pop a ternary operator on an empty stack"),
            }
            let Some(GroupParsingStackItem::Ternary { staging }) = self.stack.pop() else {
                unreachable!();
            };
            match staging.else_delimiter(tmp, &span) {
                TernaryElseResult::Continue(staging) => {
                    self.stack.push(GroupParsingStackItem::Ternary { staging });
                    return Ok(result_warnings);
                }
                TernaryElseResult::End { result, warnings } => {
                    result_warnings.extend(warnings.into_iter());
                    tmp = result;
                }
            }
        }
    }

    fn push(&mut self, expr: Expression, operator: BinaryOperation, operator_span: Span) {
        let Some(last) = self.stack.last_mut() else {
            panic!("can't push on an empty stack");
        };
        last.push(expr, operator, operator_span);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parenthesis_only() {
        let mut stack = GroupParsingStack::new(Default::default());
        let (res, warnings, errors) = stack.end_paren(Default::default(), Expression::Invalid);
        assert_eq!(res, GroupParsingStackPopResult::Ended(Expression::Invalid));
        assert!(warnings.is_empty());
        assert!(errors.is_empty());
    }

    #[test]
    fn single_ternary() {
        let mut stack = GroupParsingStack::new(Default::default());
        stack.start_ternary(Expression::Invalid, Default::default());
        assert!(
            stack
                .ternary_else(Default::default(), Expression::Invalid)
                .unwrap()
                .is_empty()
        );
        let (res, warnings, errors) = stack.end_paren(Default::default(), Expression::Invalid);
        assert_eq!(
            res,
            GroupParsingStackPopResult::Ended(Expression::TernaryOperation {
                if_expr: Box::new(Expression::Invalid),
                then_expr: Box::new(Expression::Invalid),
                else_expr: Box::new(Expression::Invalid)
            })
        );
        assert_eq!(warnings.len(), 0);
        assert!(errors.is_empty());
    }

    #[test]
    fn nested_ternary_then() {
        let mut stack = GroupParsingStack::new(Default::default());
        stack.start_ternary(Expression::Invalid, Default::default());
        stack.start_ternary(Expression::Invalid, Default::default());
        assert!(
            stack
                .ternary_else(Default::default(), Expression::Invalid)
                .unwrap()
                .is_empty()
        );
        println!("{stack:?}");
        let warnings = stack
            .ternary_else(Default::default(), Expression::Invalid)
            .unwrap();
        assert_eq!(warnings.len(), 0);
        let (res, warnings, errors) = stack.end_paren(Default::default(), Expression::Invalid);
        assert_eq!(
            res,
            GroupParsingStackPopResult::Ended(Expression::TernaryOperation {
                if_expr: Box::new(Expression::Invalid),
                then_expr: Box::new(Expression::TernaryOperation {
                    if_expr: Box::new(Expression::Invalid),
                    then_expr: Box::new(Expression::Invalid),
                    else_expr: Box::new(Expression::Invalid),
                }),
                else_expr: Box::new(Expression::Invalid),
            })
        );
        assert_eq!(warnings.len(), 1);
        assert!(errors.is_empty());
    }

    #[test]
    fn nested_ternary_else() {
        let mut stack = GroupParsingStack::new(Default::default());
        stack.start_ternary(Expression::Invalid, Default::default());
        assert!(
            stack
                .ternary_else(Default::default(), Expression::Invalid)
                .unwrap()
                .is_empty()
        );
        stack.start_ternary(Expression::Invalid, Default::default());
        assert!(
            stack
                .ternary_else(Default::default(), Expression::Invalid)
                .unwrap()
                .is_empty()
        );
        let (res, warnings, errors) = stack.end_paren(Default::default(), Expression::Invalid);
        assert_eq!(
            res,
            GroupParsingStackPopResult::Ended(Expression::TernaryOperation {
                if_expr: Box::new(Expression::Invalid),
                then_expr: Box::new(Expression::Invalid),
                else_expr: Box::new(Expression::TernaryOperation {
                    if_expr: Box::new(Expression::Invalid),
                    then_expr: Box::new(Expression::Invalid),
                    else_expr: Box::new(Expression::Invalid),
                }),
            })
        );
        assert_eq!(warnings.len(), 1);
        assert!(errors.is_empty());
    }
}

pub fn consume_label_reference<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    ampersand: Span,
) -> MultiErrorItem<Reference> {
    def_yeet!(require next from source => vec with message default);
    def_yeet!([vectorize]);
    let next = req_next!(ampersand);
    match next.token {
        Token::GroupOpening(GroupType::Brace) => {
            let (r, errors) = yeet_value!(consume_node_path(source, &next.span));
            if !errors.is_empty() {
                return StreamResult::ProcessingError(StreamedError::CanContinue(errors));
            }
            let Some((path, addr)) = r else {
                return StreamResult::ProcessingError(StreamedError::CanContinue(vec![
                    err!(raw [{
                        InvalidNodePath, [("this remains unclosed until eof", next.span)]
                    }]),
                ]));
            };
            StreamResult::Ok(Reference::NodePath(path, addr))
        }
        Token::Literal(LiteralToken::Ident(GenericLiteral {
            content,
            of_type: GenericLiteralType::Ident,
        })) => {
            return StreamResult::Ok(Reference::Label(content, next.span));
        }
        Token::Literal(LiteralToken::Ident(_)) => {
            return err!(cont_multi [{ InvalidReference, [
                ("this is not a valid label identifier", next.span),
                ("expected a label identifier after this", ampersand),
            ]}]);
        }
        _ => {
            source.push(StreamResult::Ok(next));
            return err!(cont_multi [{ InvalidReference, [
                ("expected a valid label identifier after this", ampersand),
            ]}]);
        }
    }
}

pub fn consume_expression<
    I: Iterator<Item = TokenizerStreamItem> + StreamPrepend<TokenizerStreamItem>,
>(
    source: &mut I,
    start: Span,
) -> MultiErrorItem<(Expression, WarningReports, ErrorReports)> {
    def_yeet!(require next from source => vec with message default);
    let mut stack = GroupParsingStack::new(start.clone());
    let mut res_errors = Vec::new();
    let mut res_warnings = Vec::new();
    let mut unary_operations = Vec::new();
    let mut prev_expr = None;
    macro_rules! req_prev_expr {
        ($before: expr) => {{
            if !unary_operations.is_empty() {
                unary_operations.clear();
                res_errors.push(err!(raw [{ InvalidExpression, [
                    ("expected an expression before this, but got an operator", $before.clone()),
                ]}]));
            }
            match prev_expr.take() {
                Some(v) => v,
                None => {
                    res_errors.push(err!(raw [{ InvalidExpression, [
                        ("expected an expression before this", $before.clone()),
                    ]}]));
                    Expression::Invalid
                }
            }
        }};

        (none $this: expr) => {
            if prev_expr.is_some() {
                res_errors.push(err!(raw [{ InvalidExpression, [
                    ("expected an operator or an expression end instead of this", $this),
                ]}]));
                continue;
            }
        };
    }
    macro_rules! unary {
        ($inner: expr) => {
            if unary_operations.is_empty() {
                $inner
            } else {
                nest_unary($inner, std::mem::take(&mut unary_operations))
            }
        };
    }
    loop {
        skip_possible(source);
        let token = req_next!(start);
        match token.token {
            Token::Literal(LiteralToken::Ident(GenericLiteral {
                mut content,
                of_type: GenericLiteralType::HexadecimalNumeric { prefix },
            })) => {
                req_prev_expr!(none token.span);
                if !prefix {
                    res_errors.push(err!(raw [{ InvalidNumericLiteral, [
                        (
                            "hexadecimals in integer arrays (`<`, `>`) must contain prefix `0x`",
                            token.span.clone()
                        ),
                    ]}]));
                } else {
                    content = content.strip_prefix("0x").unwrap().to_string();
                }
                prev_expr = Some(unary!(Expression::NumericLiteral(
                    NumericLiteral::Hexadecimal((content, token.span)),
                )));
            }
            Token::Literal(LiteralToken::Ident(GenericLiteral {
                content,
                of_type: GenericLiteralType::DecimalNumeric,
            })) => {
                req_prev_expr!(none token.span);
                prev_expr = Some(unary!(Expression::NumericLiteral(NumericLiteral::Decimal(
                    (content, token.span)
                ))));
            }
            Token::Literal(LiteralToken::String(_)) => {
                res_errors.push(err!(raw [{ InvalidExpression, [
                    ("did not expect a string inside an expression", token.span),
                ]}]));
            }
            Token::GroupOpening(GroupType::Paren) => {
                stack.start_paren(token.span, std::mem::take(&mut unary_operations));
            }
            Token::GroupClosing(GroupType::Paren) => {
                let prev = req_prev_expr!(token.span);
                let (res, warnings, errors) = stack.end_paren(token.span, prev);
                res_warnings.extend(warnings.into_iter());
                res_errors.extend(errors.into_iter());
                match res {
                    GroupParsingStackPopResult::Continue(v) => prev_expr = Some(v),
                    GroupParsingStackPopResult::Ended(r) => {
                        return StreamResult::Ok((r, res_warnings, res_errors));
                    }
                }
            }
            Token::QuestionMark => {
                let prev = req_prev_expr!(token.span);
                stack.start_ternary(prev, token.span);
            }
            Token::Colon => {
                let prev = req_prev_expr!(token.span);
                match stack.ternary_else(token.span, prev) {
                    Ok(warnings) => res_warnings.extend(warnings.into_iter()),
                    Err(error) => res_errors.push(error),
                }
            }

            Token::BitwiseOperator(BitwiseOperator::Not) => {
                unary_operations.push(UnaryOperation::BitwiseNot);
            }
            Token::LogicalOperator(LogicalOperator::Not) => {
                unary_operations.push(UnaryOperation::LogicalNot);
            }

            Token::Ampersand => {
                let prev = req_prev_expr!(token.span);
                stack.push(
                    prev,
                    BinaryOperation::Bitwise(BitwiseOperation::And),
                    token.span,
                );
            }
            Token::Slash => {
                let prev = req_prev_expr!(token.span);
                stack.push(
                    prev,
                    BinaryOperation::Arithmetic(ArithmeticOperation::Division),
                    token.span,
                );
            }
            Token::ArithmeticOperator(op) => {
                let prev = req_prev_expr!(token.span);
                stack.push(prev, op.into(), token.span);
            }
            Token::BitwiseOperator(op) => {
                let prev = req_prev_expr!(token.span);
                stack.push(prev, op.into(), token.span);
            }
            Token::LogicalOperator(op) => {
                let prev = req_prev_expr!(token.span);
                stack.push(prev, op.into(), token.span);
            }
            Token::RelationalOperator(op) => {
                let prev = req_prev_expr!(token.span);
                stack.push(prev, op.into(), token.span);
            }

            _ => res_errors.push(err!(raw [{ InvalidExpression, [
                ("did not expect this token", token.span),
            ]}])),
        }
    }
}
