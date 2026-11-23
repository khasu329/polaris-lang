use crate::diagnostic::{Diagnostic, Span};
use crate::token::{SpannedToken, Token};

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn current_span(&self) -> Span {
        Span::new(self.line, self.col)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn err(&self, span: Span, msg: impl Into<String>) -> Diagnostic {
        Diagnostic::new("lex", span, msg)
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), Diagnostic> {
        loop {
            while let Some(c) = self.peek() {
                if c.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }
            if self.peek() == Some('/') && self.peek_next() == Some('/') {
                while let Some(c) = self.advance() {
                    if c == '\n' {
                        break;
                    }
                }
            } else if self.peek() == Some('/') && self.peek_next() == Some('*') {
                let start_span = self.current_span();
                self.advance();
                self.advance();
                let mut closed = false;
                while let Some(c) = self.advance() {
                    if c == '*' && self.peek() == Some('/') {
                        self.advance();
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err(self
                        .err(start_span, "Unterminated block comment")
                        .with_hint("Close the block comment with '*/'"));
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, Diagnostic> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments()?;
            let span = self.current_span();
            let Some(c) = self.peek() else {
                tokens.push(SpannedToken {
                    token: Token::Eof,
                    span,
                });
                break;
            };

            let token = match c {
                ':' => {
                    self.advance();
                    Token::Colon
                }
                ';' => {
                    self.advance();
                    Token::Semicolon
                }
                ',' => {
                    self.advance();
                    Token::Comma
                }
                '(' => {
                    self.advance();
                    Token::LParen
                }
                ')' => {
                    self.advance();
                    Token::RParen
                }
                '[' => {
                    self.advance();
                    Token::LBracket
                }
                ']' => {
                    self.advance();
                    Token::RBracket
                }
                '{' => {
                    self.advance();
                    Token::LBrace
                }
                '}' => {
                    self.advance();
                    Token::RBrace
                }
                '+' => {
                    self.advance();
                    Token::Plus
                }
                '-' => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        Token::Arrow
                    } else {
                        Token::Minus
                    }
                }
                '*' => {
                    self.advance();
                    Token::Star
                }
                '/' => {
                    self.advance();
                    Token::Slash
                }
                '%' => {
                    self.advance();
                    Token::Percent
                }
                '&' => {
                    self.advance();
                    if self.peek() == Some('&') {
                        self.advance();
                        Token::And
                    } else {
                        Token::Amp
                    }
                }
                '|' => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                        Token::Or
                    } else {
                        return Err(self
                            .err(span, "Unexpected single '|'")
                            .with_hint("Use '||' for logical OR"));
                    }
                }
                '=' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Eq
                    } else {
                        Token::Assign
                    }
                }
                '!' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Neq
                    } else {
                        Token::Not
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Le
                    } else {
                        Token::Lt
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Token::Ge
                    } else {
                        Token::Gt
                    }
                }
                '.' => {
                    self.advance();
                    if self.peek() == Some('.') {
                        self.advance();
                        Token::DotDot
                    } else {
                        Token::Dot
                    }
                }
                '\'' => {
                    self.advance();
                    let ch = match self.advance() {
                        Some('\\') => match self.advance() {
                            Some('n') => '\n',
                            Some('t') => '\t',
                            Some('r') => '\r',
                            Some('0') => '\0',
                            Some('\\') => '\\',
                            Some('\'') => '\'',
                            Some(other) => {
                                return Err(
                                    self.err(span, format!("Unknown escape sequence '\\{}'", other))
                                );
                            }
                            None => return Err(self.err(span, "Unterminated character literal")),
                        },
                        Some(ch) => ch,
                        None => return Err(self.err(span, "Unterminated character literal")),
                    };
                    if self.advance() != Some('\'') {
                        return Err(self
                            .err(span, "Expected closing '\'' for character literal")
                            .with_hint("Character literals must contain exactly one character"));
                    }
                    Token::Char(ch)
                }
                '"' => {
                    self.advance();
                    let mut s = String::new();
                    loop {
                        match self.advance() {
                            Some('"') => break,
                            Some('\\') => match self.advance() {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some('r') => s.push('\r'),
                                Some('0') => s.push('\0'),
                                Some('\\') => s.push('\\'),
                                Some('"') => s.push('"'),
                                Some(other) => {
                                    return Err(self.err(
                                        span,
                                        format!("Unknown escape sequence '\\{}'", other),
                                    ));
                                }
                                None => return Err(self.err(span, "Unterminated string literal")),
                            },
                            Some(ch) => s.push(ch),
                            None => {
                                return Err(self
                                    .err(span, "Unterminated string literal")
                                    .with_hint("Add a closing '\"'"));
                            }
                        }
                    }
                    Token::Str(s)
                }
                c if c.is_ascii_digit() => {
                    let mut num_str = String::new();
                    let mut is_float = false;
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_digit() {
                            num_str.push(ch);
                            self.advance();
                        } else if ch == '.' && self.peek_next().is_some_and(|n| n.is_ascii_digit())
                        {
                            is_float = true;
                            num_str.push(ch);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    if is_float {
                        let val: f64 = num_str
                            .parse()
                            .map_err(|e| self.err(span, format!("Invalid float '{}': {}", num_str, e)))?;
                        Token::Float(val)
                    } else {
                        let val: i64 = num_str
                            .parse()
                            .map_err(|e| self.err(span, format!("Invalid int '{}': {}", num_str, e)))?;
                        Token::Int(val)
                    }
                }
                c if c.is_ascii_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_alphanumeric() || ch == '_' {
                            ident.push(ch);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    match ident.as_str() {
                        "true" => Token::Bool(true),
                        "false" => Token::Bool(false),
                        "if" => Token::If,
                        "else" => Token::Else,
                        "for" => Token::For,
                        "in" => Token::In,
                        "while" => Token::While,
                        "break" => Token::Break,
                        "continue" => Token::Continue,
                        "fn" => Token::Fn,
                        "class" => Token::Class,
                        "return" => Token::Return,
                        "self" => Token::SelfKw,
                        "import" => Token::Import,
                        "as" => Token::As,
                        _ => Token::Ident(ident),
                    }
                }
                other => {
                    return Err(self.err(span, format!("Unexpected character '{}'", other)));
                }
            };

            tokens.push(SpannedToken { token, span });
        }
        Ok(tokens)
    }
}
