use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Cursor, Write};
use std::rc::Rc;

use crate::ast::*;
use crate::value::{ClassInstance, Value};

#[derive(Debug, Clone)]
pub enum VarStorage {
    Scalar(Value),
    Table {
        row_types: Vec<TypeRef>,
        rows: Vec<Vec<Value>>,
        is_fixed: bool,
    },
}

#[derive(Debug, Clone)]
struct ScopeFrame {
    vars: HashMap<String, VarStorage>,
    order: Vec<String>,
}

impl ScopeFrame {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
            order: Vec::new(),
        }
    }

    fn insert(&mut self, name: String, storage: VarStorage) {
        if !self.vars.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.vars.insert(name, storage);
    }
}

#[derive(Debug, Clone, PartialEq)]
enum FlowCtrl {
    None,
    Break,
    Continue,
    Return(Value),
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

    fn parse_token_as(tok: &str, ty: &TypeRef) -> Result<Value, String> {
        match ty {
            TypeRef::Int => tok
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|e| format!("Failed to parse Int '{}': {}", tok, e)),
            TypeRef::Uint => tok
                .parse::<u64>()
                .map(Value::Uint)
                .map_err(|e| format!("Failed to parse Uint '{}': {}", tok, e)),
            TypeRef::Float => tok
                .parse::<f64>()
                .map(Value::Float)
                .map_err(|e| format!("Failed to parse Float '{}': {}", tok, e)),
            TypeRef::Char => {
                let mut chars = tok.chars();
                let ch = chars
                    .next()
                    .ok_or_else(|| "Empty token for Char".to_string())?;
                if chars.next().is_some() {
                    return Err(format!("Expected single Char, got '{}'", tok));
                }
                Ok(Value::Char(ch))
            }
            TypeRef::String => Ok(Value::String(tok.to_string())),
            TypeRef::Bool => tok
                .parse::<bool>()
                .map(Value::Bool)
                .map_err(|e| format!("Failed to parse Bool '{}': {}", tok, e)),
            TypeRef::Void | TypeRef::Custom(_) => {
                Err(format!("Cannot parse input token for type {}", ty.display_name()))
            }
        }
    }

    fn read_token(&mut self, ty: &TypeRef) -> Result<Value, String> {
        let tok = self.next_token()?;
        Self::parse_token_as(&tok, ty)
    }
}

pub struct Interpreter {
    globals: ScopeFrame,
    local_frames: Vec<ScopeFrame>,
    functions: HashMap<String, FnDecl>,
    classes: HashMap<String, ClassDecl>,
    scanner: InputScanner,
}

impl Interpreter {
    pub fn new(input: &str) -> Self {
        Self {
            globals: ScopeFrame::new(),
            local_frames: Vec::new(),
            functions: HashMap::new(),
            classes: HashMap::new(),
            scanner: InputScanner::from_str(input),
        }
    }

    pub fn from_stdin() -> Self {
        Self {
            globals: ScopeFrame::new(),
            local_frames: Vec::new(),
            functions: HashMap::new(),
            classes: HashMap::new(),
            scanner: InputScanner::from_stdin(),
        }
    }

    pub fn run(&mut self, stmts: &[Stmt], out: &mut dyn Write) -> Result<(), String> {
        for stmt in stmts {
            match stmt {
                Stmt::FnDecl(fd) => {
                    self.functions.insert(fd.name.clone(), fd.clone());
                }
                Stmt::ClassDecl(cd) => {
                    self.classes.insert(cd.name.clone(), cd.clone());
                }
                _ => {}
            }
        }

        let mut run_res = Ok(());
        for stmt in stmts {
            if matches!(stmt, Stmt::FnDecl(_) | Stmt::ClassDecl(_)) {
                continue;
            }
            match self.exec_stmt(stmt, out) {
                Ok(FlowCtrl::None) => {}
                Ok(FlowCtrl::Break | FlowCtrl::Continue) => {
                    run_res = Err("break/continue used outside of a loop".to_string());
                    break;
                }
                Ok(FlowCtrl::Return(_)) => {
                    run_res = Err("return used outside of a function".to_string());
                    break;
                }
                Err(e) => {
                    run_res = Err(e);
                    break;
                }
            }
        }

        let globals_frame = std::mem::replace(&mut self.globals, ScopeFrame::new());
        let cleanup_res = self.cleanup_frame(globals_frame, out);
        run_res?;
        cleanup_res?;
        Ok(())
    }

    fn lookup_var(&self, name: &str) -> Option<&VarStorage> {
        for frame in self.local_frames.iter().rev() {
            if let Some(v) = frame.vars.get(name) {
                return Some(v);
            }
        }
        self.globals.vars.get(name)
    }

    fn lookup_var_mut(&mut self, name: &str) -> Option<&mut VarStorage> {
        for frame in self.local_frames.iter_mut().rev() {
            if frame.vars.contains_key(name) {
                return frame.vars.get_mut(name);
            }
        }
        self.globals.vars.get_mut(name)
    }

    fn insert_current_scope(&mut self, name: String, storage: VarStorage) {
        if let Some(frame) = self.local_frames.last_mut() {
            frame.insert(name, storage);
        } else {
            self.globals.insert(name, storage);
        }
    }

