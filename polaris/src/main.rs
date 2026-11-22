use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Cursor, Write};

// ==========================================
// 1. Lexer (字句解析器)
// ==========================================

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Int(i64),
    Float(f64),
    Char(char),
    Str(String),
    Bool(bool),

    // Operators
    Assign,    // =
    Eq,        // ==
    Neq,       // !=
    Lt,        // <
    Gt,        // >
    Le,        // <=
    Ge,        // >=
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /
    Percent,   // %
    And,       // &
    Or,        // |
    Not,       // !

    // Punctuation
    Colon,     // :
    Comma,     // ,
    Ellipsis,  // ...
    LParen,    // (
    RParen,    // )
    LBracket,  // [
    RBracket,  // ]
    LBrace,    // {
    RBrace,    // }

    Eof,
}

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

    fn peek_n(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        Some(ch)
    }

    fn skip_whitespace_and_comments(&mut self) {
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
            } else {
                break;
            }
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let Some(c) = self.peek() else {
                tokens.push(Token::Eof);
                break;
            };

            match c {
                ':' => {
                    self.advance();
                    tokens.push(Token::Colon);
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
                    tokens.push(Token::Minus);
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
                    tokens.push(Token::And);
                }
                '|' => {
                    self.advance();
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
                    if self.peek_next() == Some('.') && self.peek_n(2) == Some('.') {
                        self.advance();
                        self.advance();
                        self.advance();
                        tokens.push(Token::Ellipsis);
                    } else {
                        return Err("Unexpected '.' (expected '...')".to_string());
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
                        } else if ch == '.' && self.peek_next().map_or(false, |n| n.is_ascii_digit()) {
                            is_float = true;
                            num_str.push(ch);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    if is_float {
                        let val: f64 = num_str.parse().map_err(|e| format!("Invalid float: {}", e))?;
                        tokens.push(Token::Float(val));
                    } else {
                        let val: i64 = num_str.parse().map_err(|e| format!("Invalid int: {}", e))?;
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
                        _ => tokens.push(Token::Ident(ident)),
                    }
                }
                other => return Err(format!("Unexpected character: '{}'", other)),
            }
        }
        Ok(tokens)
    }
}

