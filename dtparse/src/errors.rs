use crate::result::report;

report!(Errors(E) {
    InvalidUtf8Character[001] => "invalid UTF-8 character",
    InvalidCharacter[002] => "Unknown character encountered",
    InvalidNumericLiteral[003] => "invalid numeric literal",
    InvalidStringLiteral[004] => "invalid string literal",
    UnclosedBlockComment[005] => "unclosed block comment",
    UnexpectedEof[006] => "unxpected end",
    UnexpectedToken[007] => "unexpected token",
    InvalidNodeAddress[008] => "invalid node address",
    InvalidNodeName[009] => "invalid node name",
    InvalidLabelName[010] => "invalid label name",
    UnexpectedWhitespace[011] => "unexpected whitespace or comment",
    MissingParentheses[012] => "missing parentheses",
    UnmatchedDelimiter[013] => "unmatched delimiter",
    InvalidTernaryOperator[014] => "invalid ternary operator",
    InvalidNodePath[015] => "invalid node path",
    InvalidReference[016] => "invalid reference",
    InvalidExpression[017] => "invalid expression",
});

report!(Warnings(W) {
    WeirdPropertyName[001] => "weird property name",
    UnenclosedNestedExpression[002] => "unenclosed nested expression",
});
