use std::path::Path;

use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::token::{SpannedToken, Token};

pub struct Parser {
    tokens: Vec<SpannedToken>,
    pos: usize,
    eof_span: Span,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        let eof_span = tokens.last().map(|t| t.span).unwrap_or_default();
        Self {
            tokens,
            pos: 0,
            eof_span,
        }
    }

    fn peek_spanned(&self) -> Option<&SpannedToken> {
        self.tokens.get(self.pos)
    }

    fn peek(&self) -> &Token {
        self.peek_spanned().map(|t| &t.token).unwrap_or(&Token::Eof)
    }

    fn peek_span(&self) -> Span {
        self.peek_spanned().map(|t| t.span).unwrap_or(self.eof_span)
    }

    fn peek_n(&self, n: usize) -> &Token {
        self.tokens
            .get(self.pos + n)
            .map(|t| &t.token)
            .unwrap_or(&Token::Eof)
    }

    fn advance_spanned(&mut self) -> SpannedToken {
        if let Some(t) = self.tokens.get(self.pos).cloned() {
            self.pos += 1;
            t
        } else {
            SpannedToken {
                token: Token::Eof,
                span: self.eof_span,
            }
        }
    }

    fn advance(&mut self) -> Token {
        self.advance_spanned().token
    }

    fn err(&self, span: Span, msg: impl Into<String>) -> Diagnostic {
        Diagnostic::new("parse", span, msg)
    }

    fn expect(&mut self, expected: Token) -> Result<Span, Diagnostic> {
        let got = self.advance_spanned();
        if got.token == expected {
            Ok(got.span)
        } else {
            Err(self.err(
                got.span,
                format!("Expected {:?}, got {:?}", expected, got.token),
            ))
        }
    }

    fn expect_ident(&mut self) -> Result<(String, Span), Diagnostic> {
        let got = self.advance_spanned();
        match got.token {
            Token::Ident(s) => Ok((s, got.span)),
            other => Err(self.err(
                got.span,
                format!("Expected identifier, got {:?}", other),
            )),
        }
    }

    fn skip_semicolons(&mut self) {
        while *self.peek() == Token::Semicolon {
            self.advance();
        }
    }

    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
        let mut stmts = Vec::new();
        self.skip_semicolons();
        while *self.peek() != Token::Eof {
            stmts.push(self.parse_stmt()?);
            self.skip_semicolons();
        }
        Ok(stmts)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, Diagnostic> {
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

    fn parse_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.peek_span();
        match self.peek() {
            Token::Import => self.parse_import(),
            Token::Fn => Ok(Stmt::new(
                StmtKind::FnDecl(self.parse_fn_decl(false)?),
                span,
            )),
            Token::Class => Ok(Stmt::new(
                StmtKind::ClassDecl(self.parse_class_decl()?),
                span,
            )),
            Token::If => self.parse_if(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Break => {
                self.advance();
                Ok(Stmt::new(StmtKind::Break, span))
            }
            Token::Continue => {
                self.advance();
                Ok(Stmt::new(StmtKind::Continue, span))
            }
            Token::Return => self.parse_return(),
            Token::Ident(name) if name == "print" && *self.peek_n(1) == Token::LParen => {
                self.parse_print()
            }
            Token::Ident(name) if name == "input" && *self.peek_n(1) == Token::LParen => {
                if *self.peek_n(2) == Token::RParen {
                    self.advance(); // input
                    self.advance(); // (
                    self.advance(); // )
                    Ok(Stmt::new(
                        StmtKind::FnInput {
                            targets: Vec::new(),
                        },
                        span,
                    ))
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
            Token::SelfKw | Token::LBracket | Token::LParen | Token::Amp => {
                self.parse_expr_or_assign_stmt()
            }
            other => Err(self.err(
                span,
                format!("Unexpected token at start of statement: {:?}", other),
            )),
        }
    }

    fn parse_import(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(Token::Import)?;
        let tok = self.advance_spanned();
        let (path, is_file_path, default_alias) = match tok.token {
            Token::Str(p) => {
                if p.is_empty() {
                    return Err(self.err(tok.span, "Import file path cannot be empty"));
                }
                let stem = Path::new(&p)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                (p, true, stem)
            }
            Token::Ident(name) => (name.clone(), false, name),
            other => {
                return Err(self
                    .err(
                        tok.span,
                        format!(
                            "Expected module name or string path after 'import', got {:?}",
                            other
                        ),
                    )
                    .with_hint("Use 'import \"file.pol\"' or 'import mod_name'"));
            }
        };

        let alias = if *self.peek() == Token::As {
            self.advance();
            let (a, _) = self.expect_ident()?;
            a
        } else {
            if !is_valid_ident(&default_alias) {
                return Err(self
                    .err(
                        tok.span,
                        format!(
                            "Cannot derive a valid module identifier from '{}'",
                            path
                        ),
                    )
                    .with_hint("Specify an explicit alias: import \"...\" as my_mod"));
            }
            default_alias
        };

        Ok(Stmt::new(
            StmtKind::Import {
                path,
                is_file_path,
                alias,
            },
            span,
        ))
    }

    fn expr_to_assign_target(&self, expr: Expr) -> Result<AssignTarget, Diagnostic> {
        let span = expr.span;
        match expr.node {
            ExprKind::Ident(name) => Ok(AssignTarget::new(AssignTargetKind::Ident(name), span)),
            ExprKind::FieldAccess { target, field } => Ok(AssignTarget::new(
                AssignTargetKind::Field {
                    target: *target,
                    field,
                },
                span,
            )),
            ExprKind::Index { target, index } => Ok(AssignTarget::new(
                AssignTargetKind::Index {
                    target: *target,
                    index: *index,
                },
                span,
            )),
            ExprKind::SliceView { target, start, end } => Ok(AssignTarget::new(
                AssignTargetKind::Slice {
                    target: *target,
                    start: start.map(|b| *b),
                    end: end.map(|b| *b),
                },
                span,
            )),
            ExprKind::MultiIndex { target, selectors } => Ok(AssignTarget::new(
                AssignTargetKind::MultiIndex {
                    target: *target,
                    selectors,
                },
                span,
            )),
            _ => Err(self
                .err(span, "Invalid assignment target")
                .with_hint("You can assign to variables, fields, indexed elements, or slice ranges")),
        }
    }

    fn parse_expr_or_assign_stmt(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.peek_span();
        let lhs = self.parse_expr()?;
        if *self.peek() == Token::Assign {
            self.advance();
            let value = self.parse_expr()?;
            if let ExprKind::ArrayLiteral(elems) = lhs.node {
                let mut targets = Vec::with_capacity(elems.len());
                for elem in elems {
                    targets.push(self.expr_to_assign_target(elem)?);
                }
                let val_span = value.span;
                let values = match value.node {
                    ExprKind::ArrayLiteral(vs) => vs,
                    _ => {
                        return Err(self
                            .err(
                                val_span,
                                "Multi-assignment right-hand side must be '[v1, v2, ...]'",
                            )
                            .with_hint("Match destructuring form: [a, b] = [b, a]"));
                    }
                };
                Ok(Stmt::new(StmtKind::MultiAssign { targets, values }, span))
            } else {
                let target = self.expr_to_assign_target(lhs)?;
                Ok(Stmt::new(StmtKind::Assign { target, value }, span))
            }
        } else {
            Ok(Stmt::new(StmtKind::ExprStmt(lhs), span))
        }
    }

    fn parse_if(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(Token::If)?;
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

        Ok(Stmt::new(
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            },
            span,
        ))
    }

    fn parse_for(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(Token::For)?;
        let (var_name, _) = self.expect_ident()?;
        let var_ty = if *self.peek() == Token::Colon {
            self.advance();
            Some(self.parse_type_expr()?)
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

        Ok(Stmt::new(
            StmtKind::For {
                var_name,
                var_ty,
                range_start,
                range_end,
                body,
            },
            span,
        ))
    }

    fn parse_while(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(Token::While)?;
        let has_paren = *self.peek() == Token::LParen;
        if has_paren {
            self.advance();
        }
        let cond = self.parse_expr()?;
        if has_paren {
            self.expect(Token::RParen)?;
        }
        let body = self.parse_block()?;
        Ok(Stmt::new(StmtKind::While { cond, body }, span))
    }

    fn parse_return(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.expect(Token::Return)?;
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
                | Token::Import
        ) || (matches!(self.peek(), Token::Ident(_))
            && matches!(self.peek_n(1), Token::Assign | Token::Colon));

        if no_expr {
            Ok(Stmt::new(StmtKind::Return(None), span))
        } else {
            Ok(Stmt::new(StmtKind::Return(Some(self.parse_expr()?)), span))
        }
    }

    fn parse_type_params(&mut self) -> Result<Vec<String>, Diagnostic> {
        if *self.peek() != Token::Lt {
            return Ok(Vec::new());
        }
        let open_span = self.advance_spanned().span;
        let mut params = Vec::new();
        while *self.peek() != Token::Gt && *self.peek() != Token::Eof {
            let (tp, tp_span) = self.expect_ident()?;
            if params.contains(&tp) {
                return Err(self.err(
                    tp_span,
                    format!("Duplicate generic type parameter '{}'", tp),
                ));
            }
            params.push(tp);
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        if params.is_empty() {
            return Err(self.err(
                open_span,
                "Generic parameter list '<>' cannot be empty",
            ));
        }
        self.expect(Token::Gt)?;
        Ok(params)
    }

    fn parse_fn_decl(&mut self, is_method: bool) -> Result<FnDecl, Diagnostic> {
        let span = self.expect(Token::Fn)?;
        let (name, _) = self.expect_ident()?;
        let type_params = self.parse_type_params()?;
        self.expect(Token::LParen)?;
        let mut params = Vec::new();
        while *self.peek() != Token::RParen && *self.peek() != Token::Eof {
            let p_span = self.peek_span();
            if is_method && params.is_empty() && *self.peek() == Token::SelfKw {
                self.advance();
                let ty = if *self.peek() == Token::Colon {
                    self.advance();
                    Some(self.parse_type_expr()?)
                } else {
                    None
                };
                params.push(Param {
                    name: "self".to_string(),
                    ty,
                    span: p_span,
                });
            } else {
                let (pname, _) = self.expect_ident()?;
                self.expect(Token::Colon)?;
                let pty = self.parse_type_expr()?;
                params.push(Param {
                    name: pname,
                    ty: Some(pty),
                    span: p_span,
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
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        let body = self.parse_block()?;
        Ok(FnDecl {
            name,
            type_params,
            params,
            return_type,
            body,
            span,
        })
    }

    fn parse_class_decl(&mut self) -> Result<ClassDecl, Diagnostic> {
        let span = self.expect(Token::Class)?;
        let (name, _) = self.expect_ident()?;
        let type_params = self.parse_type_params()?;
        self.expect(Token::LBrace)?;

        let mut fields = Vec::new();
        let mut methods = Vec::new();

        self.skip_semicolons();
        while *self.peek() != Token::RBrace && *self.peek() != Token::Eof {
            if *self.peek() == Token::Fn {
                methods.push(self.parse_fn_decl(true)?);
            } else {
                let f_span = self.peek_span();
                let mut field_names = Vec::new();
                loop {
                    let (fname, _) = self.expect_ident()?;
                    field_names.push(fname);
                    if *self.peek() == Token::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let ty = if *self.peek() == Token::Colon {
                    self.advance();
                    Some(self.parse_type_expr()?)
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
                    return Err(self.err(
                        f_span,
                        format!(
                            "Field '{}' in class '{}' requires either ': <type>' or '= <expr>'",
                            field_names[0], name
                        ),
                    ));
                }
                for fname in field_names {
                    fields.push(FieldDecl {
                        name: fname,
                        ty: ty.clone(),
                        default_expr: default_expr.clone(),
                        span: f_span,
                    });
                }
            }
            self.skip_semicolons();
        }
        self.expect(Token::RBrace)?;

        Ok(ClassDecl {
            name,
            type_params,
            fields,
            methods,
            span,
        })
    }

    fn parse_print(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.peek_span();
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
        Ok(Stmt::new(StmtKind::Print { args }, span))
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

    fn parse_array_len(&mut self) -> Result<Option<Box<Expr>>, Diagnostic> {
        self.expect(Token::LBracket)?;
        if *self.peek() == Token::RBracket {
            self.advance();
            Ok(None)
        } else {
            let len = self.parse_expr()?;
            self.expect(Token::RBracket)?;
            Ok(Some(Box::new(len)))
        }
    }

    fn parse_type_expr(&mut self) -> Result<TypeExpr, Diagnostic> {
        let span = self.peek_span();
        if *self.peek() == Token::Amp {
            self.advance();
            let inner = self.parse_type_expr()?;
            return Ok(TypeExpr::new(TypeExprKind::Ref(Box::new(inner)), span));
        }

        if *self.peek() == Token::LBracket {
            self.advance();
            let mut row_types = Vec::new();
            while *self.peek() != Token::RBracket && *self.peek() != Token::Eof {
                row_types.push(self.parse_type_expr()?);
                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            if row_types.is_empty() {
                return Err(self.err(span, "Table type '[...]' cannot be empty"));
            }
            self.expect(Token::RBracket)?;
            let len = self.parse_array_len()?;
            return Ok(TypeExpr::new(TypeExprKind::Table { row_types, len }, span));
        }

        let (first, _) = self.expect_ident()?;
        let (module, name) = if *self.peek() == Token::Dot {
            self.advance();
            let (member, _) = self.expect_ident()?;
            (Some(first), member)
        } else {
            (None, first)
        };

        let mut type_args = Vec::new();
        if *self.peek() == Token::Lt {
            let open_span = self.advance_spanned().span;
            while *self.peek() != Token::Gt && *self.peek() != Token::Eof {
                type_args.push(self.parse_type_expr()?);
                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            if type_args.is_empty() {
                return Err(self.err(open_span, "Type argument list '<>' cannot be empty"));
            }
            self.expect(Token::Gt)?;
        }

        let mut base = TypeExpr::new(
            TypeExprKind::Named {
                module,
                name,
                type_args,
            },
            span,
        );

        let mut dims = Vec::new();
        while *self.peek() == Token::LBracket {
            dims.push(self.parse_array_len()?);
        }
        // Fold right so int[R][C] becomes Array { elem: Array { elem: int, len: C }, len: R }
        for dim in dims.into_iter().rev() {
            base = TypeExpr::new(
                TypeExprKind::Array {
                    elem: Box::new(base),
                    len: dim,
                },
                span,
            );
        }

        Ok(base)
    }

    fn parse_var_decl(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.peek_span();
        let mut names = Vec::new();
        loop {
            let (name, _) = self.expect_ident()?;
            names.push(name);
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(Token::Colon)?;
        let ty = self.parse_type_expr()?;

        let mut init = None;
        if *self.peek() == Token::Assign {
            self.advance();
            init = Some(self.parse_expr()?);
        }

        Ok(Stmt::new(StmtKind::VarDecl { names, ty, init }, span))
    }

    fn parse_fn_input(&mut self) -> Result<Stmt, Diagnostic> {
        let span = self.peek_span();
        self.expect_ident()?; // "input"
        self.expect(Token::LParen)?;
        let mut targets = Vec::new();
        while *self.peek() != Token::RParen && *self.peek() != Token::Eof {
            let t_span = self.peek_span();
            if self.is_var_decl_start() {
                let mut names = Vec::new();
                loop {
                    let (n, _) = self.expect_ident()?;
                    names.push(n);
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
                let ty = self.parse_type_expr()?;
                targets.push(InputTarget::Decl {
                    names,
                    ty,
                    span: t_span,
                });
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
        Ok(Stmt::new(StmtKind::FnInput { targets }, span))
    }

    fn parse_arg_list(&mut self) -> Result<Vec<Expr>, Diagnostic> {
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

    /// Disambiguates generic type arguments `<T1, T2>(...)` from `<` less-than comparison.
    /// Returns `Some(type_args)` only if `< ... >` parses cleanly as type arguments and is
    /// immediately followed by `(`.
    fn try_parse_expr_type_args(&mut self) -> Option<Vec<TypeExpr>> {
        if *self.peek() != Token::Lt {
            return None;
        }
        let saved_pos = self.pos;
        self.advance(); // '<'
        let mut args = Vec::new();
        while *self.peek() != Token::Gt && *self.peek() != Token::Eof {
            match self.parse_type_expr() {
                Ok(te) => args.push(te),
                Err(_) => {
                    self.pos = saved_pos;
                    return None;
                }
            }
            if *self.peek() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        if args.is_empty() || *self.peek() != Token::Gt {
            self.pos = saved_pos;
            return None;
        }
        self.advance(); // '>'
        if *self.peek() == Token::LParen {
            Some(args)
        } else {
            self.pos = saved_pos;
            None
        }
    }

    // Expression parsing (precedence climbing)
    fn parse_expr(&mut self) -> Result<Expr, Diagnostic> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_and()?;
        while *self.peek() == Token::Or {
            let span = expr.span;
            self.advance();
            let right = self.parse_and()?;
            expr = Expr::new(
                ExprKind::Binary {
                    op: BinaryOp::Or,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_eq()?;
        while *self.peek() == Token::And {
            let span = expr.span;
            self.advance();
            let right = self.parse_eq()?;
            expr = Expr::new(
                ExprKind::Binary {
                    op: BinaryOp::And,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_eq(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_cmp()?;
        while matches!(self.peek(), Token::Eq | Token::Neq) {
            let span = expr.span;
            let op = match self.advance() {
                Token::Eq => BinaryOp::Eq,
                Token::Neq => BinaryOp::Neq,
                _ => unreachable!(),
            };
            let right = self.parse_cmp()?;
            expr = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_cmp(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_add()?;
        while matches!(self.peek(), Token::Lt | Token::Gt | Token::Le | Token::Ge) {
            let span = expr.span;
            let op = match self.advance() {
                Token::Lt => BinaryOp::Lt,
                Token::Gt => BinaryOp::Gt,
                Token::Le => BinaryOp::Le,
                Token::Ge => BinaryOp::Ge,
                _ => unreachable!(),
            };
            let right = self.parse_add()?;
            expr = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_add(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_mul()?;
        while matches!(self.peek(), Token::Plus | Token::Minus) {
            let span = expr.span;
            let op = match self.advance() {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_mul()?;
            expr = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_mul(&mut self) -> Result<Expr, Diagnostic> {
        let mut expr = self.parse_unary()?;
        while matches!(self.peek(), Token::Star | Token::Slash | Token::Percent) {
            let span = expr.span;
            let op = match self.advance() {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => unreachable!(),
            };
            let right = self.parse_unary()?;
            expr = Expr::new(
                ExprKind::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            );
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, Diagnostic> {
        let span = self.peek_span();
        match self.peek() {
            Token::Minus => {
                self.advance();
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(self.parse_unary()?),
                    },
                    span,
                ))
            }
            Token::Not => {
                self.advance();
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(self.parse_unary()?),
                    },
                    span,
                ))
            }
            Token::Amp => {
                self.advance();
                Ok(Expr::new(
                    ExprKind::Unary {
                        op: UnaryOp::Ref,
                        expr: Box::new(self.parse_unary()?),
                    },
                    span,
                ))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, Diagnostic> {
        let span = self.peek_span();
        let base = match self.peek().clone() {
            Token::Int(v) => {
                self.advance();
                Expr::new(ExprKind::Int(v), span)
            }
            Token::Float(v) => {
                self.advance();
                Expr::new(ExprKind::Float(v), span)
            }
            Token::Char(v) => {
                self.advance();
                Expr::new(ExprKind::Char(v), span)
            }
            Token::Str(v) => {
                self.advance();
                Expr::new(ExprKind::Str(v), span)
            }
            Token::Bool(v) => {
                self.advance();
                Expr::new(ExprKind::Bool(v), span)
            }
            Token::SelfKw => {
                self.advance();
                Expr::new(ExprKind::SelfRef, span)
            }
            Token::LBracket => {
                self.advance();
                let mut elems = Vec::new();
                while *self.peek() != Token::RBracket && *self.peek() != Token::Eof {
                    elems.push(self.parse_expr()?);
                    if *self.peek() == Token::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(Token::RBracket)?;
                Expr::new(ExprKind::ArrayLiteral(elems), span)
            }
            Token::Ident(s)
                if s == "input"
                    && *self.peek_n(1) == Token::LParen
                    && *self.peek_n(2) == Token::RParen =>
            {
                self.advance(); // "input"
                self.advance(); // '('
                self.advance(); // ')'
                Expr::new(ExprKind::InputCall, span)
            }
            Token::Ident(s) => {
                self.advance();
                if *self.peek() == Token::LParen {
                    let args = self.parse_arg_list()?;
                    Expr::new(
                        ExprKind::Call {
                            callee: s,
                            type_args: Vec::new(),
                            args,
                        },
                        span,
                    )
                } else if let Some(type_args) = self.try_parse_expr_type_args() {
                    let args = self.parse_arg_list()?;
                    Expr::new(
                        ExprKind::Call {
                            callee: s,
                            type_args,
                            args,
                        },
                        span,
                    )
                } else {
                    Expr::new(ExprKind::Ident(s), span)
                }
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                expr
            }
            other => {
                return Err(self.err(
                    span,
                    format!("Unexpected token in expression: {:?}", other),
                ));
            }
        };

        let mut expr = base;
        loop {
            if *self.peek() == Token::LBracket {
                let bracket_span = self.peek_span();
                self.advance(); // '['
                let mut selectors = Vec::new();
                loop {
                    let sel_span = self.peek_span();
                    let start = if *self.peek() == Token::DotDot {
                        None
                    } else {
                        Some(self.parse_expr()?)
                    };
                    if *self.peek() == Token::DotDot {
                        self.advance(); // '..'
                        let end = if matches!(self.peek(), Token::RBracket | Token::Comma) {
                            None
                        } else {
                            Some(self.parse_expr()?)
                        };
                        selectors.push(SliceSelector::Range { start, end });
                    } else {
                        let index = start.ok_or_else(|| {
                            self.err(sel_span, "Expected index or slice expression inside '[]'")
                        })?;
                        selectors.push(SliceSelector::Index(index));
                    }

                    if *self.peek() == Token::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(Token::RBracket)?;
                if selectors.len() == 1 {
                    match selectors.pop().unwrap() {
                        SliceSelector::Index(index) => {
                            expr = Expr::new(
                                ExprKind::Index {
                                    target: Box::new(expr),
                                    index: Box::new(index),
                                },
                                bracket_span,
                            );
                        }
                        SliceSelector::Range { start, end } => {
                            expr = Expr::new(
                                ExprKind::SliceView {
                                    target: Box::new(expr),
                                    start: start.map(Box::new),
                                    end: end.map(Box::new),
                                },
                                bracket_span,
                            );
                        }
                    }
                } else {
                    expr = Expr::new(
                        ExprKind::MultiIndex {
                            target: Box::new(expr),
                            selectors,
                        },
                        bracket_span,
                    );
                }
            } else if *self.peek() == Token::Dot {
                let dot_span = self.peek_span();
                self.advance(); // '.'
                let (member, _) = self.expect_ident()?;
                if *self.peek() == Token::LParen {
                    let args = self.parse_arg_list()?;
                    expr = Expr::new(
                        ExprKind::MethodCall {
                            target: Box::new(expr),
                            method: member,
                            type_args: Vec::new(),
                            args,
                        },
                        dot_span,
                    );
                } else if let Some(type_args) = self.try_parse_expr_type_args() {
                    let args = self.parse_arg_list()?;
                    expr = Expr::new(
                        ExprKind::MethodCall {
                            target: Box::new(expr),
                            method: member,
                            type_args,
                            args,
                        },
                        dot_span,
                    );
                } else {
                    expr = Expr::new(
                        ExprKind::FieldAccess {
                            target: Box::new(expr),
                            field: member,
                        },
                        dot_span,
                    );
                }
            } else {
                break;
            }
        }
        Ok(expr)
    }
}

fn is_valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
