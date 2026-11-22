#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Int(i64),
    Float(f64),
    Char(char),
    Str(String),
    Bool(bool),

    // Operators
    Assign,  // =
    Eq,      // ==
    Neq,     // !=
    Lt,      // <
    Gt,      // >
    Le,      // <=
    Ge,      // >=
    Plus,    // +
    Minus,   // -
    Star,    // *
    Slash,   // /
    Percent, // %
    And,     // &
    Or,      // |
    Not,     // !

    // Punctuation
    Colon,     // :
    Semicolon, // ;
    Comma,     // ,
    Ellipsis,  // ...
    DotDot,    // ..
    LParen,    // (
    RParen,    // )
    LBracket,  // [
    RBracket,  // ]
    LBrace,    // {
    RBrace,    // }

    // Keywords
    If,
    Else,
    For,
    In,
    Break,
    Continue,

    Eof,
}