// ==========================================
// 2. AST (抽象構文木)
// ==========================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimType {
    Int,
    Uint,
    Float,
    Char,
    String,
    Bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeSpec {
    Scalar(PrimType),
    Array1D {
        elem: PrimType,
        len: Option<Expr>, // None => [...], Some(N) => [...N]
    },
    Table {
        row_types: Vec<PrimType>,
        len: Option<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Char(char),
    Str(String),
    Bool(bool),
    Ident(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternElem {
    Var(String),
    Lit(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtractMode {
    Continuous,
    Any,
    Head,
    Tail,
    Single,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Extraction {
    pub patterns: Vec<Vec<PatternElem>>, // 1D: len=1, 2D: len=rows
    pub target: String,
    pub mode: ExtractMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WhileCond {
    Extract(Extraction),
    OrIn,
    AndIn,
    BoolExpr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RewriteRule {
    pub lhs: Vec<Vec<PatternElem>>,
    pub guard: Option<Expr>,
    pub rhs: Vec<Vec<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignValue {
    Scalar(Expr),
    Slice {
        source: String,
        row: Option<usize>,
        len: Option<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    Token,
    Line,
    Char,
    All,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl {
        name: String,
        ty: TypeSpec,
        input_mode: Option<InputMode>,
    },
    SliceAssign {
        target: String,
        row: Option<usize>,
        len: Option<Expr>,
        value: AssignValue,
    },
    Output {
        target: String,
        row: Option<usize>,
        len: Option<Expr>,
    },
    While {
        cond: WhileCond,
        step: Vec<Extraction>,
        body: Vec<RewriteRule>,
    },
}

// ==========================================
// 3. Parser (構文解析器)
// ==========================================

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn peek_n(&self, n: usize) -> &Token {
        self.tokens.get(self.pos + n).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let t = self.peek().clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, expected: Token) -> Result<(), String> {
        let got = self.advance();
        if got == expected {
            Ok(())
        } else {
            Err(format!("Expected {:?}, got {:?}", expected, got))
        }
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            other => Err(format!("Expected identifier, got {:?}", other)),
        }
    }

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        while *self.peek() != Token::Eof {
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek() {
            Token::Ident(name) if name == "while" => self.parse_while(),
            Token::Ident(_) => {
                if *self.peek_n(1) == Token::Colon {
                    self.parse_var_decl()
                } else {
                    self.parse_ident_stmt()
                }
            }
            other => Err(format!("Unexpected token at start of statement: {:?}", other)),
        }
    }

    fn parse_prim_type(&mut self) -> Result<PrimType, String> {
        let name = self.expect_ident()?;
        match name.as_str() {
            "Int" => Ok(PrimType::Int),
            "Uint" => Ok(PrimType::Uint),
            "Float" => Ok(PrimType::Float),
            "Char" => Ok(PrimType::Char),
            "String" => Ok(PrimType::String),
            "Bool" => Ok(PrimType::Bool),
            _ => Err(format!("Unknown primitive type: {}", name)),
        }
    }

    fn parse_slice_range(&mut self) -> Result<Option<Expr>, String> {
        self.expect(Token::LBracket)?;
        self.expect(Token::Ellipsis)?;
        let len = if *self.peek() == Token::RBracket {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(Token::RBracket)?;
        Ok(len)
    }

    fn parse_var_decl(&mut self) -> Result<Stmt, String> {
        let name = self.expect_ident()?;
        self.expect(Token::Colon)?;

        let ty = if *self.peek() == Token::LBracket {
            self.advance();
            let mut row_types = Vec::new();
            loop {
                row_types.push(self.parse_prim_type()?);
                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RBracket)?;
            let len = self.parse_slice_range()?;
            TypeSpec::Table { row_types, len }
        } else {
            let prim = self.parse_prim_type()?;
            if *self.peek() == Token::LBracket {
                let len = self.parse_slice_range()?;
                TypeSpec::Array1D { elem: prim, len }
            } else {
                TypeSpec::Scalar(prim)
            }
        };

        let mut input_mode = None;
        if *self.peek() == Token::LParen {
            self.advance();
            let mod_name = self.expect_ident()?;
            if mod_name == "Input" {
                let mode = if *self.peek() == Token::LParen {
                    self.advance();
                    let m = self.expect_ident()?;
                    self.expect(Token::RParen)?;
                    match m.as_str() {
                        "token" => InputMode::Token,
                        "line" => InputMode::Line,
                        "char" => InputMode::Char,
                        "all" => InputMode::All,
                        _ => return Err(format!("Unknown Input mode: {}", m)),
                    }
                } else {
                    InputMode::Token
                };
                input_mode = Some(mode);
            } else {
                return Err(format!("Unknown declaration modifier: {}", mod_name));
            }
            self.expect(Token::RParen)?;
        }

        Ok(Stmt::VarDecl { name, ty, input_mode })
    }

    fn parse_slice_target(&mut self) -> Result<(String, Option<usize>, Option<Expr>), String> {
        let name = self.expect_ident()?;
        let mut row = None;
        let mut len = None;
        if *self.peek() == Token::LBracket {
            // Either [row][...len] or [...len]
            if *self.peek_n(1) != Token::Ellipsis {
                self.advance(); // '['
                match self.advance() {
                    Token::Int(idx) if idx >= 0 => row = Some(idx as usize),
                    other => return Err(format!("Expected non-negative row index, got {:?}", other)),
                }
                self.expect(Token::RBracket)?;
                if *self.peek() == Token::LBracket {
                    len = self.parse_slice_range()?;
                }
            } else {
                len = self.parse_slice_range()?;
            }
        }
        Ok((name, row, len))
    }

    fn parse_ident_stmt(&mut self) -> Result<Stmt, String> {
        let (target, row, len) = self.parse_slice_target()?;
        if *self.peek() == Token::LParen {
            self.advance();
            let mod_name = self.expect_ident()?;
            if mod_name != "Output" {
                return Err(format!("Expected Output modifier, got {}", mod_name));
            }
            self.expect(Token::RParen)?;
            Ok(Stmt::Output { target, row, len })
        } else if *self.peek() == Token::Assign {
            self.advance();
            let value = if let Token::Ident(_) = self.peek() {
                if *self.peek_n(1) == Token::LBracket {
                    let (src, s_row, s_len) = self.parse_slice_target()?;
                    AssignValue::Slice {
                        source: src,
                        row: s_row,
                        len: s_len,
                    }
                } else {
                    AssignValue::Scalar(self.parse_expr()?)
                }
            } else {
                AssignValue::Scalar(self.parse_expr()?)
            };
            Ok(Stmt::SliceAssign {
                target,
                row,
                len,
                value,
            })
        } else {
            Err(format!(
                "Expected '=' or '(Output)' after '{}', got {:?}",
                target,
                self.peek()
            ))
        }
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        let kw = self.expect_ident()?;
        if kw != "while" {
            return Err("Expected 'while'".to_string());
        }
        self.expect(Token::LParen)?;

        let cond = match self.peek() {
            Token::LBracket => WhileCond::Extract(self.parse_extraction()?),
            Token::Ident(s) if s == "OR" || s == "AND" => {
                let op = self.expect_ident()?;
                self.expect(Token::LParen)?;
                let arg = self.expect_ident()?;
                if arg != "In" {
                    return Err(format!("Expected 'In' inside {}(), got '{}'", op, arg));
                }
                self.expect(Token::RParen)?;
                if op == "OR" {
                    WhileCond::OrIn
                } else {
                    WhileCond::AndIn
                }
            }
            _ => WhileCond::BoolExpr(self.parse_expr()?),
        };

        let mut step = Vec::new();
        if *self.peek() == Token::Comma {
            self.advance();
            if *self.peek() == Token::LBrace {
                self.advance();
                while *self.peek() != Token::RBrace {
                    step.push(self.parse_extraction()?);
                }
                self.expect(Token::RBrace)?;
            } else {
                step.push(self.parse_extraction()?);
            }
        }

        self.expect(Token::RParen)?;
        self.expect(Token::LBrace)?;

        let mut body = Vec::new();
        while *self.peek() != Token::RBrace {
            body.push(self.parse_rewrite_rule()?);
        }
        self.expect(Token::RBrace)?;

        Ok(Stmt::While { cond, step, body })
    }

    fn parse_pattern_matrix(&mut self) -> Result<Vec<Vec<PatternElem>>, String> {
        self.expect(Token::LBracket)?;
        if *self.peek() == Token::LBracket {
            let mut rows = Vec::new();
            while *self.peek() == Token::LBracket {
                rows.push(self.parse_pattern_row()?);
                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RBracket)?;
            Ok(rows)
        } else {
            let row = self.parse_pattern_row_inner()?;
            self.expect(Token::RBracket)?;
            Ok(vec![row])
        }
    }

    fn parse_pattern_row(&mut self) -> Result<Vec<PatternElem>, String> {
        self.expect(Token::LBracket)?;
        let row = self.parse_pattern_row_inner()?;
        self.expect(Token::RBracket)?;
        Ok(row)
    }

    fn parse_pattern_row_inner(&mut self) -> Result<Vec<PatternElem>, String> {
        let mut elems = Vec::new();
        while *self.peek() != Token::RBracket {
            let elem = match self.peek() {
                Token::Ident(s) => {
                    let name = s.clone();
                    self.advance();
                    PatternElem::Var(name)
                }
                _ => PatternElem::Lit(self.parse_expr()?),
            };
            elems.push(elem);
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        Ok(elems)
    }

    fn parse_extraction(&mut self) -> Result<Extraction, String> {
        let patterns = self.parse_pattern_matrix()?;
        self.expect(Token::Assign)?;
        let target = self.expect_ident()?;
        let mode = if *self.peek() == Token::LParen {
            self.advance();
            let mode_str = self.expect_ident()?;
            self.expect(Token::RParen)?;
            match mode_str.as_str() {
                "continuous" | "continuos" | "continous" => ExtractMode::Continuous,
                "any" => ExtractMode::Any,
                "head" => ExtractMode::Head,
                "tail" => ExtractMode::Tail,
                _ => return Err(format!("Unknown extraction mode: {}", mode_str)),
            }
        } else {
            ExtractMode::Single
        };
        Ok(Extraction {
            patterns,
            target,
            mode,
        })
    }

    fn parse_expr_matrix(&mut self) -> Result<Vec<Vec<Expr>>, String> {
        self.expect(Token::LBracket)?;
        if *self.peek() == Token::LBracket {
            let mut rows = Vec::new();
            while *self.peek() == Token::LBracket {
                self.advance(); // '['
                rows.push(self.parse_expr_list_inner()?);
                self.expect(Token::RBracket)?;
                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RBracket)?;
            Ok(rows)
        } else {
            let row = self.parse_expr_list_inner()?;
            self.expect(Token::RBracket)?;
            Ok(vec![row])
        }
    }

    fn parse_expr_list_inner(&mut self) -> Result<Vec<Expr>, String> {
        let mut exprs = Vec::new();
        while *self.peek() != Token::RBracket {
            exprs.push(self.parse_expr()?);
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        Ok(exprs)
    }

    fn parse_rewrite_rule(&mut self) -> Result<RewriteRule, String> {
        let lhs = self.parse_pattern_matrix()?;
        let guard = if *self.peek() == Token::LParen {
            self.advance();
            let expr = self.parse_expr()?;
            self.expect(Token::RParen)?;
            Some(expr)
        } else {
            None
        };
        self.expect(Token::Assign)?;
        let rhs = self.parse_expr_matrix()?;
        Ok(RewriteRule { lhs, guard, rhs })
    }

    // Expression parsing (precedence climbing)
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_and()?;
        while *self.peek() == Token::Or {
            self.advance();
            let right = self.parse_and()?;
            expr = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_eq()?;
        while *self.peek() == Token::And {
            self.advance();
            let right = self.parse_eq()?;
            expr = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_eq(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_cmp()?;
        while matches!(self.peek(), Token::Eq | Token::Neq) {
            let op = match self.advance() {
                Token::Eq => BinaryOp::Eq,
                Token::Neq => BinaryOp::Neq,
                _ => unreachable!(),
            };
            let right = self.parse_cmp()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_cmp(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_add()?;
        while matches!(self.peek(), Token::Lt | Token::Gt | Token::Le | Token::Ge) {
            let op = match self.advance() {
                Token::Lt => BinaryOp::Lt,
                Token::Gt => BinaryOp::Gt,
                Token::Le => BinaryOp::Le,
                Token::Ge => BinaryOp::Ge,
                _ => unreachable!(),
            };
            let right = self.parse_add()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_add(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_mul()?;
        while matches!(self.peek(), Token::Plus | Token::Minus) {
            let op = match self.advance() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_mul()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_mul(&mut self) -> Result<Expr, String> {
        let mut expr = self.parse_unary()?;
        while matches!(self.peek(), Token::Star | Token::Slash | Token::Percent) {
            let op = match self.advance() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => unreachable!(),
            };
            let right = self.parse_unary()?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Token::Minus => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            Token::Not => {
                self.advance();
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.advance() {
            Token::Int(v) => Ok(Expr::Int(v)),
            Token::Float(v) => Ok(Expr::Float(v)),
            Token::Char(v) => Ok(Expr::Char(v)),
            Token::Str(v) => Ok(Expr::Str(v)),
            Token::Bool(v) => Ok(Expr::Bool(v)),
            Token::Ident(s) => Ok(Expr::Ident(s)),
            Token::LParen => {
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            other => Err(format!("Unexpected token in expression: {:?}", other)),
        }
    }
}

// ==========================================
// 4. Runtime & Interpreter (実行エンジン)
// ==========================================

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Uint(u64),
    Float(f64),
    Char(char),
    String(String),
    Bool(bool),
}

impl Value {
    fn default_for(ty: PrimType) -> Self {
        match ty {
            PrimType::Int => Value::Int(0),
            PrimType::Uint => Value::Uint(0),
            PrimType::Float => Value::Float(0.0),
            PrimType::Char => Value::Char('\0'),
            PrimType::String => Value::String(String::new()),
            PrimType::Bool => Value::Bool(false),
        }
    }

    fn display_str(&self) -> String {
        match self {
            Value::Int(v) => v.to_string(),
            Value::Uint(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::Char(v) => v.to_string(),
            Value::String(v) => v.clone(),
            Value::Bool(v) => v.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Executed,
    NoMatch,
}

#[derive(Debug, Clone)]
pub enum VarStorage {
    Scalar(Value),
    Table {
        row_types: Vec<PrimType>,
        rows: Vec<Vec<Value>>,
        is_fixed: bool,
    },
}

struct InputScanner {
    reader: Box<dyn BufRead>,
    chars: VecDeque<char>,
}

impl InputScanner {
    fn from_str(input: &str) -> Self {
        Self {
            reader: Box::new(Cursor::new(input.as_bytes().to_vec())),
            chars: VecDeque::new(),
        }
    }

    fn from_stdin() -> Self {
        Self {
            reader: Box::new(BufReader::new(io::stdin())),
            chars: VecDeque::new(),
        }
    }

    fn fill_line(&mut self) -> Result<bool, String> {
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .map_err(|e| format!("Failed to read input: {}", e))?;
        if n == 0 {
            return Ok(false);
        }
        for c in line.chars() {
            self.chars.push_back(c);
        }
        Ok(true)
    }

    fn next_token(&mut self) -> Result<String, String> {
        loop {
            while let Some(&c) = self.chars.front() {
                if c.is_whitespace() {
                    self.chars.pop_front();
                } else {
                    break;
                }
            }
            if !self.chars.is_empty() {
                break;
            }
            if !self.fill_line()? {
                return Err("Unexpected end of input".to_string());
            }
        }
        let mut tok = String::new();
        loop {
            while let Some(&c) = self.chars.front() {
                if !c.is_whitespace() {
                    tok.push(self.chars.pop_front().unwrap());
                } else {
                    return Ok(tok);
                }
            }
            if !self.fill_line()? {
                return Ok(tok);
            }
        }
    }

    fn next_line_str(&mut self) -> Result<String, String> {
        // Consume a single trailing newline left over from a prior token read
        if self.chars.front() == Some(&'\r') {
            self.chars.pop_front();
        }
        if self.chars.front() == Some(&'\n') {
            self.chars.pop_front();
        }
        if self.chars.is_empty() && !self.fill_line()? {
            return Err("Unexpected end of input".to_string());
        }
        let mut line = String::new();
        loop {
            while let Some(c) = self.chars.pop_front() {
                if c == '\n' {
                    return Ok(line);
                }
                if c != '\r' {
                    line.push(c);
                }
            }
            if !self.fill_line()? {
                return Ok(line);
            }
        }
    }

    fn next_non_newline_char(&mut self) -> Result<char, String> {
        loop {
            while let Some(c) = self.chars.pop_front() {
                if c != '\r' && c != '\n' {
                    return Ok(c);
                }
            }
            if !self.fill_line()? {
                return Err("Unexpected end of input".to_string());
            }
        }
    }

    fn read_all_str(&mut self) -> Result<String, String> {
        let mut all: String = self.chars.drain(..).collect();
        self.reader
            .read_to_string(&mut all)
            .map_err(|e| format!("Failed to read input: {}", e))?;
        Ok(all)
    }

    fn parse_token_as(tok: &str, ty: PrimType) -> Result<Value, String> {
        match ty {
            PrimType::Int => tok
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|e| format!("Failed to parse Int '{}': {}", tok, e)),
            PrimType::Uint => tok
                .parse::<u64>()
                .map(Value::Uint)
                .map_err(|e| format!("Failed to parse Uint '{}': {}", tok, e)),
            PrimType::Float => tok
                .parse::<f64>()
                .map(Value::Float)
                .map_err(|e| format!("Failed to parse Float '{}': {}", tok, e)),
            PrimType::Char => {
                let mut chars = tok.chars();
                let ch = chars
                    .next()
                    .ok_or_else(|| "Empty token for Char".to_string())?;
                if chars.next().is_some() {
                    return Err(format!("Expected single Char, got '{}'", tok));
                }
                Ok(Value::Char(ch))
            }
            PrimType::String => Ok(Value::String(tok.to_string())),
            PrimType::Bool => tok
                .parse::<bool>()
                .map(Value::Bool)
                .map_err(|e| format!("Failed to parse Bool '{}': {}", tok, e)),
        }
    }

    fn read_value(&mut self, ty: PrimType, mode: InputMode) -> Result<Value, String> {
        match mode {
            InputMode::Token => {
                let tok = self.next_token()?;
                Self::parse_token_as(&tok, ty)
            }
            InputMode::Line => {
                let line = self.next_line_str()?;
                if ty == PrimType::String {
                    Ok(Value::String(line))
                } else {
                    Self::parse_token_as(line.trim(), ty)
                }
            }
            InputMode::Char => {
                let ch = self.next_non_newline_char()?;
                match ty {
                    PrimType::Char => Ok(Value::Char(ch)),
                    PrimType::String => Ok(Value::String(ch.to_string())),
                    _ => Self::parse_token_as(&ch.to_string(), ty),
                }
            }
            InputMode::All => {
                let all = self.read_all_str()?;
                match ty {
                    PrimType::String => Ok(Value::String(all)),
                    _ => Self::parse_token_as(all.trim(), ty),
                }
            }
        }
    }
}

pub struct Interpreter {
    env: HashMap<String, VarStorage>,
    scanner: InputScanner,
}

impl Interpreter {
    pub fn new(input: &str) -> Self {
        Self {
            env: HashMap::new(),
            scanner: InputScanner::from_str(input),
        }
    }

    pub fn from_stdin() -> Self {
        Self {
            env: HashMap::new(),
            scanner: InputScanner::from_stdin(),
        }
    }

    pub fn run(&mut self, stmts: &[Stmt], out: &mut dyn Write) -> Result<(), String> {
        for stmt in stmts {
            self.exec_stmt(stmt, out)?;
        }
        Ok(())
    }

    fn eval_usize(&self, expr: &Expr, locals: &HashMap<String, Value>) -> Result<usize, String> {
        let (val, _) = self.eval_expr(expr, locals)?;
        match val {
            Value::Int(n) if n >= 0 => Ok(n as usize),
            Value::Uint(n) => Ok(n as usize),
            other => Err(format!("Expected non-negative integer length, got {:?}", other)),
        }
    }

    fn exec_stmt(&mut self, stmt: &Stmt, out: &mut dyn Write) -> Result<Status, String> {
        match stmt {
            Stmt::VarDecl { name, ty, input_mode } => {
                let storage = match ty {
                    TypeSpec::Scalar(prim) => {
                        let val = if let Some(mode) = input_mode {
                            self.scanner.read_value(*prim, *mode)?
                        } else {
                            Value::default_for(*prim)
                        };
                        VarStorage::Scalar(val)
                    }
                    TypeSpec::Array1D { elem, len } => {
                        let actual_len = match len {
                            Some(e) => self.eval_usize(e, &HashMap::new())?,
                            None => 0,
                        };
                        let mut row = Vec::with_capacity(actual_len);
                        if let Some(mode) = input_mode {
                            if *mode == InputMode::All && len.is_none() {
                                let all = self.scanner.read_all_str()?;
                                match elem {
                                    PrimType::Char => {
                                        row = all.chars().map(Value::Char).collect();
                                    }
                                    PrimType::String => {
                                        row = all.lines().map(|l| Value::String(l.to_string())).collect();
                                    }
                                    _ => {
                                        for tok in all.split_whitespace() {
                                            row.push(InputScanner::parse_token_as(tok, *elem)?);
                                        }
                                    }
                                }
                            } else {
                                for _ in 0..actual_len {
                                    row.push(self.scanner.read_value(*elem, *mode)?);
                                }
                            }
                        } else {
                            row.resize(actual_len, Value::default_for(*elem));
                        }
                        VarStorage::Table {
                            row_types: vec![*elem],
                            rows: vec![row],
                            is_fixed: len.is_some(),
                        }
                    }
                    TypeSpec::Table { row_types, len } => {
                        let actual_len = match len {
                            Some(e) => self.eval_usize(e, &HashMap::new())?,
                            None => 0,
                        };
                        if input_mode.is_some() {
                            return Err("Direct Input on multidimensional table is not supported in v0.1".to_string());
                        }
                        let rows = row_types
                            .iter()
                            .map(|t| vec![Value::default_for(*t); actual_len])
                            .collect();
                        VarStorage::Table {
                            row_types: row_types.clone(),
                            rows,
                            is_fixed: len.is_some(),
                        }
                    }
                };
                self.env.insert(name.clone(), storage);
                Ok(Status::Executed)
            }
            Stmt::SliceAssign {
                target,
                row,
                len,
                value,
            } => {
                let row_idx = row.unwrap_or(0);
                let target_len = match len {
                    Some(e) => Some(self.eval_usize(e, &HashMap::new())?),
                    None => None,
                };

                let new_row_vals = match value {
                    AssignValue::Scalar(expr) => {
                        let (val, _) = self.eval_expr(expr, &HashMap::new())?;
                        let n = target_len.ok_or_else(|| {
                            "Broadcast assignment requires explicit length [...N]".to_string()
                        })?;
                        vec![val; n]
                    }
                    AssignValue::Slice {
                        source,
                        row: s_row,
                        len: s_len,
                    } => {
                        let src_row_idx = s_row.unwrap_or(0);
                        let src_table = self
                            .env
                            .get(source)
                            .ok_or_else(|| format!("Undefined variable: {}", source))?;
                        let src_vec = match src_table {
                            VarStorage::Table { rows, .. } => rows
                                .get(src_row_idx)
                                .ok_or_else(|| format!("Row {} out of bounds in {}", src_row_idx, source))?,
                            VarStorage::Scalar(_) => {
                                return Err(format!("Cannot slice scalar {}", source))
                            }
                        };
                        let copy_len = match s_len {
                            Some(e) => self.eval_usize(e, &HashMap::new())?,
                            None => src_vec.len(),
                        };
                        if copy_len > src_vec.len() {
                            return Err(format!("Source slice length {} exceeds {}", copy_len, src_vec.len()));
                        }
                        src_vec[..copy_len].to_vec()
                    }
                };

                let dst = self
                    .env
                    .get_mut(target)
                    .ok_or_else(|| format!("Undefined variable: {}", target))?;
                match dst {
                    VarStorage::Table {
                        row_types,
                        rows,
                        is_fixed,
                    } => {
                        let r = rows
                            .get_mut(row_idx)
                            .ok_or_else(|| format!("Row {} out of bounds in {}", row_idx, target))?;
                        if !*is_fixed && r.len() < new_row_vals.len() {
                            // Extend all rows to match new length for variable-length tables
                            let new_len = new_row_vals.len();
                            for (i, row_vec) in rows.iter_mut().enumerate() {
                                if row_vec.len() < new_len {
                                    row_vec.resize(new_len, Value::default_for(row_types[i]));
                                }
                            }
                        }
                        let r = &mut rows[row_idx];
                        if new_row_vals.len() > r.len() {
                            return Err(format!(
                                "Assignment length {} exceeds target length {}",
                                new_row_vals.len(),
                                r.len()
                            ));
                        }
                        for (i, v) in new_row_vals.into_iter().enumerate() {
                            r[i] = v;
                        }
                    }
                    VarStorage::Scalar(_) => return Err(format!("Cannot slice assign to scalar {}", target)),
                }
                Ok(Status::Executed)
            }
            Stmt::Output { target, row, len } => {
                let target_len = match len {
                    Some(e) => Some(self.eval_usize(e, &HashMap::new())?),
                    None => None,
                };
                let storage = self
                    .env
                    .get(target)
                    .ok_or_else(|| format!("Undefined variable: {}", target))?;
                match storage {
                    VarStorage::Scalar(v) => {
                        writeln!(out, "{}", v.display_str()).map_err(|e| e.to_string())?;
                    }
                    VarStorage::Table { rows, .. } => {
                        if let Some(r_idx) = row {
                            let r = rows
                                .get(*r_idx)
                                .ok_or_else(|| format!("Row {} out of bounds in {}", r_idx, target))?;
                            let n = target_len.unwrap_or(r.len()).min(r.len());
                            let line = r[..n]
                                .iter()
                                .map(|v| v.display_str())
                                .collect::<Vec<_>>()
                                .join(" ");
                            writeln!(out, "{}", line).map_err(|e| e.to_string())?;
                        } else {
                            for r in rows {
                                let n = target_len.unwrap_or(r.len()).min(r.len());
                                let line = r[..n]
                                    .iter()
                                    .map(|v| v.display_str())
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                writeln!(out, "{}", line).map_err(|e| e.to_string())?;
                            }
                        }
                    }
                }
                Ok(Status::Executed)
            }
            Stmt::While { cond, step, body } => self.exec_while(cond, step, body),
        }
    }

    fn exec_while(
        &mut self,
        cond: &WhileCond,
        step: &[Extraction],
        body: &[RewriteRule],
    ) -> Result<Status, String> {
        let mut any_executed_overall = Status::NoMatch;
        let max_iters = 100_000;
        let mut iter = 0;

        loop {
            iter += 1;
            if iter > max_iters {
                return Err("While loop exceeded maximum iteration limit (100,000)".to_string());
            }

            let extractions: Vec<&Extraction> = match cond {
                WhileCond::Extract(ext) => vec![ext],
                _ => step.iter().collect(),
            };

            if let WhileCond::BoolExpr(expr) = cond {
                let (val, _) = self.eval_expr(expr, &HashMap::new())?;
                if val != Value::Bool(true) {
                    break;
                }
            }

            if extractions.is_empty() {
                break;
            }

            // Execute pass over the primary extraction
            let ext = extractions[0];
            let (any_exec, all_exec) = self.exec_extraction_pass(ext, body)?;

            if any_exec {
                any_executed_overall = Status::Executed;
            }

            let should_continue = match cond {
                WhileCond::Extract(_) | WhileCond::OrIn => any_exec,
                WhileCond::AndIn => all_exec,
                WhileCond::BoolExpr(_) => true,
            };

            if !should_continue {
                break;
            }
        }

        Ok(any_executed_overall)
    }

    fn exec_extraction_pass(
        &mut self,
        ext: &Extraction,
        rules: &[RewriteRule],
    ) -> Result<(bool, bool), String> {
        let window_width = ext
            .patterns
            .first()
            .map(|r| r.len())
            .ok_or_else(|| "Empty extraction pattern".to_string())?;

        // Track which row index each extracted variable name belongs to
        let mut var_row_map: HashMap<String, usize> = HashMap::new();
        for (r_idx, row_pat) in ext.patterns.iter().enumerate() {
            for elem in row_pat {
                if let PatternElem::Var(name) = elem {
                    var_row_map.insert(name.clone(), r_idx);
                }
            }
        }

        let mut executed_any = false;
        let mut rule_executed = vec![false; rules.len()];
        let mut idx = 0usize;

        loop {
            let (col_len, is_fixed, row_count) = match self.env.get(&ext.target) {
                Some(VarStorage::Table { rows, is_fixed, .. }) => {
                    (rows.first().map_or(0, |r| r.len()), *is_fixed, rows.len())
                }
                _ => return Err(format!("Target '{}' is not a table/array", ext.target)),
            };

            if ext.patterns.len() > row_count {
                return Err(format!(
                    "Pattern has {} rows, but '{}' has {} rows",
                    ext.patterns.len(),
                    ext.target,
                    row_count
                ));
            }

            let positions: Vec<usize> = match ext.mode {
                ExtractMode::Continuous => {
                    if idx + window_width <= col_len {
                        vec![idx]
                    } else {
                        break;
                    }
                }
                ExtractMode::Head => {
                    if idx == 0 && window_width <= col_len {
                        vec![0]
                    } else {
                        break;
                    }
                }
                ExtractMode::Tail => {
                    if idx == 0 && window_width <= col_len {
                        vec![col_len - window_width]
                    } else {
                        break;
                    }
                }
                ExtractMode::Single | ExtractMode::Any => {
                    if idx + window_width <= col_len {
                        vec![idx]
                    } else {
                        break;
                    }
                }
            };

            let pos = positions[0];
            let mut locals = HashMap::new();
            let mut matched_ext = true;

            {
                let table = match self.env.get(&ext.target) {
                    Some(VarStorage::Table { rows, .. }) => rows,
                    _ => unreachable!(),
                };
                'outer: for (r_idx, pat_row) in ext.patterns.iter().enumerate() {
                    for (c_off, pat_elem) in pat_row.iter().enumerate() {
                        let actual = &table[r_idx][pos + c_off];
                        match pat_elem {
                            PatternElem::Var(vname) => {
                                if let Some(prev) = locals.get(vname) {
                                    if prev != actual {
                                        matched_ext = false;
                                        break 'outer;
                                    }
                                } else {
                                    locals.insert(vname.clone(), actual.clone());
                                }
                            }
                            PatternElem::Lit(lit_expr) => {
                                let (lit_val, _) = self.eval_expr(lit_expr, &locals)?;
                                if &lit_val != actual {
                                    matched_ext = false;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }

            let mut resized = false;
            if matched_ext {
                for (rule_idx, rule) in rules.iter().enumerate() {
                    // Determine which rows this rule targets
                    let target_rows = self.resolve_rule_rows(rule, &var_row_map, ext.patterns.len())?;

                    // Check rule.lhs against current table slice
                    let mut rule_locals = locals.clone();
                    let mut lhs_matched = true;
                    {
                        let table = match self.env.get(&ext.target) {
                            Some(VarStorage::Table { rows, .. }) => rows,
                            _ => unreachable!(),
                        };
                        'lhs_check: for (pat_r, &actual_r) in target_rows.iter().enumerate() {
                            let pat_row = &rule.lhs[pat_r];
                            for (c_off, pat_elem) in pat_row.iter().enumerate() {
                                if pos + c_off >= table[actual_r].len() {
                                    lhs_matched = false;
                                    break 'lhs_check;
                                }
                                let actual = &table[actual_r][pos + c_off];
                                match pat_elem {
                                    PatternElem::Var(vname) => {
                                        if let Some(prev) = rule_locals.get(vname) {
                                            if prev != actual {
                                                lhs_matched = false;
                                                break 'lhs_check;
                                            }
                                        } else {
                                            rule_locals.insert(vname.clone(), actual.clone());
                                        }
                                    }
                                    PatternElem::Lit(lit_expr) => {
                                        let (lit_val, _) = self.eval_expr(lit_expr, &rule_locals)?;
                                        if &lit_val != actual {
                                            lhs_matched = false;
                                            break 'lhs_check;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !lhs_matched {
                        continue;
                    }

                    // Check guard
                    let guard_ok = match &rule.guard {
                        Some(g) => {
                            let (val, _) = self.eval_expr(g, &rule_locals)?;
                            val == Value::Bool(true)
                        }
                        None => true,
                    };

                    if !guard_ok {
                        continue;
                    }

                    // Evaluate RHS
                    if rule.rhs.len() != target_rows.len() {
                        return Err("RHS row count does not match LHS row count".to_string());
                    }
                    let mut new_slices: Vec<Vec<Value>> = Vec::new();
                    for rhs_row in &rule.rhs {
                        let mut evaluated_row = Vec::new();
                        for expr in rhs_row {
                            let (v, _) = self.eval_expr(expr, &rule_locals)?;
                            evaluated_row.push(v);
                        }
                        new_slices.push(evaluated_row);
                    }

                    let lhs_width = rule.lhs[0].len();
                    let rhs_width = new_slices[0].len();
                    if lhs_width != rhs_width && is_fixed {
                        return Err("Cannot resize fixed-length array during rewrite".to_string());
                    }

                    // Apply rewrite (skip as no-op if values are already identical)
                    let mut changed = lhs_width != rhs_width;
                    if let Some(VarStorage::Table { rows, .. }) = self.env.get_mut(&ext.target) {
                        for (i, &actual_r) in target_rows.iter().enumerate() {
                            let replacement = new_slices[i].clone();
                            if !changed && rows[actual_r][pos..(pos + lhs_width)] != replacement[..] {
                                changed = true;
                            }
                            rows[actual_r].splice(pos..(pos + lhs_width), replacement);
                        }
                    }

                    if !changed {
                        continue;
                    }
                    executed_any = true;
                    rule_executed[rule_idx] = true;
                    if lhs_width != rhs_width {
                        resized = true;
                    }
                    break;
                }
            }

            if matches!(ext.mode, ExtractMode::Head | ExtractMode::Tail) {
                break;
            }
            if !resized {
                idx += 1;
            }
        }

        let all_executed = !rules.is_empty() && rule_executed.iter().all(|&b| b);
        Ok((executed_any, all_executed))
    }

    fn resolve_rule_rows(
        &self,
        rule: &RewriteRule,
        var_row_map: &HashMap<String, usize>,
        total_ext_rows: usize,
    ) -> Result<Vec<usize>, String> {
        if rule.lhs.len() == total_ext_rows {
            return Ok((0..total_ext_rows).collect());
        }
        let mut target_rows = Vec::with_capacity(rule.lhs.len());
        for pat_row in &rule.lhs {
            let mut found_row = None;
            for elem in pat_row {
                if let PatternElem::Var(vname) = elem {
                    if let Some(&r_idx) = var_row_map.get(vname) {
                        found_row = Some(r_idx);
                        break;
                    }
                }
            }
            target_rows.push(found_row.unwrap_or(0));
        }
        Ok(target_rows)
    }

    fn eval_expr(
        &self,
        expr: &Expr,
        locals: &HashMap<String, Value>,
    ) -> Result<(Value, Status), String> {
        let val = match expr {
            Expr::Int(v) => Value::Int(*v),
            Expr::Float(v) => Value::Float(*v),
            Expr::Char(v) => Value::Char(*v),
            Expr::Str(v) => Value::String(v.clone()),
            Expr::Bool(v) => Value::Bool(*v),
            Expr::Ident(name) => {
                if let Some(v) = locals.get(name) {
                    v.clone()
                } else if let Some(VarStorage::Scalar(v)) = self.env.get(name) {
                    v.clone()
                } else {
                    return Err(format!("Undefined variable in expression: {}", name));
                }
            }
            Expr::Unary { op, expr } => {
                let (v, _) = self.eval_expr(expr, locals)?;
                match (op, v) {
                    (UnaryOp::Neg, Value::Int(x)) => Value::Int(-x),
                    (UnaryOp::Neg, Value::Float(x)) => Value::Float(-x),
                    (UnaryOp::Not, Value::Bool(b)) => Value::Bool(!b),
                    _ => return Err("Invalid unary operation".to_string()),
                }
            }
            Expr::Binary { op, left, right } => {
                let (l, _) = self.eval_expr(left, locals)?;
                let (r, _) = self.eval_expr(right, locals)?;
                eval_binary(*op, l, r)?
            }
        };
        Ok((val, Status::Executed))
    }
}

fn eval_binary(op: BinaryOp, l: Value, r: Value) -> Result<Value, String> {
    match (op, l, r) {
        (BinaryOp::Add, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
        (BinaryOp::Sub, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
        (BinaryOp::Mul, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
        (BinaryOp::Div, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
        (BinaryOp::Mod, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),

        (BinaryOp::Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (BinaryOp::Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (BinaryOp::Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (BinaryOp::Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),

        (BinaryOp::Eq, a, b) => Ok(Value::Bool(a == b)),
        (BinaryOp::Neq, a, b) => Ok(Value::Bool(a != b)),

        (BinaryOp::Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        (BinaryOp::Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        (BinaryOp::Le, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (BinaryOp::Ge, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),

        (BinaryOp::Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
        (BinaryOp::Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
        (BinaryOp::Le, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
        (BinaryOp::Ge, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),

        (BinaryOp::And, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a && b)),
        (BinaryOp::Or, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a || b)),

        (op, l, r) => Err(format!("Type mismatch for {:?}: {:?} and {:?}", op, l, r)),
    }
}

pub fn run_source(source: &str, input: &str) -> Result<String, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program()?;
    let mut interp = Interpreter::new(input);
    let mut out = Vec::new();
    interp.run(&stmts, &mut out)?;
    String::from_utf8(out).map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        let source = fs::read_to_string(&args[1]).expect("Failed to read source file");
        let run = || -> Result<(), String> {
            let mut lexer = Lexer::new(&source);
            let tokens = lexer.tokenize()?;
            let mut parser = Parser::new(tokens);
            let stmts = parser.parse_program()?;
            let mut interp = Interpreter::from_stdin();
            let mut stdout = io::stdout();
            interp.run(&stmts, &mut stdout)
        };
        if let Err(e) = run() {
            eprintln!("Runtime error: {}", e);
            std::process::exit(1);
        }
    } else {
        println!("Usage: {} <source_file>", args[0]);
    }
}

// ==========================================
// 5. v0.1 検証用テストスイート
// ==========================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_1_shortest_distance() {
        let src = r#"
N: Int(Input)
C: Int[...N](Input)
Cd: [Int, Int][...N]
Cd[0][...N] = 0
Cd[1][...N] = C[...N]

while([[a1, a2], [c1, c2]] = Cd(continuous)) {
  [a1, a2](c1 != c2) = [1, 1]
}

while([[a1, a2], [c1, c2]] = Cd(continuous)) {
  [a1, a2](a1 != 0 & a2 == 0) = [a1, a1 + 1]
  [a1, a2](a1 == 0 & a2 != 0) = [a2 + 1, a2]
}

Cd[0][...N](Output)
"#;
        let out = run_source(src, "6 1 1 2 1 1 1").unwrap();
        assert_eq!(out.trim(), "2 1 1 1 2 3");
    }

    #[test]
    fn test_2_run_length_encoding() {
        let src = r#"
N: Int(Input)
S: Char[...N](Input)
R: [Char, Int][...]
R[0][...N] = S[...N]
R[1][...N] = 1

while([[c1, c2], [n1, n2]] = R(continuous)) {
  [[c1, c2], [n1, n2]](c1 == c2) = [[c1], [n1 + n2]]
}

R(Output)
"#;
        let out = run_source(src, "7 a a a b b c a").unwrap();
        assert_eq!(out.trim(), "a b c a\n3 2 1 1");
    }

    #[test]
    fn test_3_bubble_sort_or_in() {
        let src = r#"
N: Int(Input)
A: Int[...N](Input)
while(OR(In),[a1,a2]=A(continous)){
    [a1,a2](a1>a2)=[a2,a1]
}A(Output)
"#;
        let out = run_source(src, "5 5 3 1 4 2").unwrap();
        assert_eq!(out.trim(), "1 2 3 4 5");
    }

    #[test]
    fn test_4_input_modes() {
        let src_line = r#"
N: Int(Input)
L: String[...N](Input(line))
L(Output)
"#;
        let out_line = run_source(src_line, "2
hello world
foo bar
").unwrap();
        assert_eq!(out_line.trim(), "hello world
foo bar");

        let src_char = r#"
N: Int(Input)
C: Char[...N](Input(char))
C(Output)
"#;
        let out_char = run_source(src_char, "5
a bcd
").unwrap();
        assert_eq!(out_char.trim(), "a   b c d");

        let src_all = r#"
C: Char[...](Input(all))
C(Output)
"#;
        let out_all = run_source(src_all, "hi!").unwrap();
        assert_eq!(out_all.trim(), "h i !");
    }
}