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
            Token::If => self.parse_if(),
            Token::For => self.parse_for(),
            Token::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            Token::Continue => {
                self.advance();
                Ok(Stmt::Continue)
            }
            Token::LBracket => self.parse_multi_assign(),
            Token::Ident(name) if name == "while" => self.parse_while(),
            Token::Ident(name)
                if (name == "print" || name == "output") && *self.peek_n(1) == Token::LParen =>
            {
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
                    self.parse_ident_stmt()
                }
            }
            other => Err(format!(
                "Unexpected token at start of statement: {:?}",
                other
            )),
        }
    }

    fn parse_assign_target_item(&mut self) -> Result<AssignTarget, String> {
        let name = self.expect_ident()?;
        if *self.peek() != Token::LBracket {
            return Ok(AssignTarget::Ident(name));
        }
        self.advance(); // '['
        let first_expr = self.parse_expr()?;
        if *self.peek() == Token::DotDot {
            self.advance(); // '..'
            let end_expr = self.parse_expr()?;
            self.expect(Token::RBracket)?;
            return Ok(AssignTarget::Slice {
                target: name,
                row: None,
                start: Some(first_expr),
                end: Some(end_expr),
            });
        }
        self.expect(Token::RBracket)?;
        if *self.peek() == Token::LBracket {
            self.advance(); // '['
            let second_expr = self.parse_expr()?;
            if *self.peek() == Token::DotDot {
                self.advance(); // '..'
                let end_expr = self.parse_expr()?;
                self.expect(Token::RBracket)?;
                let row_idx = match first_expr {
                    Expr::Int(i) if i >= 0 => i as usize,
                    _ => return Err("Row index must be a non-negative integer".to_string()),
                };
                return Ok(AssignTarget::Slice {
                    target: name,
                    row: Some(row_idx),
                    start: Some(second_expr),
                    end: Some(end_expr),
                });
            }
            self.expect(Token::RBracket)?;
            return Ok(AssignTarget::Index {
                target: name,
                row: Some(first_expr),
                index: second_expr,
            });
        }
        Ok(AssignTarget::Index {
            target: name,
            row: None,
            index: first_expr,
        })
    }

    fn parse_multi_assign(&mut self) -> Result<Stmt, String> {
        self.expect(Token::LBracket)?;
        let mut targets = Vec::new();
        while *self.peek() != Token::RBracket && *self.peek() != Token::Eof {
            targets.push(self.parse_assign_target_item()?);
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
            Some(self.parse_prim_type()?)
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

    fn parse_print(&mut self) -> Result<Stmt, String> {
        self.advance(); // "print" or "output"
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

    fn parse_prim_type(&mut self) -> Result<PrimType, String> {
        let name = self.expect_ident()?;
        match name.to_lowercase().as_str() {
            "int" => Ok(PrimType::Int),
            "uint" => Ok(PrimType::Uint),
            "float" => Ok(PrimType::Float),
            "char" => Ok(PrimType::Char),
            "string" => Ok(PrimType::String),
            "bool" => Ok(PrimType::Bool),
            _ => Err(format!("Unknown primitive type: {}", name)),
        }
    }

    fn parse_slice_range(&mut self) -> Result<Option<Expr>, String> {
        self.expect(Token::LBracket)?;
        // Support both A[N] / A[] (v0.2) and A[...N] / A[...] (legacy)
        if *self.peek() == Token::Ellipsis {
            self.advance();
            let len = if *self.peek() == Token::RBracket {
                None
            } else {
                Some(self.parse_expr()?)
            };
            self.expect(Token::RBracket)?;
            Ok(len)
        } else if *self.peek() == Token::RBracket {
            self.expect(Token::RBracket)?;
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
                row_types.push(self.parse_prim_type()?);
                if *self.peek() == Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::RBracket)?;
            let len = self.parse_slice_range()?;
            Ok(TypeSpec::Table { row_types, len })
        } else {
            let prim = self.parse_prim_type()?;
            if *self.peek() == Token::LBracket {
                let len = self.parse_slice_range()?;
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

        let mut input_mode = None;
        if *self.peek() == Token::LParen {
            self.advance();
            let mod_name = self.expect_ident()?;
            if mod_name.eq_ignore_ascii_case("input") {
                let mode = if *self.peek() == Token::LParen {
                    self.advance();
                    let m = self.expect_ident()?;
                    self.expect(Token::RParen)?;
                    match m.to_lowercase().as_str() {
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

        Ok(Stmt::VarDecl {
            names,
            ty,
            init,
            input_mode,
        })
    }

    fn parse_fn_input(&mut self) -> Result<Stmt, String> {
        self.expect_ident()?; // "input"
        self.expect(Token::LParen)?;
        let mut targets = Vec::new();
        while *self.peek() != Token::RParen {
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
            } else if *self.peek() == Token::LBracket {
                self.advance(); // '['
                while *self.peek() != Token::RBracket {
                    let (name, row, len) = self.parse_slice_target()?;
                    targets.push(InputTarget::Slice {
                        target: name,
                        row,
                        len,
                    });
                    if *self.peek() == Token::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(Token::RBracket)?;
            } else {
                let (name, row, len) = self.parse_slice_target()?;
                targets.push(InputTarget::Slice {
                    target: name,
                    row,
                    len,
                });
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

    fn parse_slice_target(&mut self) -> Result<(String, Option<usize>, Option<Expr>), String> {
        let name = self.expect_ident()?;
        let mut row = None;
        let mut len = None;
        if *self.peek() == Token::LBracket {
            if *self.peek_n(1) != Token::Ellipsis {
                self.advance(); // '['
                match self.advance() {
                    Token::Int(idx) if idx >= 0 => row = Some(idx as usize),
                    other => {
                        return Err(format!("Expected non-negative row index, got {:?}", other));
                    }
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
        let name = self.expect_ident()?;
        if *self.peek() == Token::LParen {
            self.advance();
            let mod_name = self.expect_ident()?;
            if mod_name != "Output" {
                return Err(format!("Expected Output modifier, got {}", mod_name));
            }
            self.expect(Token::RParen)?;
            return Ok(Stmt::Output {
                target: name,
                row: None,
                len: None,
            });
        }

        if *self.peek() == Token::LBracket {
            self.advance(); // '['
            if *self.peek() == Token::Ellipsis {
                self.advance();
                let len = if *self.peek() == Token::RBracket {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.expect(Token::RBracket)?;
                if *self.peek() == Token::LParen {
                    self.advance();
                    self.expect_ident()?;
                    self.expect(Token::RParen)?;
                    return Ok(Stmt::Output {
                        target: name,
                        row: None,
                        len,
                    });
                }
                self.expect(Token::Assign)?;
                let value = self.parse_assign_value()?;
                return Ok(Stmt::SliceAssign {
                    target: name,
                    row: None,
                    len,
                    value,
                });
            }

            let first_expr = self.parse_expr()?;
            if *self.peek() == Token::DotDot {
                self.advance(); // '..'
                let end_expr = self.parse_expr()?;
                self.expect(Token::RBracket)?;
                self.expect(Token::Assign)?;
                let value = self.parse_expr()?;
                return Ok(Stmt::Assign {
                    target: AssignTarget::Slice {
                        target: name,
                        row: None,
                        start: Some(first_expr),
                        end: Some(end_expr),
                    },
                    value,
                });
            }
            self.expect(Token::RBracket)?;

            if *self.peek() == Token::LBracket {
                self.advance();
                let row_idx = match first_expr {
                    Expr::Int(i) if i >= 0 => i as usize,
                    _ => return Err("Row index must be a non-negative integer".to_string()),
                };

                if *self.peek() == Token::Ellipsis {
                    self.advance();
                    let len = if *self.peek() == Token::RBracket {
                        None
                    } else {
                        Some(self.parse_expr()?)
                    };
                    self.expect(Token::RBracket)?;
                    if *self.peek() == Token::LParen {
                        self.advance();
                        self.expect_ident()?;
                        self.expect(Token::RParen)?;
                        return Ok(Stmt::Output {
                            target: name,
                            row: Some(row_idx),
                            len,
                        });
                    }
                    self.expect(Token::Assign)?;
                    let value = self.parse_assign_value()?;
                    return Ok(Stmt::SliceAssign {
                        target: name,
                        row: Some(row_idx),
                        len,
                        value,
                    });
                }

                let start_expr = self.parse_expr()?;
                if *self.peek() == Token::DotDot {
                    self.advance();
                    let end_expr = self.parse_expr()?;
                    self.expect(Token::RBracket)?;
                    self.expect(Token::Assign)?;
                    let value = self.parse_expr()?;
                    return Ok(Stmt::Assign {
                        target: AssignTarget::Slice {
                            target: name,
                            row: Some(row_idx),
                            start: Some(start_expr),
                            end: Some(end_expr),
                        },
                        value,
                    });
                }
                self.expect(Token::RBracket)?;
                self.expect(Token::Assign)?;
                let value = self.parse_expr()?;
                return Ok(Stmt::Assign {
                    target: AssignTarget::Index {
                        target: name,
                        row: Some(Expr::Int(row_idx as i64)),
                        index: start_expr,
                    },
                    value,
                });
            }

            self.expect(Token::Assign)?;
            let value = self.parse_expr()?;
            return Ok(Stmt::Assign {
                target: AssignTarget::Index {
                    target: name,
                    row: None,
                    index: first_expr,
                },
                value,
            });
        }

        self.expect(Token::Assign)?;
        let value = self.parse_expr()?;
        Ok(Stmt::Assign {
            target: AssignTarget::Ident(name),
            value,
        })
    }

    fn parse_assign_value(&mut self) -> Result<AssignValue, String> {
        if let Token::Ident(_) = self.peek() {
            if *self.peek_n(1) == Token::LBracket {
                let (src, s_row, s_len) = self.parse_slice_target()?;
                return Ok(AssignValue::Slice {
                    source: src,
                    row: s_row,
                    len: s_len,
                });
            }
        }
        Ok(AssignValue::Scalar(self.parse_expr()?))
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
        while *self.peek() == Token::LBracket {
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
        }
        Ok(expr)
    }
}
