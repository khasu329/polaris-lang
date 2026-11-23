use crate::token::Token;

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
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
        Some(ch)
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), String> {
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
                    return Err("Unterminated block comment".to_string());
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments()?;
            let Some(c) = self.peek() else {
                tokens.push(Token::Eof);
                break;
            };

            match c {
                ':' => {
                    self.advance();
                    tokens.push(Token::Colon);
                }
                ';' => {
                    self.advance();
                    tokens.push(Token::Semicolon);
                }
                ',' => {
                    self.advance();
                    tokens.push(Token::Comma);
                }
                '(' => {
                    self.advance();
                    tokens.push(Token::LParen);
                }
                ')' => {
                    self.advance();
                    tokens.push(Token::RParen);
                }
                '[' => {
                    self.advance();
                    tokens.push(Token::LBracket);
                }
                ']' => {
                    self.advance();
                    tokens.push(Token::RBracket);
                }
                '{' => {
                    self.advance();
                    tokens.push(Token::LBrace);
                }
                '}' => {
                    self.advance();
                    tokens.push(Token::RBrace);
                }
                '+' => {
                    self.advance();
                    tokens.push(Token::Plus);
                }
                '-' => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        tokens.push(Token::Arrow);
                    } else {
                        tokens.push(Token::Minus);
                    }
                }
                '*' => {
                    self.advance();
                    tokens.push(Token::Star);
                }
                '/' => {
                    self.advance();
                    tokens.push(Token::Slash);
                }
                '%' => {
                    self.advance();
                    tokens.push(Token::Percent);
                }
                '&' => {
                    self.advance();
                    if self.peek() == Some('&') {
                        self.advance();
                    }
                    tokens.push(Token::And);
                }
                '|' => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                    }
                    tokens.push(Token::Or);
                }
                '=' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::Eq);
                    } else {
                        tokens.push(Token::Assign);
                    }
                }
                '!' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::Neq);
                    } else {
                        tokens.push(Token::Not);
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::Le);
                    } else {
                        tokens.push(Token::Lt);
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        tokens.push(Token::Ge);
                    } else {
                        tokens.push(Token::Gt);
                    }
                }
                '.' => {
                    self.advance();
                    if self.peek() == Some('.') {
                        self.advance();
                        tokens.push(Token::DotDot);
                    } else {
                        tokens.push(Token::Dot);
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
                            Some(other) => return Err(format!("Unknown escape \\{}", other)),
                            None => return Err("Unterminated char literal".to_string()),
                        },
                        Some(ch) => ch,
                        None => return Err("Unterminated char literal".to_string()),
                    };
                    if self.advance() != Some('\'') {
                        return Err("Expected closing '\'' for char literal".to_string());
                    }
                    tokens.push(Token::Char(ch));
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
                                Some('\\') => s.push('\\'),
                                Some('"') => s.push('"'),
                                Some(other) => return Err(format!("Unknown escape \\{}", other)),
                                None => return Err("Unterminated string literal".to_string()),
                            },
                            Some(ch) => s.push(ch),
                            None => return Err("Unterminated string literal".to_string()),
                        }
                    }
                    tokens.push(Token::Str(s));
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
                            .map_err(|e| format!("Invalid float: {}", e))?;
                        tokens.push(Token::Float(val));
                    } else {
                        let val: i64 =
                            num_str.parse().map_err(|e| format!("Invalid int: {}", e))?;
                        tokens.push(Token::Int(val));
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
                        "true" => tokens.push(Token::Bool(true)),
                        "false" => tokens.push(Token::Bool(false)),
                        "if" => tokens.push(Token::If),
                        "else" => tokens.push(Token::Else),
                        "for" => tokens.push(Token::For),
                        "in" => tokens.push(Token::In),
                        "while" => tokens.push(Token::While),
                        "break" => tokens.push(Token::Break),
                        "continue" => tokens.push(Token::Continue),
                        "fn" => tokens.push(Token::Fn),
                        "class" => tokens.push(Token::Class),
                        "return" => tokens.push(Token::Return),
                        "self" => tokens.push(Token::SelfKw),
                        _ => tokens.push(Token::Ident(ident)),
                    }
                }
                other => return Err(format!("Unexpected character: '{}'", other)),
            }
        }
        Ok(tokens)
    }
}
