use crate::ast::*;
use crate::token::Token;

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

    fn skip_semicolons(&mut self) {
        while *self.peek() == Token::Semicolon {
            self.advance();
        }
    }

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        self.skip_semicolons();
        while *self.peek() != Token::Eof {
            stmts.push(self.parse_stmt()?);
            self.skip_semicolons();
        }
        Ok(stmts)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect(Token::LBrace)?;
        let mut stmts = Vec::new();
        self.skip_semicolons();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            stmts.push(self.parse_stmt()?);
            self.skip_semicolons();
        }
        self.expect(Token::RBrace)?;
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek() {
            Token::Fn => Ok(Stmt::FnDecl(self.parse_fn_decl(false)?)),
            Token::Class => Ok(Stmt::ClassDecl(self.parse_class_decl()?)),
            Token::If => self.parse_if(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            Token::Continue => {
                self.advance();
                Ok(Stmt::Continue)
            }
            Token::Return => self.parse_return(),
            Token::LBracket => self.parse_multi_assign(),
            Token::Ident(name) if name == "print" && *self.peek_n(1) == Token::LParen => {
                self.parse_print()
            }
            Token::Ident(name) if name == "input" && *self.peek_n(1) == Token::LParen => {
                if *self.peek_n(2) == Token::RParen {
                    self.advance(); // input
                    self.advance(); // (
                    self.advance(); // )
                    Ok(Stmt::FnInput {
                        targets: Vec::new(),
                    })
                } else {
                    self.parse_fn_input()
                }
            }
            Token::Ident(_) => {
                if self.is_var_decl_start() {
                    self.parse_var_decl()
                } else {
                    self.parse_expr_or_assign_stmt()
                }
            }
            Token::SelfKw => self.parse_expr_or_assign_stmt(),
            other => Err(format!(
                "Unexpected token at start of statement: {:?}",
                other
            )),
        }
    }

    fn expr_to_assign_target(&self, expr: Expr) -> Result<AssignTarget, String> {
        match expr {
            Expr::Ident(name) => Ok(AssignTarget::Ident(name)),
            Expr::FieldAccess { target, field } => Ok(AssignTarget::Field {
                target: *target,
                field,
            }),
            Expr::Index { target, index } => Ok(AssignTarget::Index {
                target: *target,
                index: *index,
            }),
            Expr::SliceView { target, start, end } => Ok(AssignTarget::Slice {
                target: *target,
                start: start.map(|b| *b),
                end: end.map(|b| *b),
            }),
            other => Err(format!("Invalid assignment target: {:?}", other)),
        }
    }

    fn parse_expr_or_assign_stmt(&mut self) -> Result<Stmt, String> {
        let lhs = self.parse_expr()?;
        if *self.peek() == Token::Assign {
            self.advance();
            let value = self.parse_expr()?;
            let target = self.expr_to_assign_target(lhs)?;
            Ok(Stmt::Assign { target, value })
        } else {
            Ok(Stmt::ExprStmt(lhs))
        }
    }

    fn parse_multi_assign(&mut self) -> Result<Stmt, String> {
        self.expect(Token::LBracket)?;
        let mut targets = Vec::new();
        while *self.peek() != Token::RBracket && *self.peek() != Token::Eof {
            let expr = self.parse_expr()?;
            targets.push(self.expr_to_assign_target(expr)?);
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::RBracket)?;
        self.expect(Token::Assign)?;
        self.expect(Token::LBracket)?;
        let mut values = Vec::new();
        while *self.peek() != Token::RBracket && *self.peek() != Token::Eof {
            values.push(self.parse_expr()?);
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::RBracket)?;
        Ok(Stmt::MultiAssign { targets, values })
    }

    fn parse_if(&mut self) -> Result<Stmt, String> {
        self.expect(Token::If)?;
        let has_paren = *self.peek() == Token::LParen;
        if has_paren {
            self.advance();
        }
        let cond = self.parse_expr()?;
        if has_paren {
            self.expect(Token::RParen)?;
        }
        let then_branch = self.parse_block()?;

        let else_branch = if *self.peek() == Token::Else {
            self.advance();
            if *self.peek() == Token::If {
                Some(vec![self.parse_if()?])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        Ok(Stmt::If {
            cond,
            then_branch,
            else_branch,
        })
    }

    fn parse_for(&mut self) -> Result<Stmt, String> {
        self.expect(Token::For)?;
        let var_name = self.expect_ident()?;
        let var_ty = if *self.peek() == Token::Colon {
            self.advance();
            Some(self.parse_type_ref()?)
        } else {
            None
        };
        self.expect(Token::In)?;
        self.expect(Token::LBracket)?;
        let range_start = self.parse_expr()?;
        self.expect(Token::DotDot)?;
        let range_end = self.parse_expr()?;
        self.expect(Token::RBracket)?;

        let body = self.parse_block()?;

        Ok(Stmt::For {
            var_name,
            var_ty,
            range_start,
            range_end,
            body,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        self.expect(Token::While)?;
        let has_paren = *self.peek() == Token::LParen;
        if has_paren {
            self.advance();
        }
        let cond = self.parse_expr()?;
        if has_paren {
            self.expect(Token::RParen)?;
        }
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body })
    }

    fn parse_return(&mut self) -> Result<Stmt, String> {
        self.expect(Token::Return)?;
        let no_expr = matches!(
            self.peek(),
            Token::RBrace
                | Token::Semicolon
                | Token::Eof
                | Token::If
                | Token::Else
                | Token::For
                | Token::While
                | Token::Break
                | Token::Continue
                | Token::Fn
                | Token::Class
                | Token::Return
        ) || (matches!(self.peek(), Token::Ident(_))
            && matches!(self.peek_n(1), Token::Assign | Token::Colon));

        if no_expr {
            Ok(Stmt::Return(None))
        } else {
            Ok(Stmt::Return(Some(self.parse_expr()?)))
        }
    }

    fn parse_fn_decl(&mut self, is_method: bool) -> Result<FnDecl, String> {
        self.expect(Token::Fn)?;
        let name = self.expect_ident()?;
        self.expect(Token::LParen)?;
        let mut params = Vec::new();
        while *self.peek() != Token::RParen && *self.peek() != Token::Eof {
            if is_method && params.is_empty() && *self.peek() == Token::SelfKw {
                self.advance();
                let ty = if *self.peek() == Token::Colon {
                    self.advance();
                    Some(self.parse_type_spec()?)
                } else {
                    None
                };
                params.push(Param {
                    name: "self".to_string(),
                    ty,
                });
            } else {
                let pname = self.expect_ident()?;
                self.expect(Token::Colon)?;
                let pty = self.parse_type_spec()?;
                params.push(Param {
                    name: pname,
                    ty: Some(pty),
                });
            }
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::RParen)?;

        let return_type = if *self.peek() == Token::Arrow {
            self.advance();
            Some(self.parse_type_spec()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        Ok(FnDecl {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_class_decl(&mut self) -> Result<ClassDecl, String> {
        self.expect(Token::Class)?;
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;

        let mut fields = Vec::new();
        let mut methods = Vec::new();

        self.skip_semicolons();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            if *self.peek() == Token::Fn {
                methods.push(self.parse_fn_decl(true)?);
            } else {
                let mut field_names = Vec::new();
                loop {
                    field_names.push(self.expect_ident()?);
                    if *self.peek() == Token::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let ty = if *self.peek() == Token::Colon {
                    self.advance();
                    Some(self.parse_type_spec()?)
                } else {
                    None
                };
                let default_expr = if *self.peek() == Token::Assign {
                    self.advance();
                    Some(self.parse_expr()?)
                } else {
                    None
                };
                if ty.is_none() && default_expr.is_none() {
                    return Err(format!(
                        "Field '{}' in class '{}' requires either ': <type>' or '= <expr>'",
                        field_names[0], name
                    ));
                }
                for fname in field_names {
                    fields.push(FieldDecl {
                        name: fname,
                        ty: ty.clone(),
                        default_expr: default_expr.clone(),
                    });
                }
            }
            self.skip_semicolons();
        }
        self.expect(Token::RBrace)?;

        Ok(ClassDecl {
            name,
            fields,
            methods,
        })
    }

    fn parse_print(&mut self) -> Result<Stmt, String> {
        self.advance(); // "print"
        self.expect(Token::LParen)?;
        let mut args = Vec::new();
        while *self.peek() != Token::RParen && *self.peek() != Token::Eof {
            args.push(self.parse_expr()?);
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::RParen)?;
        Ok(Stmt::Print { args })
    }

    fn is_var_decl_start(&self) -> bool {
        let mut offset = 0;
        while let Token::Ident(_) = self.peek_n(offset) {
            offset += 1;
            if *self.peek_n(offset) == Token::Comma {
                offset += 1;
            } else {
                return *self.peek_n(offset) == Token::Colon;
            }
        }
        false
    }

    fn parse_type_ref(&mut self) -> Result<TypeRef, String> {
        let name = self.expect_ident()?;
        match name.as_str() {
            "int" => Ok(TypeRef::Int),
            "uint" => Ok(TypeRef::Uint),
            "float" => Ok(TypeRef::Float),
            "char" => Ok(TypeRef::Char),
            "string" => Ok(TypeRef::String),
            "bool" => Ok(TypeRef::Bool),
            "void" => Ok(TypeRef::Void),
            _ => Ok(TypeRef::Custom(name)),
        }
    }

    fn parse_array_len(&mut self) -> Result<Option<Expr>, String> {
        self.expect(Token::LBracket)?;
        if *self.peek() == Token::RBracket {
            self.advance();
            Ok(None)
        } else {
            let len = self.parse_expr()?;
            self.expect(Token::RBracket)?;
            Ok(Some(len))
        }
    }

    fn parse_type_spec(&mut self) -> Result<TypeSpec, String> {
        if *self.peek() == Token::LBracket {
            self.advance();
            let mut row_types = Vec::new();
            loop {
                row_types.push(self.parse_type_ref()?);
                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RBracket)?;
            let len = self.parse_array_len()?;
            Ok(TypeSpec::Table { row_types, len })
        } else {
            let prim = self.parse_type_ref()?;
            if *self.peek() == Token::LBracket {
                let len = self.parse_array_len()?;
                Ok(TypeSpec::Array1D { elem: prim, len })
            } else {
                Ok(TypeSpec::Scalar(prim))
            }
        }
    }

    fn parse_var_decl(&mut self) -> Result<Stmt, String> {
        let mut names = Vec::new();
        loop {
            names.push(self.expect_ident()?);
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::Colon)?;
        let ty = self.parse_type_spec()?;

        let mut init = None;
        if *self.peek() == Token::Assign {
            self.advance();
            init = Some(self.parse_expr()?);
        }

        Ok(Stmt::VarDecl { names, ty, init })
    }

    fn parse_fn_input(&mut self) -> Result<Stmt, String> {
        self.expect_ident()?; // "input"
        self.expect(Token::LParen)?;
        let mut targets = Vec::new();
        while *self.peek() != Token::RParen && *self.peek() != Token::Eof {
            if self.is_var_decl_start() {
                let mut names = Vec::new();
                loop {
                    names.push(self.expect_ident()?);
                    if *self.peek() == Token::Comma {
                        if let Token::Ident(_) = self.peek_n(1) {
                            if matches!(self.peek_n(2), Token::Comma | Token::Colon) {
                                self.advance();
                                continue;
                            }
                        }
                        break;
                    } else {
                        break;
                    }
                }
                self.expect(Token::Colon)?;
                let ty = self.parse_type_spec()?;
                targets.push(InputTarget::Decl { names, ty });
            } else {
                let expr = self.parse_expr()?;
                targets.push(InputTarget::Var { target: expr });
            }

            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::RParen)?;
        Ok(Stmt::FnInput { targets })
    }

    fn parse_arg_list(&mut self) -> Result<Vec<Expr>, String> {
        self.expect(Token::LParen)?;
        let mut args = Vec::new();
        while *self.peek() != Token::RParen && *self.peek() != Token::Eof {
            args.push(self.parse_expr()?);
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::RParen)?;
        Ok(args)
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
        let base = match self.peek() {
            Token::Int(v) => {
                let val = *v;
                self.advance();
                Expr::Int(val)
            }
            Token::Float(v) => {
                let val = *v;
                self.advance();
                Expr::Float(val)
            }
            Token::Char(v) => {
                let val = *v;
                self.advance();
                Expr::Char(val)
            }
            Token::Str(v) => {
                let val = v.clone();
                self.advance();
                Expr::Str(val)
            }
            Token::Bool(v) => {
                let val = *v;
                self.advance();
                Expr::Bool(val)
            }
            Token::SelfKw => {
                self.advance();
                Expr::SelfRef
            }
            Token::Ident(s)
                if s == "input"
                    && *self.peek_n(1) == Token::LParen
                    && *self.peek_n(2) == Token::RParen =>
            {
                self.advance(); // "input"
                self.advance(); // '('
                self.advance(); // ')'
                Expr::InputCall
            }
            Token::Ident(s) if *self.peek_n(1) == Token::LParen => {
                let callee = s.clone();
                self.advance();
                let args = self.parse_arg_list()?;
                Expr::Call { callee, args }
            }
            Token::Ident(s) => {
                let name = s.clone();
                self.advance();
                Expr::Ident(name)
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                expr
            }
            other => return Err(format!("Unexpected token in expression: {:?}", other)),
        };

        let mut expr = base;
        loop {
            if *self.peek() == Token::LBracket {
                self.advance(); // '['
                let start = if *self.peek() == Token::DotDot {
                    None
                } else {
                    Some(Box::new(self.parse_expr()?))
                };
                if *self.peek() == Token::DotDot {
                    self.advance(); // '..'
                    let end = if *self.peek() == Token::RBracket {
                        None
                    } else {
                        Some(Box::new(self.parse_expr()?))
                    };
                    self.expect(Token::RBracket)?;
                    expr = Expr::SliceView {
                        target: Box::new(expr),
                        start,
                        end,
                    };
                } else {
                    self.expect(Token::RBracket)?;
                    let index = start.ok_or_else(|| "Expected index expression".to_string())?;
                    expr = Expr::Index {
                        target: Box::new(expr),
                        index,
                    };
                }
            } else if *self.peek() == Token::Dot {
                self.advance(); // '.'
                let member = self.expect_ident()?;
                if *self.peek() == Token::LParen {
                    let args = self.parse_arg_list()?;
                    expr = Expr::MethodCall {
                        target: Box::new(expr),
                        method: member,
                        args,
                    };
                } else {
                    expr = Expr::FieldAccess {
                        target: Box::new(expr),
                        field: member,
                    };
                }
            } else {
                break;
            }
        }
        Ok(expr)
    }
}