    fn cleanup_frame(&mut self, mut frame: ScopeFrame, out: &mut dyn Write) -> Result<(), String> {
        let names: Vec<String> = frame.order.drain(..).rev().collect();
        for name in names {
            if let Some(storage) = frame.vars.remove(&name) {
                match storage {
                    VarStorage::Scalar(Value::Instance(rc)) => {
                        if Rc::strong_count(&rc) == 1 && !rc.borrow().dropped {
                            self.drop_instance(rc, out)?;
                        }
                    }
                    VarStorage::Scalar(Value::Array(arr)) => {
                        for item in arr.into_iter().rev() {
                            if let Value::Instance(rc) = item {
                                if Rc::strong_count(&rc) == 1 && !rc.borrow().dropped {
                                    self.drop_instance(rc, out)?;
                                }
                            }
                        }
                    }
                    VarStorage::Table { rows, .. } => {
                        for row in rows.into_iter().rev() {
                            for item in row.into_iter().rev() {
                                if let Value::Instance(rc) = item {
                                    if Rc::strong_count(&rc) == 1 && !rc.borrow().dropped {
                                        self.drop_instance(rc, out)?;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn eval_usize(&mut self, expr: &Expr, out: &mut dyn Write) -> Result<usize, String> {
        let val = self.eval_expr(expr, out)?;
        match val {
            Value::Int(n) if n >= 0 => Ok(n as usize),
            Value::Uint(n) => Ok(n as usize),
            other => Err(format!(
                "Expected non-negative integer length, got {:?}",
                other
            )),
        }
    }

    fn coerce_for_type(val: Value, ty: &TypeRef) -> Value {
        match (ty, val) {
            (TypeRef::Uint, Value::Int(n)) if n >= 0 => Value::Uint(n as u64),
            (_, v) => v,
        }
    }

    fn create_storage_for_type(
        &mut self,
        ty: &TypeSpec,
        read_input: bool,
        out: &mut dyn Write,
    ) -> Result<VarStorage, String> {
        match ty {
            TypeSpec::Scalar(prim) => {
                let val = if read_input {
                    self.scanner.read_token(prim)?
                } else if let TypeRef::Custom(cname) = prim {
                    self.instantiate_class(cname, Vec::new(), out)?
                } else {
                    Value::default_for(prim)
                };
                Ok(VarStorage::Scalar(val))
            }
            TypeSpec::Array1D { elem, len } => {
                let actual_len = match len {
                    Some(e) => self.eval_usize(e, out)?,
                    None => 0,
                };
                let mut row = Vec::with_capacity(actual_len);
                if read_input {
                    for _ in 0..actual_len {
                        row.push(self.scanner.read_token(elem)?);
                    }
                } else if let TypeRef::Custom(cname) = elem {
                    for _ in 0..actual_len {
                        row.push(self.instantiate_class(cname, Vec::new(), out)?);
                    }
                } else {
                    row.resize(actual_len, Value::default_for(elem));
                }
                Ok(VarStorage::Table {
                    row_types: vec![elem.clone()],
                    rows: vec![row],
                    is_fixed: len.is_some(),
                })
            }
            TypeSpec::Table { row_types, len } => {
                let actual_len = match len {
                    Some(e) => self.eval_usize(e, out)?,
                    None => 0,
                };
                if read_input {
                    return Err(
                        "Direct input() on multidimensional table is not supported".to_string()
                    );
                }
                let rows = row_types
                    .iter()
                    .map(|t| vec![Value::default_for(t); actual_len])
                    .collect();
                Ok(VarStorage::Table {
                    row_types: row_types.clone(),
                    rows,
                    is_fixed: len.is_some(),
                })
            }
        }
    }

    fn read_into_expr(&mut self, expr: &Expr, out: &mut dyn Write) -> Result<(), String> {
        match expr {
            Expr::Ident(name) => {
                enum ReadAction {
                    Scalar(TypeRef),
                    Table(Vec<(usize, TypeRef)>),
                }
                let action = match self.lookup_var(name) {
                    Some(VarStorage::Scalar(val)) => {
                        let tr = match val {
                            Value::Int(_) => TypeRef::Int,
                            Value::Uint(_) => TypeRef::Uint,
                            Value::Float(_) => TypeRef::Float,
                            Value::Char(_) => TypeRef::Char,
                            Value::String(_) => TypeRef::String,
                            Value::Bool(_) => TypeRef::Bool,
                            _ => return Err(format!("Cannot read input() into '{}'", name)),
                        };
                        ReadAction::Scalar(tr)
                    }
                    Some(VarStorage::Table {
                        row_types, rows, ..
                    }) => {
                        let plan = rows
                            .iter()
                            .enumerate()
                            .map(|(i, r)| (r.len(), row_types[i].clone()))
                            .collect();
                        ReadAction::Table(plan)
                    }
                    None => return Err(format!("Undefined variable: {}", name)),
                };
                match action {
                    ReadAction::Scalar(tr) => {
                        let new_val = self.scanner.read_token(&tr)?;
                        if let Some(VarStorage::Scalar(slot)) = self.lookup_var_mut(name) {
                            *slot = new_val;
                        }
                    }
                    ReadAction::Table(plan) => {
                        let mut new_rows = Vec::with_capacity(plan.len());
                        for (len, tr) in plan {
                            let mut r = Vec::with_capacity(len);
                            for _ in 0..len {
                                r.push(self.scanner.read_token(&tr)?);
                            }
                            new_rows.push(r);
                        }
                        if let Some(VarStorage::Table { rows, .. }) = self.lookup_var_mut(name) {
                            for (r, nr) in rows.iter_mut().zip(new_rows.into_iter()) {
                                *r = nr;
                            }
                        }
                    }
                }
                Ok(())
            }
            Expr::Index { target, index } => {
                let idx = self.eval_usize(index, out)?;
                if let Expr::Ident(name) = target.as_ref() {
                    let (is_row_of_table, elem_ty, row_len) = match self.lookup_var(name) {
                        Some(VarStorage::Table {
                            row_types, rows, ..
                        }) => {
                            if row_types.len() > 1 {
                                let row = rows
                                    .get(idx)
                                    .ok_or_else(|| format!("Row {} out of bounds in {}", idx, name))?;
                                (true, row_types[idx].clone(), row.len())
                            } else {
                                (false, row_types[0].clone(), rows[0].len())
                            }
                        }
                        _ => return Err(format!("Cannot read input() into indexed '{}'", name)),
                    };
                    if is_row_of_table {
                        let mut new_vals = Vec::with_capacity(row_len);
                        for _ in 0..row_len {
                            new_vals.push(self.scanner.read_token(&elem_ty)?);
                        }
                        if let Some(VarStorage::Table { rows, .. }) = self.lookup_var_mut(name) {
                            rows[idx] = new_vals;
                        }
                    } else {
                        if idx >= row_len {
                            return Err(format!("Index {} out of bounds in {}", idx, name));
                        }
                        let val = self.scanner.read_token(&elem_ty)?;
                        if let Some(VarStorage::Table { rows, .. }) = self.lookup_var_mut(name) {
                            rows[0][idx] = val;
                        }
                    }
                    return Ok(());
                }
                Err("Unsupported input() target".to_string())
            }
            Expr::FieldAccess { target, field } => {
                let obj = self.eval_expr(target, out)?;
                let Value::Instance(rc) = obj else {
                    return Err(format!("Cannot access field '{}' on non-instance", field));
                };
                let tr = {
                    let borrowed = rc.borrow();
                    let cur = borrowed
                        .fields
                        .get(field)
                        .ok_or_else(|| format!("Field '{}' not found", field))?;
                    match cur {
                        Value::Int(_) => TypeRef::Int,
                        Value::Uint(_) => TypeRef::Uint,
                        Value::Float(_) => TypeRef::Float,
                        Value::Char(_) => TypeRef::Char,
                        Value::String(_) => TypeRef::String,
                        Value::Bool(_) => TypeRef::Bool,
                        _ => return Err(format!("Cannot read input() into field '{}'", field)),
                    }
                };
                let new_val = self.scanner.read_token(&tr)?;
                rc.borrow_mut().fields.insert(field.clone(), new_val);
                Ok(())
            }
            _ => Err("Unsupported input() target".to_string()),
        }
    }

    fn apply_assign_target(
        &mut self,
        target: &AssignTarget,
        val: Value,
        out: &mut dyn Write,
    ) -> Result<(), String> {
        match target {
            AssignTarget::Ident(name) => {
                if let Some(storage) = self.lookup_var_mut(name) {
                    let mut dropped_old = None;
                    match storage {
                        VarStorage::Scalar(v) => {
                            let coerced = match v {
                                Value::Uint(_) => match val {
                                    Value::Int(n) if n >= 0 => Value::Uint(n as u64),
                                    other => other,
                                },
                                _ => val,
                            };
                            let old = std::mem::replace(v, coerced);
                            if let Value::Instance(rc) = old {
                                if Rc::strong_count(&rc) == 1 && !rc.borrow().dropped {
                                    dropped_old = Some(rc);
                                }
                            }
                        }
                        _ => return Err(format!("Cannot assign scalar to table {}", name)),
                    }
                    if let Some(rc) = dropped_old {
                        self.drop_instance(rc, out)?;
                    }
                } else {
                    self.insert_current_scope(name.clone(), VarStorage::Scalar(val));
                }
            }
            AssignTarget::Field { target, field } => {
                let obj = self.eval_expr(target, out)?;
                let Value::Instance(rc) = obj else {
                    return Err(format!("Cannot assign field '{}' on non-instance", field));
                };
                let old = {
                    let mut borrowed = rc.borrow_mut();
                    if !borrowed.fields.contains_key(field) {
                        return Err(format!(
                            "Class '{}' has no field '{}'",
                            borrowed.class_name, field
                        ));
                    }
                    let coerced = match borrowed.fields.get(field) {
                        Some(Value::Uint(_)) => match val {
                            Value::Int(n) if n >= 0 => Value::Uint(n as u64),
                            other => other,
                        },
                        _ => val,
                    };
                    borrowed.fields.insert(field.clone(), coerced)
                };
                if let Some(Value::Instance(old_rc)) = old {
                    if Rc::strong_count(&old_rc) == 1 && !old_rc.borrow().dropped {
                        self.drop_instance(old_rc, out)?;
                    }
                }
            }
            AssignTarget::Index { target, index } => {
                let idx = self.eval_usize(index, out)?;
                match target {
                    Expr::Index {
                        target: outer,
                        index: row_expr,
                    } => {
                        if let Expr::Ident(name) = outer.as_ref() {
                            let r_idx = self.eval_usize(row_expr, out)?;
                            let storage = self
                                .lookup_var_mut(name)
                                .ok_or_else(|| format!("Undefined variable: {}", name))?;
                            if let VarStorage::Table {
                                rows, row_types, ..
                            } = storage
                            {
                                let r = rows.get_mut(r_idx).ok_or_else(|| {
                                    format!("Row {} out of bounds in {}", r_idx, name)
                                })?;
                                let coerced = Self::coerce_for_type(val, &row_types[r_idx]);
                                let slot = r.get_mut(idx).ok_or_else(|| {
                                    format!("Index {} out of bounds in {}", idx, name)
                                })?;
                                *slot = coerced;
                                return Ok(());
                            }
                        }
                        return Err("Invalid 2D index assignment target".to_string());
                    }
                    Expr::Ident(name) => {
                        let storage = self
                            .lookup_var_mut(name)
                            .ok_or_else(|| format!("Undefined variable: {}", name))?;
                        match storage {
                            VarStorage::Table {
                                rows, row_types, ..
                            } => {
                                let coerced = Self::coerce_for_type(val, &row_types[0]);
                                let slot = rows[0].get_mut(idx).ok_or_else(|| {
                                    format!("Index {} out of bounds in {}", idx, name)
                                })?;
                                *slot = coerced;
                            }
                            VarStorage::Scalar(_) => {
                                return Err(format!("Cannot index scalar {}", name));
                            }
                        }
                    }
                    Expr::FieldAccess {
                        target: obj_expr,
                        field,
                    } => {
                        let obj = self.eval_expr(obj_expr, out)?;
                        let Value::Instance(rc) = obj else {
                            return Err("Cannot index field on non-instance".to_string());
                        };
                        let mut borrowed = rc.borrow_mut();
                        let field_val = borrowed
                            .fields
                            .get_mut(field)
                            .ok_or_else(|| format!("Unknown field '{}'", field))?;
                        if let Value::Array(arr) = field_val {
                            let slot = arr
                                .get_mut(idx)
                                .ok_or_else(|| format!("Index {} out of bounds", idx))?;
                            *slot = val;
                        } else {
                            return Err(format!("Field '{}' is not an array", field));
                        }
                    }
                    _ => return Err("Unsupported index assignment target".to_string()),
                }
            }
            AssignTarget::Slice { target, start, end } => {
                let s = match start {
                    Some(e) => self.eval_usize(e, out)?,
                    None => 0,
                };
                let e_idx_opt = match end {
                    Some(e) => Some(self.eval_usize(e, out)?),
                    None => None,
                };
                match target {
                    Expr::Index {
                        target: outer,
                        index: row_expr,
                    } => {
                        if let Expr::Ident(name) = outer.as_ref() {
                            let r_idx = self.eval_usize(row_expr, out)?;
                            let storage = self
                                .lookup_var_mut(name)
                                .ok_or_else(|| format!("Undefined variable: {}", name))?;
                            if let VarStorage::Table {
                                rows, row_types, ..
                            } = storage
                            {
                                let r = rows.get_mut(r_idx).ok_or_else(|| {
                                    format!("Row {} out of bounds in {}", r_idx, name)
                                })?;
                                let coerced = Self::coerce_for_type(val, &row_types[r_idx]);
                                let e_idx = e_idx_opt.unwrap_or(r.len()).min(r.len());
                                for slot in r[s..e_idx].iter_mut() {
                                    *slot = coerced.clone();
                                }
                                return Ok(());
                            }
                        }
                        return Err("Invalid 2D slice assignment target".to_string());
                    }
                    Expr::Ident(name) => {
                        let storage = self
                            .lookup_var_mut(name)
                            .ok_or_else(|| format!("Undefined variable: {}", name))?;
                        if let VarStorage::Table {
                            rows, row_types, ..
                        } = storage
                        {
                            let coerced = Self::coerce_for_type(val, &row_types[0]);
                            let e_idx = e_idx_opt.unwrap_or(rows[0].len()).min(rows[0].len());
                            for slot in rows[0][s..e_idx].iter_mut() {
                                *slot = coerced.clone();
                            }
                        } else {
                            return Err(format!("Cannot slice scalar {}", name));
                        }
                    }
                    _ => return Err("Unsupported slice assignment target".to_string()),
                }
            }
        }
        Ok(())
    }

    fn exec_block(&mut self, stmts: &[Stmt], out: &mut dyn Write) -> Result<FlowCtrl, String> {
        self.local_frames.push(ScopeFrame::new());
        let mut ctrl = FlowCtrl::None;
        let mut exec_res = Ok(());
        for stmt in stmts {
            match self.exec_stmt(stmt, out) {
                Ok(c) => {
                    ctrl = c;
                    if ctrl != FlowCtrl::None {
                        break;
                    }
                }
                Err(e) => {
                    exec_res = Err(e);
                    break;
                }
            }
        }
        let popped = self.local_frames.pop().unwrap();
        let cleanup_res = self.cleanup_frame(popped, out);
        exec_res?;
        cleanup_res?;
        Ok(ctrl)
    }

    fn exec_stmt(&mut self, stmt: &Stmt, out: &mut dyn Write) -> Result<FlowCtrl, String> {
        match stmt {
            Stmt::FnDecl(fd) => {
                self.functions.insert(fd.name.clone(), fd.clone());
                Ok(FlowCtrl::None)
            }
            Stmt::ClassDecl(cd) => {
                self.classes.insert(cd.name.clone(), cd.clone());
                Ok(FlowCtrl::None)
            }
            Stmt::VarDecl { names, ty, init } => {
                for name in names {
                    if let Some(Expr::InputCall) = init {
                        let storage = self.create_storage_for_type(ty, true, out)?;
                        self.insert_current_scope(name.clone(), storage);
                    } else if let Some(init_expr) = init {
                        let val = self.eval_expr(init_expr, out)?;
                        let storage = match ty {
                            TypeSpec::Scalar(tr) => {
                                VarStorage::Scalar(Self::coerce_for_type(val, tr))
                            }
                            TypeSpec::Array1D { elem, len } => {
                                let is_fixed = len.is_some();
                                let arr = match val {
                                    Value::Array(items) => items,
                                    other => {
                                        return Err(format!(
                                            "Expected array initializer, got {:?}",
                                            other
                                        ));
                                    }
                                };
                                VarStorage::Table {
                                    row_types: vec![elem.clone()],
                                    rows: vec![arr],
                                    is_fixed,
                                }
                            }
                            TypeSpec::Table { .. } => {
                                return Err("Cannot initialize 2D table with scalar expression"
                                    .to_string());
                            }
                        };
                        self.insert_current_scope(name.clone(), storage);
                    } else {
                        let storage = self.create_storage_for_type(ty, false, out)?;
                        self.insert_current_scope(name.clone(), storage);
                    }
                }
                Ok(FlowCtrl::None)
            }
            Stmt::Assign { target, value } => {
                let val = self.eval_expr(value, out)?;
                self.apply_assign_target(target, val, out)?;
                Ok(FlowCtrl::None)
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
                    evaluated.push(self.eval_expr(expr, out)?);
                }
                for (target, val) in targets.iter().zip(evaluated.into_iter()) {
                    self.apply_assign_target(target, val, out)?;
                }
                Ok(FlowCtrl::None)
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let cond_val = self.eval_expr(cond, out)?;
                if cond_val == Value::Bool(true) {
                    self.exec_block(then_branch, out)
                } else if let Some(else_stmts) = else_branch {
                    self.exec_block(else_stmts, out)
                } else {
                    Ok(FlowCtrl::None)
                }
            }
            Stmt::For {
                var_name,
                var_ty,
                range_start,
                range_end,
                body,
            } => {
                let start_val = self.eval_expr(range_start, out)?;
                let end_val = self.eval_expr(range_end, out)?;
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
                        Some(TypeRef::Uint) => Value::Uint(i as u64),
                        _ => Value::Int(i),
                    };
                    let mut iter_frame = ScopeFrame::new();
                    iter_frame.insert(var_name.clone(), VarStorage::Scalar(loop_val));
                    self.local_frames.push(iter_frame);
                    let mut ctrl = FlowCtrl::None;
                    let mut body_res = Ok(());
                    for s in body {
                        match self.exec_stmt(s, out) {
                            Ok(c) => {
                                ctrl = c;
                                if ctrl != FlowCtrl::None {
                                    break;
                                }
                            }
                            Err(e) => {
                                body_res = Err(e);
                                break;
                            }
                        }
                    }
                    let popped = self.local_frames.pop().unwrap();
                    let cleanup_res = self.cleanup_frame(popped, out);
                    body_res?;
                    cleanup_res?;

                    match ctrl {
                        FlowCtrl::None | FlowCtrl::Continue => continue,
                        FlowCtrl::Break => break,
                        FlowCtrl::Return(v) => return Ok(FlowCtrl::Return(v)),
                    }
                }
                Ok(FlowCtrl::None)
            }
            Stmt::While { cond, body } => {
                loop {
                    let cond_val = self.eval_expr(cond, out)?;
                    if cond_val != Value::Bool(true) {
                        break;
                    }
                    match self.exec_block(body, out)? {
                        FlowCtrl::None | FlowCtrl::Continue => continue,
                        FlowCtrl::Break => break,
                        FlowCtrl::Return(v) => return Ok(FlowCtrl::Return(v)),
                    }
                }
                Ok(FlowCtrl::None)
            }
            Stmt::Break => Ok(FlowCtrl::Break),
            Stmt::Continue => Ok(FlowCtrl::Continue),
            Stmt::Return(expr_opt) => {
                let val = match expr_opt {
                    Some(e) => self.eval_expr(e, out)?,
                    None => Value::Void,
                };
                Ok(FlowCtrl::Return(val))
            }
            Stmt::ExprStmt(expr) => {
                let val = self.eval_expr(expr, out)?;
                if let Value::Instance(rc) = val {
                    if Rc::strong_count(&rc) == 1 && !rc.borrow().dropped {
                        self.drop_instance(rc, out)?;
                    }
                }
                Ok(FlowCtrl::None)
            }
            Stmt::Print { args } => {
                if args.is_empty() {
                    writeln!(out).map_err(|e| e.to_string())?;
                } else {
                    let mut parts = Vec::with_capacity(args.len());
                    for arg in args {
                        let val = self.eval_expr(arg, out)?;
                        parts.push(val.display_str());
                    }
                    writeln!(out, "{}", parts.join(" ")).map_err(|e| e.to_string())?;
                }
                Ok(FlowCtrl::None)
            }
            Stmt::FnInput { targets } => {
                for target in targets {
                    match target {
                        InputTarget::Decl { names, ty } => {
                            for name in names {
                                let storage = self.create_storage_for_type(ty, true, out)?;
                                self.insert_current_scope(name.clone(), storage);
                            }
                        }
                        InputTarget::Var { target } => {
                            self.read_into_expr(target, out)?;
                        }
                    }
                }
                Ok(FlowCtrl::None)
            }
        }
    }

    fn instantiate_class(
        &mut self,
        class_name: &str,
        arg_vals: Vec<Value>,
        out: &mut dyn Write,
    ) -> Result<Value, String> {
        let cd = self
            .classes
            .get(class_name)
            .cloned()
            .ok_or_else(|| format!("Undefined class '{}'", class_name))?;

        let saved_locals = std::mem::take(&mut self.local_frames);
        let mut fields = HashMap::new();
        let mut field_res = Ok(());
        for f in &cd.fields {
            let val = if let Some(def_expr) = &f.default_expr {
                match self.eval_expr(def_expr, out) {
                    Ok(v) => match &f.ty {
                        Some(TypeSpec::Scalar(tr)) => Self::coerce_for_type(v, tr),
                        _ => v,
                    },
                    Err(e) => {
                        field_res = Err(e);
                        break;
                    }
                }
            } else if let Some(ts) = &f.ty {
                match ts {
                    TypeSpec::Scalar(TypeRef::Custom(nested)) => {
                        match self.instantiate_class(nested, Vec::new(), out) {
                            Ok(v) => v,
                            Err(e) => {
                                field_res = Err(e);
                                break;
                            }
                        }
                    }
                    TypeSpec::Scalar(tr) => Value::default_for(tr),
                    TypeSpec::Array1D { elem, len } => {
                        let n = match len {
                            Some(e) => match self.eval_usize(e, out) {
                                Ok(len_val) => len_val,
                                Err(e) => {
                                    field_res = Err(e);
                                    break;
                                }
                            },
                            None => 0,
                        };
                        Value::Array(vec![Value::default_for(elem); n])
                    }
                    TypeSpec::Table { .. } => Value::Array(Vec::new()),
                }
            } else {
                Value::Void
            };
            fields.insert(f.name.clone(), val);
        }
        self.local_frames = saved_locals;
        field_res?;

        let rc = Rc::new(RefCell::new(ClassInstance {
            class_name: cd.name.clone(),
            fields,
            dropped: false,
        }));

        if let Some(init_fn) = cd.methods.iter().find(|m| m.name == "init").cloned() {
            self.call_method_on_rc(&rc, &init_fn, arg_vals, out)?;
        } else if !arg_vals.is_empty() {
            return Err(format!(
                "Class '{}' has no 'init' constructor taking {} arguments",
                class_name,
                arg_vals.len()
            ));
        }

        Ok(Value::Instance(rc))
    }

    fn drop_instance(
        &mut self,
        rc: Rc<RefCell<ClassInstance>>,
        out: &mut dyn Write,
    ) -> Result<(), String> {
        if rc.borrow().dropped {
            return Ok(());
        }
        rc.borrow_mut().dropped = true;
        let class_name = rc.borrow().class_name.clone();
        if let Some(drop_fn) = self
            .classes
            .get(&class_name)
            .and_then(|c| c.methods.iter().find(|m| m.name == "drop"))
            .cloned()
        {
            self.call_method_on_rc(&rc, &drop_fn, Vec::new(), out)?;
        }
        Ok(())
    }

    fn call_method_on_rc(
        &mut self,
        rc: &Rc<RefCell<ClassInstance>>,
        method: &FnDecl,
        arg_vals: Vec<Value>,
        out: &mut dyn Write,
    ) -> Result<Value, String> {
        let mut all_args = Vec::with_capacity(arg_vals.len() + 1);
        all_args.push(Value::Instance(Rc::clone(rc)));
        all_args.extend(arg_vals);
        self.exec_fn_with_args(method, all_args, out)
    }

    fn exec_fn_with_args(
        &mut self,
        fd: &FnDecl,
        arg_vals: Vec<Value>,
        out: &mut dyn Write,
    ) -> Result<Value, String> {
        if arg_vals.len() != fd.params.len() {
            return Err(format!(
                "Function '{}' expects {} arguments, got {}",
                fd.name,
                fd.params.len(),
                arg_vals.len()
            ));
        }

        let mut fn_frame = ScopeFrame::new();
        for (param, val) in fd.params.iter().zip(arg_vals.into_iter()) {
            let coerced = match &param.ty {
                Some(TypeSpec::Scalar(tr)) => Self::coerce_for_type(val, tr),
                _ => val,
            };
            let storage = match (&param.ty, coerced) {
                (Some(TypeSpec::Array1D { elem, len }), Value::Array(arr)) => VarStorage::Table {
                    row_types: vec![elem.clone()],
                    rows: vec![arr],
                    is_fixed: len.is_some(),
                },
                (_, v) => VarStorage::Scalar(v),
            };
            fn_frame.insert(param.name.clone(), storage);
        }

        let saved_locals = std::mem::take(&mut self.local_frames);
        self.local_frames.push(fn_frame);

        let mut ctrl = FlowCtrl::None;
        let mut body_res = Ok(());
        for stmt in &fd.body {
            match self.exec_stmt(stmt, out) {
                Ok(c) => {
                    ctrl = c;
                    if ctrl != FlowCtrl::None {
                        break;
                    }
                }
                Err(e) => {
                    body_res = Err(e);
                    break;
                }
            }
        }

        let popped = self.local_frames.pop().unwrap();
        let cleanup_res = self.cleanup_frame(popped, out);
        self.local_frames = saved_locals;

        body_res?;
        cleanup_res?;

        match ctrl {
            FlowCtrl::None => Ok(Value::Void),
            FlowCtrl::Return(v) => Ok(v),
            FlowCtrl::Break | FlowCtrl::Continue => {
                Err("break/continue used outside of a loop".to_string())
            }
        }
    }

    fn eval_expr(&mut self, expr: &Expr, out: &mut dyn Write) -> Result<Value, String> {
        match expr {
            Expr::Int(v) => Ok(Value::Int(*v)),
            Expr::Float(v) => Ok(Value::Float(*v)),
            Expr::Char(v) => Ok(Value::Char(*v)),
            Expr::Str(v) => Ok(Value::String(v.clone())),
            Expr::Bool(v) => Ok(Value::Bool(*v)),
            Expr::SelfRef => match self.lookup_var("self") {
                Some(VarStorage::Scalar(v)) => Ok(v.clone()),
                _ => Err("'self' used outside of a class method".to_string()),
            },
            Expr::Ident(name) => match self.lookup_var(name) {
                Some(VarStorage::Scalar(v)) => Ok(v.clone()),
                Some(VarStorage::Table { rows, .. }) => {
                    Ok(Value::Array(rows.first().cloned().unwrap_or_default()))
                }
                None => Err(format!("Undefined variable in expression: {}", name)),
            },
            Expr::Unary { op, expr } => {
                let v = self.eval_expr(expr, out)?;
                match (op, v) {
                    (UnaryOp::Neg, Value::Int(x)) => Ok(Value::Int(-x)),
                    (UnaryOp::Neg, Value::Float(x)) => Ok(Value::Float(-x)),
                    (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    _ => Err("Invalid unary operation".to_string()),
                }
            }
            Expr::Binary { op, left, right } => {
                let l = self.eval_expr(left, out)?;
                let r = self.eval_expr(right, out)?;
                eval_binary(*op, l, r)
            }
            Expr::Call { callee, args } => {
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(self.eval_expr(a, out)?);
                }

                if callee == "len" {
                    if arg_vals.len() != 1 {
                        return Err(format!("len() expects 1 argument, got {}", arg_vals.len()));
                    }
                    return match &arg_vals[0] {
                        Value::Array(arr) => Ok(Value::Int(arr.len() as i64)),
                        Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
                        _ => Err("len() expects an array or string".to_string()),
                    };
                }

                if self.classes.contains_key(callee) {
                    return self.instantiate_class(callee, arg_vals, out);
                }

                let fd = self
                    .functions
                    .get(callee)
                    .cloned()
                    .ok_or_else(|| format!("Undefined function '{}'", callee))?;
                self.exec_fn_with_args(&fd, arg_vals, out)
            }
            Expr::FieldAccess { target, field } => {
                let obj = self.eval_expr(target, out)?;
                let Value::Instance(rc) = obj else {
                    return Err(format!("Cannot access field '{}' on non-instance", field));
                };
                let borrowed = rc.borrow();
                borrowed.fields.get(field).cloned().ok_or_else(|| {
                    format!("Class '{}' has no field '{}'", borrowed.class_name, field)
                })
            }
            Expr::MethodCall {
                target,
                method,
                args,
            } => {
                let obj = self.eval_expr(target, out)?;
                let Value::Instance(rc) = obj else {
                    return Err(format!("Cannot call method '{}' on non-instance", method));
                };
                let mut arg_vals = Vec::with_capacity(args.len());
                for a in args {
                    arg_vals.push(self.eval_expr(a, out)?);
                }
                let cname = rc.borrow().class_name.clone();
                let method_fn = self
                    .classes
                    .get(&cname)
                    .and_then(|c| c.methods.iter().find(|m| m.name == *method))
                    .cloned()
                    .ok_or_else(|| format!("Class '{}' has no method '{}'", cname, method))?;
                self.call_method_on_rc(&rc, &method_fn, arg_vals, out)
            }
            Expr::Index { target, index } => {
                let idx = self.eval_usize(index, out)?;
                if let Expr::Index {
                    target: outer,
                    index: row_expr,
                } = target.as_ref()
                {
                    if let Expr::Ident(name) = outer.as_ref() {
                        let r_idx = self.eval_usize(row_expr, out)?;
                        if let Some(VarStorage::Table { rows, .. }) = self.lookup_var(name) {
                            let row = rows.get(r_idx).ok_or_else(|| {
                                format!("Row {} out of bounds in {}", r_idx, name)
                            })?;
                            return row.get(idx).cloned().ok_or_else(|| {
                                format!("Index {} out of bounds in {}[{}]", idx, name, r_idx)
                            });
                        }
                    }
                }
                if let Expr::Ident(name) = target.as_ref() {
                    if let Some(VarStorage::Table { rows, .. }) = self.lookup_var(name) {
                        if rows.len() > 1 {
                            let row = rows.get(idx).ok_or_else(|| {
                                format!("Row {} out of bounds in {}", idx, name)
                            })?;
                            return Ok(Value::Array(row.clone()));
                        }
                    }
                }
                let tval = self.eval_expr(target, out)?;
                match tval {
                    Value::Array(arr) => arr
                        .into_iter()
                        .nth(idx)
                        .ok_or_else(|| format!("Index {} out of bounds", idx)),
                    Value::String(s) => s
                        .chars()
                        .nth(idx)
                        .map(Value::Char)
                        .ok_or_else(|| format!("String index {} out of bounds", idx)),
                    _ => Err("Cannot index non-array/non-string value".to_string()),
                }
            }
            Expr::SliceView { target, start, end } => {
                if let Expr::Index {
                    target: outer,
                    index: row_expr,
                } = target.as_ref()
                {
                    if let Expr::Ident(name) = outer.as_ref() {
                        let r_idx = self.eval_usize(row_expr, out)?;
                        let s = match start {
                            Some(e) => self.eval_usize(e, out)?,
                            None => 0,
                        };
                        let e_opt = match end {
                            Some(e) => Some(self.eval_usize(e, out)?),
                            None => None,
                        };
                        if let Some(VarStorage::Table { rows, .. }) = self.lookup_var(name) {
                            let row = rows.get(r_idx).ok_or_else(|| {
                                format!("Row {} out of bounds in {}", r_idx, name)
                            })?;
                            let e_idx = e_opt.unwrap_or(row.len());
                            let end_clamped = e_idx.min(row.len());
                            let start_clamped = s.min(end_clamped);
                            return Ok(Value::Array(row[start_clamped..end_clamped].to_vec()));
                        }
                    }
                }
                let tval = self.eval_expr(target, out)?;
                let s = match start {
                    Some(e) => self.eval_usize(e, out)?,
                    None => 0,
                };
                match tval {
                    Value::Array(row_data) => {
                        let e_idx = match end {
                            Some(e) => self.eval_usize(e, out)?,
                            None => row_data.len(),
                        };
                        let end_clamped = e_idx.min(row_data.len());
                        let start_clamped = s.min(end_clamped);
                        Ok(Value::Array(row_data[start_clamped..end_clamped].to_vec()))
                    }
                    Value::String(str_val) => {
                        let chars: Vec<char> = str_val.chars().collect();
                        let e_idx = match end {
                            Some(e) => self.eval_usize(e, out)?,
                            None => chars.len(),
                        };
                        let end_clamped = e_idx.min(chars.len());
                        let start_clamped = s.min(end_clamped);
                        Ok(Value::String(chars[start_clamped..end_clamped].iter().collect()))
                    }
                    _ => Err("SliceView target must be an array or string".to_string()),
                }
            }
            Expr::InputCall => Err(
                "InputCall cannot be used in this expression context; use 'N: int = input()' in variable declaration"
                    .to_string(),
            ),
        }
    }
}

pub fn eval_binary(op: BinaryOp, l: Value, r: Value) -> Result<Value, String> {
    let (l, r) = match (l, r) {
        (Value::Uint(a), Value::Int(b)) if b >= 0 => (Value::Uint(a), Value::Uint(b as u64)),
        (Value::Int(a), Value::Uint(b)) if a >= 0 => (Value::Uint(a as u64), Value::Uint(b)),
        other => other,
    };

    match (op, l, r) {
        (BinaryOp::Add, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
        (BinaryOp::Sub, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
        (BinaryOp::Mul, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
        (BinaryOp::Div, Value::Int(_), Value::Int(0)) => Err("Division by zero".to_string()),
        (BinaryOp::Div, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
        (BinaryOp::Mod, Value::Int(_), Value::Int(0)) => Err("Modulo by zero".to_string()),
        (BinaryOp::Mod, Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),

        (BinaryOp::Add, Value::Uint(a), Value::Uint(b)) => Ok(Value::Uint(a + b)),
        (BinaryOp::Sub, Value::Uint(a), Value::Uint(b)) => Ok(Value::Uint(a - b)),
        (BinaryOp::Mul, Value::Uint(a), Value::Uint(b)) => Ok(Value::Uint(a * b)),
        (BinaryOp::Div, Value::Uint(_), Value::Uint(0)) => Err("Division by zero".to_string()),
        (BinaryOp::Div, Value::Uint(a), Value::Uint(b)) => Ok(Value::Uint(a / b)),
        (BinaryOp::Mod, Value::Uint(_), Value::Uint(0)) => Err("Modulo by zero".to_string()),
        (BinaryOp::Mod, Value::Uint(a), Value::Uint(b)) => Ok(Value::Uint(a % b)),

        (BinaryOp::Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (BinaryOp::Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (BinaryOp::Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (BinaryOp::Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),

        (BinaryOp::Add, Value::String(a), Value::String(b)) => Ok(Value::String(a + &b)),

        (BinaryOp::Eq, a, b) => Ok(Value::Bool(a == b)),
        (BinaryOp::Neq, a, b) => Ok(Value::Bool(a != b)),

        (BinaryOp::Lt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a < b)),
        (BinaryOp::Gt, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a > b)),
        (BinaryOp::Le, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a <= b)),
        (BinaryOp::Ge, Value::Int(a), Value::Int(b)) => Ok(Value::Bool(a >= b)),

        (BinaryOp::Lt, Value::Uint(a), Value::Uint(b)) => Ok(Value::Bool(a < b)),
        (BinaryOp::Gt, Value::Uint(a), Value::Uint(b)) => Ok(Value::Bool(a > b)),
        (BinaryOp::Le, Value::Uint(a), Value::Uint(b)) => Ok(Value::Bool(a <= b)),
        (BinaryOp::Ge, Value::Uint(a), Value::Uint(b)) => Ok(Value::Bool(a >= b)),

        (BinaryOp::Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
        (BinaryOp::Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
        (BinaryOp::Le, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
        (BinaryOp::Ge, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),

        (BinaryOp::Lt, Value::Char(a), Value::Char(b)) => Ok(Value::Bool(a < b)),
        (BinaryOp::Gt, Value::Char(a), Value::Char(b)) => Ok(Value::Bool(a > b)),
        (BinaryOp::Le, Value::Char(a), Value::Char(b)) => Ok(Value::Bool(a <= b)),
        (BinaryOp::Ge, Value::Char(a), Value::Char(b)) => Ok(Value::Bool(a >= b)),

        (BinaryOp::Lt, Value::String(a), Value::String(b)) => Ok(Value::Bool(a < b)),
        (BinaryOp::Gt, Value::String(a), Value::String(b)) => Ok(Value::Bool(a > b)),
        (BinaryOp::Le, Value::String(a), Value::String(b)) => Ok(Value::Bool(a <= b)),
        (BinaryOp::Ge, Value::String(a), Value::String(b)) => Ok(Value::Bool(a >= b)),

        (BinaryOp::And, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a && b)),
        (BinaryOp::Or, Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a || b)),

        (op, l, r) => Err(format!("Type mismatch for {:?}: {:?} and {:?}", op, l, r)),
    }
}
