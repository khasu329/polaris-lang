use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Cursor, Write};

use crate::ast::*;
use crate::value::{Status, Value};

#[derive(Debug, Clone)]
pub enum VarStorage {
    Scalar(Value),
    Table {
        row_types: Vec<PrimType>,
        rows: Vec<Vec<Value>>,
        is_fixed: bool,
    },
}

pub struct InputScanner {
    reader: Box<dyn BufRead>,
    chars: VecDeque<char>,
}

impl InputScanner {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(input: &str) -> Self {
        Self {
            reader: Box::new(Cursor::new(input.as_bytes().to_vec())),
            chars: VecDeque::new(),
        }
    }

    pub fn from_stdin() -> Self {
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
            let (ctrl, _) = self.exec_stmt(stmt, out)?;
            if ctrl != LoopCtrl::None {
                return Err("break/continue used outside of a loop".to_string());
            }
        }
        Ok(())
    }

    fn eval_usize(&self, expr: &Expr, locals: &HashMap<String, Value>) -> Result<usize, String> {
        let (val, _) = self.eval_expr(expr, locals)?;
        match val {
            Value::Int(n) if n >= 0 => Ok(n as usize),
            Value::Uint(n) => Ok(n as usize),
            other => Err(format!(
                "Expected non-negative integer length, got {:?}",
                other
            )),
        }
    }

    fn create_storage_for_type(
        &mut self,
        ty: &TypeSpec,
        input_mode: Option<InputMode>,
    ) -> Result<VarStorage, String> {
        match ty {
            TypeSpec::Scalar(prim) => {
                let val = if let Some(mode) = input_mode {
                    self.scanner.read_value(*prim, mode)?
                } else {
                    Value::default_for(*prim)
                };
                Ok(VarStorage::Scalar(val))
            }
            TypeSpec::Array1D { elem, len } => {
                let actual_len = match len {
                    Some(e) => self.eval_usize(e, &HashMap::new())?,
                    None => 0,
                };
                let mut row = Vec::with_capacity(actual_len);
                if let Some(mode) = input_mode {
                    if mode == InputMode::All && len.is_none() {
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
                            row.push(self.scanner.read_value(*elem, mode)?);
                        }
                    }
                } else {
                    row.resize(actual_len, Value::default_for(*elem));
                }
                Ok(VarStorage::Table {
                    row_types: vec![*elem],
                    rows: vec![row],
                    is_fixed: len.is_some(),
                })
            }
            TypeSpec::Table { row_types, len } => {
                let actual_len = match len {
                    Some(e) => self.eval_usize(e, &HashMap::new())?,
                    None => 0,
                };
                if input_mode.is_some() {
                    return Err(
                        "Direct Input on multidimensional table is not supported".to_string()
                    );
                }
                let rows = row_types
                    .iter()
                    .map(|t| vec![Value::default_for(*t); actual_len])
                    .collect();
                Ok(VarStorage::Table {
                    row_types: row_types.clone(),
                    rows,
                    is_fixed: len.is_some(),
                })
            }
        }
    }

    fn read_into_target(
        &mut self,
        target: &str,
        row_opt: Option<usize>,
        len_opt: Option<usize>,
    ) -> Result<(), String> {
        let storage = self
            .env
            .get_mut(target)
            .ok_or_else(|| format!("Undefined variable: {}", target))?;
        match storage {
            VarStorage::Scalar(val) => {
                let prim = match val {
                    Value::Int(_) => PrimType::Int,
                    Value::Uint(_) => PrimType::Uint,
                    Value::Float(_) => PrimType::Float,
                    Value::Char(_) => PrimType::Char,
                    Value::String(_) => PrimType::String,
                    Value::Bool(_) => PrimType::Bool,
                    Value::Array(_) => {
                        return Err("Cannot read into Array value directly".to_string());
                    }
                };
                *val = self.scanner.read_value(prim, InputMode::Token)?;
            }
            VarStorage::Table {
                row_types,
                rows,
                is_fixed,
            } => {
                if let Some(r_idx) = row_opt {
                    let r = rows
                        .get_mut(r_idx)
                        .ok_or_else(|| format!("Row {} out of bounds in {}", r_idx, target))?;
                    let n = len_opt.unwrap_or(r.len());
                    let ty = row_types[r_idx];
                    if !*is_fixed && r.len() < n {
                        r.resize(n, Value::default_for(ty));
                    }
                    for slot in r.iter_mut().take(n) {
                        *slot = self.scanner.read_value(ty, InputMode::Token)?;
                    }
                } else {
                    for (r_idx, r) in rows.iter_mut().enumerate() {
                        let n = len_opt.unwrap_or(r.len());
                        let ty = row_types[r_idx];
                        if !*is_fixed && r.len() < n {
                            r.resize(n, Value::default_for(ty));
                        }
                        for slot in r.iter_mut().take(n) {
                            *slot = self.scanner.read_value(ty, InputMode::Token)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn output_target(
        &self,
        target: &str,
        row: Option<usize>,
        target_len: Option<usize>,
        out: &mut dyn Write,
    ) -> Result<(), String> {
        let storage = self
            .env
            .get(target)
            .ok_or_else(|| format!("Undefined variable: {}", target))?;
        match storage {
            VarStorage::Scalar(v) => {
                writeln!(out, "{}", v.display_str()).map_err(|e| e.to_string())?;
            }
            VarStorage::Table {
                rows, row_types, ..
            } => {
                if let Some(r_idx) = row {
                    let r = rows
                        .get(r_idx)
                        .ok_or_else(|| format!("Row {} out of bounds in {}", r_idx, target))?;
                    let n = target_len.unwrap_or(r.len()).min(r.len());
                    if row_types.get(r_idx) == Some(&PrimType::String) {
                        for v in &r[..n] {
                            writeln!(out, "{}", v.display_str()).map_err(|e| e.to_string())?;
                        }
                    } else {
                        let line = r[..n]
                            .iter()
                            .map(|v| v.display_str())
                            .collect::<Vec<_>>()
                            .join(" ");
                        writeln!(out, "{}", line).map_err(|e| e.to_string())?;
                    }
                } else {
                    for (r_idx, r) in rows.iter().enumerate() {
                        let n = target_len.unwrap_or(r.len()).min(r.len());
                        if row_types.get(r_idx) == Some(&PrimType::String) {
                            for v in &r[..n] {
                                writeln!(out, "{}", v.display_str()).map_err(|e| e.to_string())?;
                            }
                        } else {
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
        }
        Ok(())
    }

    fn eval_assign_value(
        &self,
        value: &AssignValue,
        target_len: Option<usize>,
    ) -> Result<Vec<Value>, String> {
        match value {
            AssignValue::Scalar(expr) => {
                let (val, _) = self.eval_expr(expr, &HashMap::new())?;
                let n = target_len.ok_or_else(|| {
                    "Broadcast assignment requires explicit length [N] / [...N]".to_string()
                })?;
                Ok(vec![val; n])
            }
            AssignValue::Slice {
                source,
                row: s_row,
                len: s_len,
            } => {
                let src_row_idx = s_row.unwrap_or(0);
                let (src_vec, src_len) = match self.env.get(source) {
                    Some(VarStorage::Table { rows, .. }) => {
                        let row = rows.get(src_row_idx).ok_or_else(|| {
                            format!("Row {} out of bounds in {}", src_row_idx, source)
                        })?;
                        (row.clone(), row.len())
                    }
                    Some(VarStorage::Scalar(_)) => {
                        return Err(format!("Cannot slice scalar {}", source));
                    }
                    None => return Err(format!("Undefined variable: {}", source)),
                };
                let copy_len = match s_len {
                    Some(e) => self.eval_usize(e, &HashMap::new())?,
                    None => src_len,
                };
                if copy_len > src_vec.len() {
                    return Err(format!(
                        "Source slice length {} exceeds {}",
                        copy_len,
                        src_vec.len()
                    ));
                }
                Ok(src_vec[..copy_len].to_vec())
            }
        }
    }

    fn apply_assign_target(&mut self, target: &AssignTarget, val: Value) -> Result<(), String> {
        let empty = HashMap::new();
        match target {
            AssignTarget::Ident(name) => {
                let storage = self
                    .env
                    .get_mut(name)
                    .ok_or_else(|| format!("Undefined variable: {}", name))?;
                match storage {
                    VarStorage::Scalar(v) => *v = val,
                    _ => return Err(format!("Cannot assign scalar to table {}", name)),
                }
            }
            AssignTarget::Index { target, row, index } => {
                let row_idx = match row {
                    Some(r) => self.eval_usize(r, &empty)?,
                    None => 0,
                };
                let idx = self.eval_usize(index, &empty)?;
                let storage = self
                    .env
                    .get_mut(target)
                    .ok_or_else(|| format!("Undefined variable: {}", target))?;
                match storage {
                    VarStorage::Table { rows, .. } => {
                        let r = rows.get_mut(row_idx).ok_or_else(|| {
                            format!("Row {} out of bounds in {}", row_idx, target)
                        })?;
                        *r.get_mut(idx).ok_or_else(|| {
                            format!("Index {} out of bounds in {}", idx, target)
                        })? = val;
                    }
                    VarStorage::Scalar(_) => {
                        return Err(format!("Cannot index scalar {}", target));
                    }
                }
            }
            AssignTarget::Slice {
                target,
                row,
                start,
                end,
            } => {
                let row_idx = row.unwrap_or(0);
                let s = match start {
                    Some(e) => self.eval_usize(e, &empty)?,
                    None => 0,
                };
                let e_idx_opt = match end {
                    Some(e) => Some(self.eval_usize(e, &empty)?),
                    None => None,
                };
                let storage = self
                    .env
                    .get_mut(target)
                    .ok_or_else(|| format!("Undefined variable: {}", target))?;
                match storage {
                    VarStorage::Table { rows, .. } => {
                        let r = rows.get_mut(row_idx).ok_or_else(|| {
                            format!("Row {} out of bounds in {}", row_idx, target)
                        })?;
                        let e_idx = e_idx_opt.unwrap_or(r.len()).min(r.len());
                        for slot in r[s..e_idx].iter_mut() {
                            *slot = val.clone();
                        }
                    }
                    VarStorage::Scalar(_) => {
                        return Err(format!("Cannot slice scalar {}", target));
                    }
                }
            }
        }
        Ok(())
    }

    fn exec_block(&mut self, stmts: &[Stmt], out: &mut dyn Write) -> Result<LoopCtrl, String> {
        for stmt in stmts {
            let (ctrl, _) = self.exec_stmt(stmt, out)?;
            if ctrl != LoopCtrl::None {
                return Ok(ctrl);
            }
        }
        Ok(LoopCtrl::None)
    }

    fn exec_stmt(
        &mut self,
        stmt: &Stmt,
        out: &mut dyn Write,
    ) -> Result<(LoopCtrl, Status), String> {
        match stmt {
            Stmt::VarDecl {
                names,
                ty,
                init,
                input_mode,
            } => {
                for name in names {
                    if let Some(Expr::InputCall) = init {
                        let mode = input_mode.unwrap_or(InputMode::Token);
                        let storage = self.create_storage_for_type(ty, Some(mode))?;
                        self.env.insert(name.clone(), storage);
                    } else if let Some(init_expr) = init {
                        let (val, _) = self.eval_expr(init_expr, &HashMap::new())?;
                        self.env.insert(name.clone(), VarStorage::Scalar(val));
                    } else {
                        let storage = self.create_storage_for_type(ty, *input_mode)?;
                        self.env.insert(name.clone(), storage);
                    }
                }
                Ok((LoopCtrl::None, Status::Executed))
            }
            Stmt::Assign { target, value } => {
                let (val, _) = self.eval_expr(value, &HashMap::new())?;
                self.apply_assign_target(target, val)?;
                Ok((LoopCtrl::None, Status::Executed))
            }
            Stmt::MultiAssign { targets, values } => {
                if targets.len() != values.len() {
                    return Err(format!(
                        "Multi-assignment arity mismatch: {} targets vs {} values",
                        targets.len(),
                        values.len()
                    ));
                }
                let mut evaluated = Vec::with_capacity(values.len());
                for expr in values {
                    let (v, _) = self.eval_expr(expr, &HashMap::new())?;
                    evaluated.push(v);
                }
                for (target, val) in targets.iter().zip(evaluated.into_iter()) {
                    self.apply_assign_target(target, val)?;
                }
                Ok((LoopCtrl::None, Status::Executed))
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let (cond_val, _) = self.eval_expr(cond, &HashMap::new())?;
                let ctrl = if cond_val == Value::Bool(true) {
                    self.exec_block(then_branch, out)?
                } else if let Some(else_stmts) = else_branch {
                    self.exec_block(else_stmts, out)?
                } else {
                    LoopCtrl::None
                };
                Ok((ctrl, Status::Executed))
            }
            Stmt::For {
                var_name,
                var_ty,
                range_start,
                range_end,
                body,
            } => {
                let (start_val, _) = self.eval_expr(range_start, &HashMap::new())?;
                let (end_val, _) = self.eval_expr(range_end, &HashMap::new())?;
                let start = match start_val {
                    Value::Int(i) => i,
                    Value::Uint(u) => u as i64,
                    other => {
                        return Err(format!("For range start must be integer, got {:?}", other));
                    }
                };
                let end = match end_val {
                    Value::Int(i) => i,
                    Value::Uint(u) => u as i64,
                    other => return Err(format!("For range end must be integer, got {:?}", other)),
                };
                for i in start..end {
                    let loop_val = match var_ty {
                        Some(PrimType::Uint) => Value::Uint(i as u64),
                        _ => Value::Int(i),
                    };
                    self.env
                        .insert(var_name.clone(), VarStorage::Scalar(loop_val));
                    match self.exec_block(body, out)? {
                        LoopCtrl::None | LoopCtrl::Continue => continue,
                        LoopCtrl::Break => break,
                    }
                }
                Ok((LoopCtrl::None, Status::Executed))
            }
            Stmt::Break => Ok((LoopCtrl::Break, Status::Executed)),
            Stmt::Continue => Ok((LoopCtrl::Continue, Status::Executed)),
            Stmt::Print { args } => {
                if args.is_empty() {
                    writeln!(out).map_err(|e| e.to_string())?;
                } else {
                    let mut parts = Vec::new();
                    for arg in args {
                        let (val, _) = self.eval_expr(arg, &HashMap::new())?;
                        parts.push(val.display_str());
                    }
                    writeln!(out, "{}", parts.join(" ")).map_err(|e| e.to_string())?;
                }
                Ok((LoopCtrl::None, Status::Executed))
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

                let new_row_vals = self.eval_assign_value(value, target_len)?;

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
                        let r = rows.get_mut(row_idx).ok_or_else(|| {
                            format!("Row {} out of bounds in {}", row_idx, target)
                        })?;
                        if !*is_fixed && r.len() < new_row_vals.len() {
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
                    VarStorage::Scalar(_) => {
                        return Err(format!("Cannot slice assign to scalar {}", target));
                    }
                }
                Ok((LoopCtrl::None, Status::Executed))
            }
            Stmt::Output { target, row, len } => {
                let target_len = match len {
                    Some(e) => Some(self.eval_usize(e, &HashMap::new())?),
                    None => None,
                };
                self.output_target(target, *row, target_len, out)?;
                Ok((LoopCtrl::None, Status::Executed))
            }
            Stmt::FnInput { targets } => {
                for target in targets {
                    match target {
                        InputTarget::Decl { names, ty } => {
                            for name in names {
                                let storage = self.create_storage_for_type(ty, None)?;
                                self.env.insert(name.clone(), storage);
                                self.read_into_target(name, None, None)?;
                            }
                        }
                        InputTarget::Slice { target, row, len } => {
                            let target_len = match len {
                                Some(e) => Some(self.eval_usize(e, &HashMap::new())?),
                                None => None,
                            };
                            self.read_into_target(target, *row, target_len)?;
                        }
                    }
                }
                Ok((LoopCtrl::None, Status::Executed))
            }
            Stmt::FnOutput { targets } => {
                for target in targets {
                    let target_len = match &target.len {
                        Some(e) => Some(self.eval_usize(e, &HashMap::new())?),
                        None => None,
                    };
                    self.output_target(&target.target, target.row, target_len, out)?;
                }
                Ok((LoopCtrl::None, Status::Executed))
            }
            Stmt::While { cond, step, body } => {
                let status = self.exec_while(cond, step, body)?;
                Ok((LoopCtrl::None, status))
            }
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

            let maybe_pos = match ext.mode {
                ExtractMode::Continuous | ExtractMode::Single | ExtractMode::Any => {
                    if idx + window_width <= col_len {
                        Some(idx)
                    } else {
                        None
                    }
                }
                ExtractMode::Head => {
                    if idx == 0 && window_width <= col_len {
                        Some(0)
                    } else {
                        None
                    }
                }
                ExtractMode::Tail => {
                    if idx == 0 && window_width <= col_len {
                        Some(col_len - window_width)
                    } else {
                        None
                    }
                }
            };

            let Some(pos) = maybe_pos else {
                break;
            };
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
                    let target_rows =
                        self.resolve_rule_rows(rule, &var_row_map, ext.patterns.len())?;

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
                                        let (lit_val, _) =
                                            self.eval_expr(lit_expr, &rule_locals)?;
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

                    let mut changed = lhs_width != rhs_width;
                    if let Some(VarStorage::Table { rows, .. }) = self.env.get_mut(&ext.target) {
                        for (i, &actual_r) in target_rows.iter().enumerate() {
                            let replacement = new_slices[i].clone();
                            if !changed && rows[actual_r][pos..(pos + lhs_width)] != replacement[..]
                            {
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
                } else {
                    match self.env.get(name) {
                        Some(VarStorage::Scalar(v)) => v.clone(),
                        Some(VarStorage::Table { rows, .. }) => {
                            Value::Array(rows.first().cloned().unwrap_or_default())
                        }
                        None => return Err(format!("Undefined variable in expression: {}", name)),
                    }
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
            Expr::Index { target, index } => {
                let idx = self.eval_usize(index, locals)?;
                // Support 2D table row indexing: Table[row][col]
                if let Expr::Index {
                    target: outer,
                    index: row_expr,
                } = target.as_ref()
                {
                    if let Expr::Ident(name) = outer.as_ref() {
                        if let Some(VarStorage::Table { rows, .. }) = self.env.get(name) {
                            let r_idx = self.eval_usize(row_expr, locals)?;
                            let row = rows.get(r_idx).ok_or_else(|| {
                                format!("Row {} out of bounds in {}", r_idx, name)
                            })?;
                            let val = row.get(idx).cloned().ok_or_else(|| {
                                format!("Index {} out of bounds in {}[{}]", idx, name, r_idx)
                            })?;
                            return Ok((val, Status::Executed));
                        }
                    }
                }
                let (tval, _) = self.eval_expr(target, locals)?;
                match tval {
                    Value::Array(arr) => arr
                        .into_iter()
                        .nth(idx)
                        .ok_or_else(|| format!("Index {} out of bounds", idx))?,
                    Value::String(s) => s
                        .chars()
                        .nth(idx)
                        .map(Value::Char)
                        .ok_or_else(|| format!("String index {} out of bounds", idx))?,
                    _ => return Err("Cannot index non-array/non-string value".to_string()),
                }
            }
            Expr::SliceView { target, start, end } => {
                // Support 2D table row slicing: Table[row][s..e]
                if let Expr::Index {
                    target: outer,
                    index: row_expr,
                } = target.as_ref()
                {
                    if let Expr::Ident(name) = outer.as_ref() {
                        if let Some(VarStorage::Table { rows, .. }) = self.env.get(name) {
                            let r_idx = self.eval_usize(row_expr, locals)?;
                            let row = rows.get(r_idx).ok_or_else(|| {
                                format!("Row {} out of bounds in {}", r_idx, name)
                            })?;
                            let s = match start {
                                Some(e) => self.eval_usize(e, locals)?,
                                None => 0,
                            };
                            let e_idx = match end {
                                Some(e) => self.eval_usize(e, locals)?,
                                None => row.len(),
                            };
                            let end_clamped = e_idx.min(row.len());
                            let start_clamped = s.min(end_clamped);
                            return Ok((
                                Value::Array(row[start_clamped..end_clamped].to_vec()),
                                Status::Executed,
                            ));
                        }
                    }
                }
                let (tval, _) = self.eval_expr(target, locals)?;
                let s = match start {
                    Some(e) => self.eval_usize(e, locals)?,
                    None => 0,
                };
                match tval {
                    Value::Array(row_data) => {
                        let e_idx = match end {
                            Some(e) => self.eval_usize(e, locals)?,
                            None => row_data.len(),
                        };
                        let end_clamped = e_idx.min(row_data.len());
                        let start_clamped = s.min(end_clamped);
                        Value::Array(row_data[start_clamped..end_clamped].to_vec())
                    }
                    Value::String(str_val) => {
                        let chars: Vec<char> = str_val.chars().collect();
                        let e_idx = match end {
                            Some(e) => self.eval_usize(e, locals)?,
                            None => chars.len(),
                        };
                        let end_clamped = e_idx.min(chars.len());
                        let start_clamped = s.min(end_clamped);
                        Value::String(chars[start_clamped..end_clamped].iter().collect())
                    }
                    _ => return Err("SliceView target must be an array or string".to_string()),
                }
            }
            Expr::InputCall => {
                return Err(
                    "InputCall cannot be used in this expression context; use 'N: int = input()' in variable declaration"
                        .to_string(),
                );
            }
        };
        Ok((val, Status::Executed))
    }
}

pub fn eval_binary(op: BinaryOp, l: Value, r: Value) -> Result<Value, String> {
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
