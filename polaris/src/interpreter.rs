use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Cursor, Write};
use std::rc::Rc;

use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::module::ModuleProgram;
use crate::value::{ClassInstance, RefTarget, RuntimeType, SliceView, Value, ViewStorage};

#[derive(Debug, Clone)]
enum EvalSelector {
    Index(usize, Span),
    Range {
        start: usize,
        end: Option<usize>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
struct ScopeFrame {
    vars: HashMap<String, Rc<RefCell<Value>>>,
    order: Vec<String>,
}

impl ScopeFrame {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
            order: Vec::new(),
        }
    }

    fn insert_new(&mut self, name: String, val: Value) {
        if !self.vars.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.vars.insert(name, Rc::new(RefCell::new(val)));
    }
}

#[derive(Debug, Clone, PartialEq)]
enum FlowCtrl {
    None,
    Break,
    Continue,
    Return(Value),
}

fn runtime_type_from_value(v: &Value) -> RuntimeType {
    match v {
        Value::Int(_) => RuntimeType::Int,
        Value::Uint(_) => RuntimeType::Uint,
        Value::Float(_) => RuntimeType::Float,
        Value::Char(_) => RuntimeType::Char,
        Value::String(_) => RuntimeType::String,
        Value::Bool(_) => RuntimeType::Bool,
        Value::Array(rc) => {
            let inner = rc
                .borrow()
                .first()
                .map(runtime_type_from_value)
                .unwrap_or(RuntimeType::Int);
            RuntimeType::Array(Box::new(inner))
        }
        Value::SliceView(sv) => {
            let inner = sv
                .to_vec()
                .first()
                .map(runtime_type_from_value)
                .unwrap_or(RuntimeType::Int);
            RuntimeType::Array(Box::new(inner))
        }
        Value::Table(rows) => {
            let rt = rows
                .iter()
                .map(|r| {
                    r.borrow()
                        .first()
                        .map(runtime_type_from_value)
                        .unwrap_or(RuntimeType::Int)
                })
                .collect();
            RuntimeType::Table(rt)
        }
        Value::Instance(rc) => {
            let b = rc.borrow();
            RuntimeType::Class {
                module_id: b.module_id.clone(),
                name: b.class_name.clone(),
                type_args: b.type_args.clone(),
            }
        }
        Value::Ref(target) => target
            .read()
            .map(|rv| runtime_type_from_value(&rv))
            .unwrap_or(RuntimeType::Void),
        Value::Void => RuntimeType::Void,
    }
}

struct ModuleRuntime {
    display_name: String,
    imports: HashMap<String, String>,
    globals: ScopeFrame,
    functions: HashMap<String, FnDecl>,
    classes: HashMap<String, ClassDecl>,
}

impl ModuleRuntime {
    fn new(display_name: String, imports: HashMap<String, String>) -> Self {
        Self {
            display_name,
            imports,
            globals: ScopeFrame::new(),
            functions: HashMap::new(),
            classes: HashMap::new(),
        }
    }
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

    fn parse_token_as(tok: &str, ty: &RuntimeType) -> Result<Value, String> {
        match ty {
            RuntimeType::Int => tok
                .parse::<i64>()
                .map(Value::Int)
                .map_err(|e| format!("Failed to parse int '{}': {}", tok, e)),
            RuntimeType::Uint => tok
                .parse::<u64>()
                .map(Value::Uint)
                .map_err(|e| format!("Failed to parse uint '{}': {}", tok, e)),
            RuntimeType::Float => tok
                .parse::<f64>()
                .map(Value::Float)
                .map_err(|e| format!("Failed to parse float '{}': {}", tok, e)),
            RuntimeType::Char => {
                let mut chars = tok.chars();
                let ch = chars
                    .next()
                    .ok_or_else(|| "Empty token for char".to_string())?;
                if chars.next().is_some() {
                    return Err(format!("Expected single char, got '{}'", tok));
                }
                Ok(Value::Char(ch))
            }
            RuntimeType::String => Ok(Value::String(tok.to_string())),
            RuntimeType::Bool => tok
                .parse::<bool>()
                .map(Value::Bool)
                .map_err(|e| format!("Failed to parse bool '{}': {}", tok, e)),
            _ => Err("Cannot parse input token for complex type".to_string()),
        }
    }

    fn read_token(&mut self, ty: &RuntimeType) -> Result<Value, String> {
        let tok = self.next_token()?;
        Self::parse_token_as(&tok, ty)
    }
}

pub struct Interpreter {
    modules: HashMap<String, ModuleRuntime>,
    current_module: String,
    local_frames: Vec<ScopeFrame>,
    type_bindings: HashMap<String, RuntimeType>,
    scanner: InputScanner,
}

impl Interpreter {
    pub fn new(input: &str) -> Self {
        let mut modules = HashMap::new();
        modules.insert(
            "__main__".to_string(),
            ModuleRuntime::new("<main>".to_string(), HashMap::new()),
        );
        Self {
            modules,
            current_module: "__main__".to_string(),
            local_frames: Vec::new(),
            type_bindings: HashMap::new(),
            scanner: InputScanner::from_str(input),
        }
    }

    pub fn from_stdin() -> Self {
        let mut modules = HashMap::new();
        modules.insert(
            "__main__".to_string(),
            ModuleRuntime::new("<main>".to_string(), HashMap::new()),
        );
        Self {
            modules,
            current_module: "__main__".to_string(),
            local_frames: Vec::new(),
            type_bindings: HashMap::new(),
            scanner: InputScanner::from_stdin(),
        }
    }

    fn err(&self, span: Span, msg: impl Into<String>) -> Diagnostic {
        let file = self
            .modules
            .get(&self.current_module)
            .map(|m| m.display_name.clone())
            .unwrap_or_else(|| "<main>".to_string());
        Diagnostic::new("runtime", span, msg).with_file(file)
    }

    fn current_mod(&self) -> &ModuleRuntime {
        self.modules
            .get(&self.current_module)
            .expect("current module exists")
    }

    fn current_mod_mut(&mut self) -> &mut ModuleRuntime {
        self.modules
            .get_mut(&self.current_module)
            .expect("current module exists")
    }

    fn mod_ref(&self, mod_id: &str) -> &ModuleRuntime {
        self.modules.get(mod_id).expect("module exists")
    }

    fn mod_mut(&mut self, mod_id: &str) -> &mut ModuleRuntime {
        self.modules.get_mut(mod_id).expect("module exists")
    }

    pub fn run(&mut self, stmts: &[Stmt], out: &mut dyn Write) -> Result<(), Diagnostic> {
        self.modules.clear();
        self.modules.insert(
            "__main__".to_string(),
            ModuleRuntime::new("<main>".to_string(), HashMap::new()),
        );
        self.current_module = "__main__".to_string();

        let run_res = self.exec_module_stmts("__main__", stmts, out);
        let frame = std::mem::replace(&mut self.mod_mut("__main__").globals, ScopeFrame::new());
        let cleanup_res = self.cleanup_frame(frame, out);
        run_res?;
        cleanup_res?;
        Ok(())
    }

    pub fn run_module_program(
        &mut self,
        program: &ModuleProgram,
        out: &mut dyn Write,
    ) -> Result<(), Diagnostic> {
        self.modules.clear();
        for (id, lm) in &program.modules {
            self.modules.insert(
                id.clone(),
                ModuleRuntime::new(lm.display_name.clone(), lm.imports.clone()),
            );
        }

        let mut run_res = Ok(());
        for mod_id in &program.load_order {
            let stmts = &program.modules[mod_id].stmts;
            if let Err(e) = self.exec_module_stmts(mod_id, stmts, out) {
                run_res = Err(e);
                break;
            }
        }

        let mut cleanup_res = Ok(());
        for mod_id in program.load_order.iter().rev() {
            let prev = std::mem::replace(&mut self.current_module, mod_id.clone());
            let frame = std::mem::replace(&mut self.mod_mut(mod_id).globals, ScopeFrame::new());
            if let Err(e) = self.cleanup_frame(frame, out) {
                if cleanup_res.is_ok() {
                    cleanup_res = Err(e);
                }
            }
            self.current_module = prev;
        }

        run_res?;
        cleanup_res?;
        Ok(())
    }

    fn exec_module_stmts(
        &mut self,
        mod_id: &str,
        stmts: &[Stmt],
        out: &mut dyn Write,
    ) -> Result<(), Diagnostic> {
        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let saved_locals = std::mem::take(&mut self.local_frames);

        for stmt in stmts {
            match &stmt.node {
                StmtKind::FnDecl(fd) => {
                    self.current_mod_mut()
                        .functions
                        .insert(fd.name.clone(), fd.clone());
                }
                StmtKind::ClassDecl(cd) => {
                    self.current_mod_mut()
                        .classes
                        .insert(cd.name.clone(), cd.clone());
                }
                _ => {}
            }
        }

        let mut run_res = Ok(());
        for stmt in stmts {
            if matches!(
                stmt.node,
                StmtKind::FnDecl(_) | StmtKind::ClassDecl(_) | StmtKind::Import { .. }
            ) {
                continue;
            }
            match self.exec_stmt(stmt, out) {
                Ok(FlowCtrl::None) => {}
                Ok(FlowCtrl::Break | FlowCtrl::Continue) => {
                    run_res = Err(self.err(stmt.span, "break/continue used outside of a loop"));
                    break;
                }
                Ok(FlowCtrl::Return(_)) => {
                    run_res = Err(self.err(stmt.span, "return used outside of a function"));
                    break;
                }
                Err(e) => {
                    run_res = Err(e);
                    break;
                }
            }
        }

        self.local_frames = saved_locals;
        self.current_module = prev_mod;
        run_res
    }

    fn lookup_var_cell(&self, name: &str) -> Option<Rc<RefCell<Value>>> {
        for frame in self.local_frames.iter().rev() {
            if let Some(cell) = frame.vars.get(name) {
                return Some(Rc::clone(cell));
            }
        }
        self.current_mod().globals.vars.get(name).cloned()
    }

    fn insert_current_scope(&mut self, name: String, val: Value) {
        if let Some(frame) = self.local_frames.last_mut() {
            frame.insert_new(name, val);
        } else {
            self.current_mod_mut().globals.insert_new(name, val);
        }
    }

    fn cleanup_value(&mut self, val: Value, out: &mut dyn Write) -> Result<(), Diagnostic> {
        match val {
            Value::Instance(rc) => {
                if Rc::strong_count(&rc) == 1 && !rc.borrow().dropped {
                    self.drop_instance(rc, out)?;
                }
            }
            Value::Array(rc) => {
                if Rc::strong_count(&rc) == 1 {
                    let items: Vec<Value> = {
                        let mut borrowed = rc.borrow_mut();
                        std::mem::take(&mut *borrowed)
                    };
                    for item in items.into_iter().rev() {
                        self.cleanup_value(item, out)?;
                    }
                }
            }
            Value::Table(rows) => {
                for row_rc in rows.into_iter().rev() {
                    if Rc::strong_count(&row_rc) == 1 {
                        let items: Vec<Value> = {
                            let mut borrowed = row_rc.borrow_mut();
                            std::mem::take(&mut *borrowed)
                        };
                        for item in items.into_iter().rev() {
                            self.cleanup_value(item, out)?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn cleanup_frame(&mut self, mut frame: ScopeFrame, out: &mut dyn Write) -> Result<(), Diagnostic> {
        let names: Vec<String> = frame.order.drain(..).rev().collect();
        for name in names {
            if let Some(cell) = frame.vars.remove(&name) {
                let val = {
                    let mut b = cell.borrow_mut();
                    std::mem::replace(&mut *b, Value::Void)
                };
                self.cleanup_value(val, out)?;
            }
        }
        Ok(())
    }

    fn eval_usize(&mut self, expr: &Expr, out: &mut dyn Write) -> Result<usize, Diagnostic> {
        let val = self.eval_expr(expr, out)?;
        match val {
            Value::Int(n) if n >= 0 => Ok(n as usize),
            Value::Uint(n) => Ok(n as usize),
            other => Err(self.err(
                expr.span,
                format!("Expected non-negative integer, got '{}'", other.display_str()),
            )),
        }
    }

    fn coerce_for_runtime_type(val: Value, ty: &RuntimeType) -> Value {
        match (ty, val) {
            (RuntimeType::Uint, Value::Int(n)) if n >= 0 => Value::Uint(n as u64),
            (_, v) => v,
        }
    }

    fn coerce_matching_existing(new_val: Value, existing: &Value) -> Value {
        match (existing, new_val) {
            (Value::Uint(_), Value::Int(n)) if n >= 0 => Value::Uint(n as u64),
            (_, v) => v,
        }
    }

    fn resolve_runtime_type(
        &mut self,
        te: &TypeExpr,
        out: &mut dyn Write,
    ) -> Result<RuntimeType, Diagnostic> {
        match &te.node {
            TypeExprKind::Ref(inner) => self.resolve_runtime_type(inner, out),
            TypeExprKind::Array { elem, .. } => {
                let et = self.resolve_runtime_type(elem, out)?;
                Ok(RuntimeType::Array(Box::new(et)))
            }
            TypeExprKind::Table { row_types, .. } => {
                let mut rows = Vec::with_capacity(row_types.len());
                for rt in row_types {
                    rows.push(self.resolve_runtime_type(rt, out)?);
                }
                Ok(RuntimeType::Table(rows))
            }
            TypeExprKind::Named {
                module,
                name,
                type_args,
            } => {
                if module.is_none() {
                    if let Some(bound) = self.type_bindings.get(name) {
                        return Ok(bound.clone());
                    }
                    match name.as_str() {
                        "int" => return Ok(RuntimeType::Int),
                        "uint" => return Ok(RuntimeType::Uint),
                        "float" => return Ok(RuntimeType::Float),
                        "char" => return Ok(RuntimeType::Char),
                        "string" => return Ok(RuntimeType::String),
                        "bool" => return Ok(RuntimeType::Bool),
                        "void" => return Ok(RuntimeType::Void),
                        _ => {}
                    }
                }
                let target_mod = match module {
                    Some(alias) => self
                        .current_mod()
                        .imports
                        .get(alias)
                        .cloned()
                        .ok_or_else(|| self.err(te.span, format!("Unknown module '{}'", alias)))?,
                    None => self.current_module.clone(),
                };
                let mut r_args = Vec::with_capacity(type_args.len());
                for ta in type_args {
                    r_args.push(self.resolve_runtime_type(ta, out)?);
                }
                Ok(RuntimeType::Class {
                    module_id: target_mod,
                    name: name.clone(),
                    type_args: r_args,
                })
            }
        }
    }

    fn default_value_for_runtime_type(
        &mut self,
        ty: &RuntimeType,
        span: Span,
        out: &mut dyn Write,
    ) -> Result<Value, Diagnostic> {
        match ty {
            RuntimeType::Int => Ok(Value::Int(0)),
            RuntimeType::Uint => Ok(Value::Uint(0)),
            RuntimeType::Float => Ok(Value::Float(0.0)),
            RuntimeType::Char => Ok(Value::Char('\0')),
            RuntimeType::String => Ok(Value::String(String::new())),
            RuntimeType::Bool => Ok(Value::Bool(false)),
            RuntimeType::Void => Ok(Value::Void),
            RuntimeType::Array(_) => Ok(Value::Array(Rc::new(RefCell::new(Vec::new())))),
            RuntimeType::Table(rows) => {
                let row_bufs = rows
                    .iter()
                    .map(|_| Rc::new(RefCell::new(Vec::new())))
                    .collect();
                Ok(Value::Table(row_bufs))
            }
            RuntimeType::Class {
                module_id,
                name,
                type_args,
            } => self.instantiate_class_resolved(module_id, name, type_args.clone(), Vec::new(), span, out),
        }
    }

    fn create_value_for_type_expr(
        &mut self,
        te: &TypeExpr,
        read_input: bool,
        out: &mut dyn Write,
    ) -> Result<Value, Diagnostic> {
        match &te.node {
            TypeExprKind::Ref(_) => Err(self.err(te.span, "Cannot default-create a reference")),
            TypeExprKind::Array { elem, len } => {
                let actual_len = match len {
                    Some(e) => self.eval_usize(e, out)?,
                    None => 0,
                };
                let mut items = Vec::with_capacity(actual_len);
                if read_input {
                    let elem_rt = self.resolve_runtime_type(elem, out)?;
                    for _ in 0..actual_len {
                        let v = self
                            .scanner
                            .read_token(&elem_rt)
                            .map_err(|msg| self.err(te.span, msg))?;
                        items.push(v);
                    }
                } else {
                    for _ in 0..actual_len {
                        items.push(self.create_value_for_type_expr(elem, false, out)?);
                    }
                }
                Ok(Value::Array(Rc::new(RefCell::new(items))))
            }
            TypeExprKind::Table { row_types, len } => {
                if read_input {
                    return Err(self.err(
                        te.span,
                        "Direct input() initialization on 2D table is not supported",
                    ));
                }
                let actual_len = match len {
                    Some(e) => self.eval_usize(e, out)?,
                    None => 0,
                };
                let mut rows = Vec::with_capacity(row_types.len());
                for rt in row_types {
                    let mut row = Vec::with_capacity(actual_len);
                    for _ in 0..actual_len {
                        row.push(self.create_value_for_type_expr(rt, false, out)?);
                    }
                    rows.push(Rc::new(RefCell::new(row)));
                }
                Ok(Value::Table(rows))
            }
            TypeExprKind::Named { .. } => {
                let rt = self.resolve_runtime_type(te, out)?;
                if read_input {
                    self.scanner
                        .read_token(&rt)
                        .map_err(|msg| self.err(te.span, msg))
                } else {
                    self.default_value_for_runtime_type(&rt, te.span, out)
                }
            }
        }
    }

    fn write_to_ref_target(
        &mut self,
        target: &RefTarget,
        val: Value,
        span: Span,
        out: &mut dyn Write,
    ) -> Result<(), Diagnostic> {
        match target {
            RefTarget::Var(cell) => {
                let inner_is_ref = matches!(&*cell.borrow(), Value::Ref(_));
                if inner_is_ref {
                    let nested = match &*cell.borrow() {
                        Value::Ref(rt) => rt.clone(),
                        _ => unreachable!(),
                    };
                    return self.write_to_ref_target(&nested, val, span, out);
                }
                let coerced = {
                    let existing = cell.borrow();
                    Self::coerce_matching_existing(val, &existing)
                };
                let old = {
                    let mut b = cell.borrow_mut();
                    std::mem::replace(&mut *b, coerced)
                };
                self.cleanup_value(old, out)
            }
            RefTarget::Field { instance, field } => {
                let coerced = {
                    let b = instance.borrow();
                    let existing = b.fields.get(field).ok_or_else(|| {
                        self.err(span, format!("Class '{}' has no field '{}'", b.class_name, field))
                    })?;
                    Self::coerce_matching_existing(val, existing)
                };
                let old = {
                    let mut b = instance.borrow_mut();
                    b.fields.insert(field.clone(), coerced)
                };
                if let Some(old_val) = old {
                    self.cleanup_value(old_val, out)?;
                }
                Ok(())
            }
            RefTarget::Index { buffer, index } => {
                let coerced = {
                    let b = buffer.borrow();
                    let existing = b.get(*index).ok_or_else(|| {
                        self.err(span, format!("Referenced index {} out of bounds", index))
                    })?;
                    Self::coerce_matching_existing(val, existing)
                };
                let old = {
                    let mut b = buffer.borrow_mut();
                    if *index >= b.len() {
                        return Err(self.err(span, format!("Referenced index {} out of bounds", index)));
                    }
                    std::mem::replace(&mut b[*index], coerced)
                };
                self.cleanup_value(old, out)
            }
        }
    }

    fn read_into_expr(&mut self, expr: &Expr, out: &mut dyn Write) -> Result<(), Diagnostic> {
        match &expr.node {
            ExprKind::Ident(name) => {
                let cell = self.lookup_var_cell(name).ok_or_else(|| {
                    self.err(expr.span, format!("Undefined variable '{}'", name))
                })?;
                let cur = cell.borrow().deref_read().map_err(|m| self.err(expr.span, m))?;
                match cur {
                    Value::Array(arr_rc) => {
                        let plan: Vec<RuntimeType> = arr_rc
                            .borrow()
                            .iter()
                            .map(runtime_type_from_value)
                            .collect();
                        let mut new_vals = Vec::with_capacity(plan.len());
                        for rt in plan {
                            new_vals.push(
                                self.scanner
                                    .read_token(&rt)
                                    .map_err(|m| self.err(expr.span, m))?,
                            );
                        }
                        *arr_rc.borrow_mut() = new_vals;
                        Ok(())
                    }
                    Value::Table(rows) => {
                        for row_rc in rows {
                            let plan: Vec<RuntimeType> = row_rc
                                .borrow()
                                .iter()
                                .map(runtime_type_from_value)
                                .collect();
                            let mut new_row = Vec::with_capacity(plan.len());
                            for rt in plan {
                                new_row.push(
                                    self.scanner
                                        .read_token(&rt)
                                        .map_err(|m| self.err(expr.span, m))?,
                                );
                            }
                            *row_rc.borrow_mut() = new_row;
                        }
                        Ok(())
                    }
                    scalar => {
                        let rt = runtime_type_from_value(&scalar);
                        let new_val = self
                            .scanner
                            .read_token(&rt)
                            .map_err(|m| self.err(expr.span, m))?;
                        let is_ref = matches!(&*cell.borrow(), Value::Ref(_));
                        if is_ref {
                            let target = match &*cell.borrow() {
                                Value::Ref(t) => t.clone(),
                                _ => unreachable!(),
                            };
                            self.write_to_ref_target(&target, new_val, expr.span, out)
                        } else {
                            *cell.borrow_mut() = new_val;
                            Ok(())
                        }
                    }
                }
            }
            ExprKind::Index { target, index } => {
                let idx = self.eval_usize(index, out)?;
                let tval = self.eval_expr(target, out)?;
                match tval {
                    Value::Table(rows) => {
                        let row_rc = rows.get(idx).cloned().ok_or_else(|| {
                            self.err(index.span, format!("Row {} out of bounds", idx))
                        })?;
                        let plan: Vec<RuntimeType> = row_rc
                            .borrow()
                            .iter()
                            .map(runtime_type_from_value)
                            .collect();
                        let mut new_row = Vec::with_capacity(plan.len());
                        for rt in plan {
                            new_row.push(
                                self.scanner
                                    .read_token(&rt)
                                    .map_err(|m| self.err(expr.span, m))?,
                            );
                        }
                        *row_rc.borrow_mut() = new_row;
                        Ok(())
                    }
                    Value::Array(rc) => {
                        let rt = {
                            let b = rc.borrow();
                            let slot = b.get(idx).ok_or_else(|| {
                                self.err(index.span, format!("Index {} out of bounds", idx))
                            })?;
                            runtime_type_from_value(slot)
                        };
                        let new_val = self
                            .scanner
                            .read_token(&rt)
                            .map_err(|m| self.err(expr.span, m))?;
                        rc.borrow_mut()[idx] = new_val;
                        Ok(())
                    }
                    Value::SliceView(sv) => {
                        let len = sv.len();
                        let (buf_rc, abs_idx) = sv.get_slot(idx).ok_or_else(|| {
                            self.err(
                                index.span,
                                format!("Slice index {} out of bounds (len {})", idx, len),
                            )
                        })?;
                        let rt = runtime_type_from_value(&buf_rc.borrow()[abs_idx]);
                        let new_val = self
                            .scanner
                            .read_token(&rt)
                            .map_err(|m| self.err(expr.span, m))?;
                        buf_rc.borrow_mut()[abs_idx] = new_val;
                        Ok(())
                    }
                    _ => Err(self.err(expr.span, "Unsupported input() index target")),
                }
            }
            ExprKind::MultiIndex { target, selectors } => {
                if selectors
                    .iter()
                    .any(|s| matches!(s, SliceSelector::Range { .. }))
                {
                    return Err(self.err(expr.span, "Cannot read input() directly into a slice range"));
                }
                let eval_sels = self.eval_selectors(selectors, out)?;
                let tval = self.eval_expr(target, out)?;
                let (buf_rc, abs_idx) = self
                    .resolve_slot(tval, &eval_sels, expr.span)?
                    .ok_or_else(|| self.err(expr.span, "Unsupported input() target"))?;
                let rt = runtime_type_from_value(&buf_rc.borrow()[abs_idx]);
                let new_val = self
                    .scanner
                    .read_token(&rt)
                    .map_err(|m| self.err(expr.span, m))?;
                buf_rc.borrow_mut()[abs_idx] = new_val;
                Ok(())
            }
            ExprKind::FieldAccess { target, field } => {
                let obj = self.eval_expr(target, out)?;
                let Value::Instance(rc) = obj else {
                    return Err(self.err(
                        expr.span,
                        format!("Cannot access field '{}' on non-instance", field),
                    ));
                };
                let rt = {
                    let b = rc.borrow();
                    let cur = b.fields.get(field).ok_or_else(|| {
                        self.err(expr.span, format!("Field '{}' not found", field))
                    })?;
                    runtime_type_from_value(cur)
                };
                let new_val = self
                    .scanner
                    .read_token(&rt)
                    .map_err(|m| self.err(expr.span, m))?;
                rc.borrow_mut().fields.insert(field.clone(), new_val);
                Ok(())
            }
            _ => Err(self.err(expr.span, "Unsupported input() target")),
        }
    }

    fn eval_selectors(
        &mut self,
        selectors: &[SliceSelector],
        out: &mut dyn Write,
    ) -> Result<Vec<EvalSelector>, Diagnostic> {
        let mut res = Vec::with_capacity(selectors.len());
        for sel in selectors {
            match sel {
                SliceSelector::Index(e) => {
                    let idx = self.eval_usize(e, out)?;
                    res.push(EvalSelector::Index(idx, e.span));
                }
                SliceSelector::Range { start, end } => {
                    let s = match start {
                        Some(e) => self.eval_usize(e, out)?,
                        None => 0,
                    };
                    let e_opt = match end {
                        Some(e) => Some(self.eval_usize(e, out)?),
                        None => None,
                    };
                    let span = start
                        .as_ref()
                        .map(|e| e.span)
                        .or_else(|| end.as_ref().map(|e| e.span))
                        .unwrap_or_default();
                    res.push(EvalSelector::Range {
                        start: s,
                        end: e_opt,
                        span,
                    });
                }
            }
        }
        Ok(res)
    }

    fn apply_selectors(
        &self,
        val: Value,
        selectors: &[EvalSelector],
        root_span: Span,
    ) -> Result<Value, Diagnostic> {
        if selectors.is_empty() {
            return Ok(val);
        }
        let (head, tail) = (&selectors[0], &selectors[1..]);
        match (head, val) {
            (EvalSelector::Index(idx, idx_span), Value::Array(rc)) => {
                let elem = {
                    let b = rc.borrow();
                    b.get(*idx).cloned().ok_or_else(|| {
                        self.err(
                            *idx_span,
                            format!("Index {} out of bounds (len {})", idx, b.len()),
                        )
                    })?
                };
                self.apply_selectors(elem, tail, root_span)
            }
            (EvalSelector::Index(idx, idx_span), Value::SliceView(sv)) => {
                let len = sv.len();
                let elem = sv.get(*idx).ok_or_else(|| {
                    self.err(
                        *idx_span,
                        format!("Slice index {} out of bounds (len {})", idx, len),
                    )
                })?;
                self.apply_selectors(elem, tail, root_span)
            }
            (EvalSelector::Index(idx, idx_span), Value::Table(rows)) => {
                let row_rc = rows.get(*idx).cloned().ok_or_else(|| {
                    self.err(
                        *idx_span,
                        format!("Row {} out of bounds (rows {})", idx, rows.len()),
                    )
                })?;
                self.apply_selectors(Value::Array(row_rc), tail, root_span)
            }
            (EvalSelector::Index(idx, idx_span), Value::String(s)) => {
                if !tail.is_empty() {
                    return Err(self.err(root_span, "Cannot index further into a character"));
                }
                let ch = s.chars().nth(*idx).ok_or_else(|| {
                    self.err(*idx_span, format!("String index {} out of bounds", idx))
                })?;
                Ok(Value::Char(ch))
            }
            (EvalSelector::Range { start, end, .. }, Value::Array(rc)) => {
                let elems: Vec<Value> = {
                    let b = rc.borrow();
                    let len = b.len();
                    let e_clamped = end.unwrap_or(len).min(len);
                    let s_clamped = (*start).min(e_clamped);
                    if tail.is_empty() {
                        drop(b);
                        return Ok(Value::SliceView(SliceView::contiguous(
                            Rc::clone(&rc),
                            s_clamped,
                            e_clamped,
                        )));
                    }
                    b[s_clamped..e_clamped].to_vec()
                };
                self.project_elems_over_tail(elems, tail, root_span)
            }
            (EvalSelector::Range { start, end, .. }, Value::SliceView(sv)) => {
                if tail.is_empty() {
                    return Ok(Value::SliceView(sv.slice(*start, *end)));
                }
                let len = sv.len();
                let e_clamped = end.unwrap_or(len).min(len);
                let s_clamped = (*start).min(e_clamped);
                let mut elems = Vec::with_capacity(e_clamped.saturating_sub(s_clamped));
                for i in s_clamped..e_clamped {
                    let el = sv.get(i).ok_or_else(|| {
                        self.err(root_span, "Slice underlying index out of bounds")
                    })?;
                    elems.push(el);
                }
                self.project_elems_over_tail(elems, tail, root_span)
            }
            (EvalSelector::Range { span, .. }, _) => {
                Err(self.err(*span, "Slice range selector requires an array or slice view"))
            }
            (EvalSelector::Index(_, idx_span), _) => {
                Err(self.err(*idx_span, "Cannot index non-array value"))
            }
        }
    }

    fn project_elems_over_tail(
        &self,
        elems: Vec<Value>,
        tail: &[EvalSelector],
        root_span: Span,
    ) -> Result<Value, Diagnostic> {
        let tail_has_range = tail
            .iter()
            .any(|s| matches!(s, EvalSelector::Range { .. }));
        if !tail_has_range {
            let mut slots = Vec::with_capacity(elems.len());
            let mut nested_views = Vec::new();
            let mut use_nested = false;
            for elem in elems {
                if !use_nested {
                    if let Some(slot) = self.resolve_slot(elem.clone(), tail, root_span)? {
                        slots.push(slot);
                        continue;
                    } else {
                        use_nested = true;
                    }
                }
                let sub = self.apply_selectors(elem, tail, root_span)?;
                match sub {
                    Value::SliceView(sv) => nested_views.push(sv),
                    _ => {
                        return Err(self.err(
                            root_span,
                            "Expected slice view while projecting multidimensional slice",
                        ));
                    }
                }
            }
            if use_nested {
                Ok(Value::SliceView(SliceView::from_nested(nested_views)))
            } else {
                Ok(Value::SliceView(SliceView::from_slots(slots)))
            }
        } else {
            let mut views = Vec::with_capacity(elems.len());
            for elem in elems {
                let sub = self.apply_selectors(elem, tail, root_span)?;
                match sub {
                    Value::SliceView(sv) => views.push(sv),
                    _ => {
                        return Err(self.err(
                            root_span,
                            "Expected slice view while projecting multidimensional slice",
                        ));
                    }
                }
            }
            Ok(Value::SliceView(SliceView::from_nested(views)))
        }
    }

    fn resolve_slot(
        &self,
        val: Value,
        selectors: &[EvalSelector],
        root_span: Span,
    ) -> Result<Option<(Rc<RefCell<Vec<Value>>>, usize)>, Diagnostic> {
        if selectors.is_empty() {
            return Ok(None);
        }
        let (head, tail) = (&selectors[0], &selectors[1..]);
        let EvalSelector::Index(idx, idx_span) = head else {
            return Err(self.err(
                root_span,
                "Range selector cannot resolve to a single mutable slot",
            ));
        };
        if tail.is_empty() {
            match val {
                Value::Array(rc) => {
                    let len = rc.borrow().len();
                    if *idx >= len {
                        return Err(self.err(
                            *idx_span,
                            format!("Index {} out of bounds (len {})", idx, len),
                        ));
                    }
                    Ok(Some((rc, *idx)))
                }
                Value::SliceView(sv) => {
                    let len = sv.len();
                    if *idx >= len {
                        return Err(self.err(
                            *idx_span,
                            format!("Slice index {} out of bounds (len {})", idx, len),
                        ));
                    }
                    Ok(sv.get_slot(*idx))
                }
                Value::Table(rows) => {
                    if *idx >= rows.len() {
                        return Err(self.err(
                            *idx_span,
                            format!("Row {} out of bounds (rows {})", idx, rows.len()),
                        ));
                    }
                    Ok(None)
                }
                _ => Err(self.err(*idx_span, "Cannot index non-array value")),
            }
        } else {
            let elem = match val {
                Value::Array(rc) => {
                    let b = rc.borrow();
                    b.get(*idx).cloned().ok_or_else(|| {
                        self.err(
                            *idx_span,
                            format!("Index {} out of bounds (len {})", idx, b.len()),
                        )
                    })?
                }
                Value::SliceView(sv) => {
                    let len = sv.len();
                    sv.get(*idx).ok_or_else(|| {
                        self.err(
                            *idx_span,
                            format!("Slice index {} out of bounds (len {})", idx, len),
                        )
                    })?
                }
                Value::Table(rows) => {
                    let row_rc = rows.get(*idx).cloned().ok_or_else(|| {
                        self.err(
                            *idx_span,
                            format!("Row {} out of bounds (rows {})", idx, rows.len()),
                        )
                    })?;
                    Value::Array(row_rc)
                }
                _ => return Err(self.err(*idx_span, "Cannot index non-array value")),
            };
            self.resolve_slot(elem, tail, root_span)
        }
    }

    fn write_slot(
        &mut self,
        buf_rc: &Rc<RefCell<Vec<Value>>>,
        abs_idx: usize,
        val: Value,
        span: Span,
        out: &mut dyn Write,
    ) -> Result<(), Diagnostic> {
        let coerced = {
            let b = buf_rc.borrow();
            let existing = b.get(abs_idx).ok_or_else(|| {
                self.err(
                    span,
                    format!("Index {} out of bounds (len {})", abs_idx, b.len()),
                )
            })?;
            Self::coerce_matching_existing(val, existing)
        };
        let old = {
            let mut b = buf_rc.borrow_mut();
            std::mem::replace(&mut b[abs_idx], coerced)
        };
        self.cleanup_value(old, out)
    }

    fn assign_to_view_row(
        &mut self,
        row_sv: &SliceView,
        val: Value,
        span: Span,
        out: &mut dyn Write,
    ) -> Result<(), Diagnostic> {
        let src_items = match val {
            Value::Array(rc) => rc.borrow().clone(),
            Value::SliceView(sv) => sv.to_vec(),
            _ => {
                return Err(self.err(
                    span,
                    "Expected array or slice view when assigning to nested slice view row",
                ));
            }
        };
        if src_items.len() != row_sv.len() {
            return Err(self.err(
                span,
                format!(
                    "Slice view row length mismatch: expected {} elements, got {}",
                    row_sv.len(),
                    src_items.len()
                ),
            ));
        }
        for (i, item) in src_items.into_iter().enumerate() {
            if let Some((buf, abs_idx)) = row_sv.get_slot(i) {
                self.write_slot(&buf, abs_idx, item.clone_for_value_binding(), span, out)?;
            } else if let ViewStorage::Nested(sub_views) = &row_sv.storage {
                self.assign_to_view_row(&sub_views[i], item, span, out)?;
            } else {
                return Err(self.err(span, "Nested slice view slot out of bounds"));
            }
        }
        Ok(())
    }

    fn apply_assign_target(
        &mut self,
        target: &AssignTarget,
        val: Value,
        out: &mut dyn Write,
    ) -> Result<(), Diagnostic> {
        match &target.node {
            AssignTargetKind::Ident(name) => {
                if let Some(cell) = self.lookup_var_cell(name) {
                    let ref_target_opt = match &*cell.borrow() {
                        Value::Ref(rt) => Some(rt.clone()),
                        _ => None,
                    };
                    if let Some(rt) = ref_target_opt {
                        self.write_to_ref_target(&rt, val.clone_for_value_binding(), target.span, out)?;
                    } else {
                        let bound = val.clone_for_value_binding();
                        let coerced = {
                            let existing = cell.borrow();
                            Self::coerce_matching_existing(bound, &existing)
                        };
                        let old = {
                            let mut b = cell.borrow_mut();
                            std::mem::replace(&mut *b, coerced)
                        };
                        self.cleanup_value(old, out)?;
                    }
                } else {
                    // Undeclared x = expr: keep Value::Ref if rhs was &expr, otherwise clone value
                    let stored = match val {
                        Value::Ref(_) => val,
                        other => other.clone_for_value_binding(),
                    };
                    self.insert_current_scope(name.clone(), stored);
                }
            }
            AssignTargetKind::Field { target: obj_expr, field } => {
                if let ExprKind::Ident(alias) = &obj_expr.node {
                    if let Some(dep_mod_id) = self.current_mod().imports.get(alias).cloned() {
                        if self.lookup_var_cell(alias).is_none() {
                            let cell = self
                                .mod_ref(&dep_mod_id)
                                .globals
                                .vars
                                .get(field)
                                .cloned()
                                .ok_or_else(|| {
                                    self.err(
                                        target.span,
                                        format!("Module '{}' has no variable '{}'", alias, field),
                                    )
                                })?;
                            let bound = val.clone_for_value_binding();
                            let coerced = {
                                let existing = cell.borrow();
                                Self::coerce_matching_existing(bound, &existing)
                            };
                            let old = {
                                let mut b = cell.borrow_mut();
                                std::mem::replace(&mut *b, coerced)
                            };
                            return self.cleanup_value(old, out);
                        }
                    }
                }
                let obj = self.eval_expr(obj_expr, out)?;
                let Value::Instance(rc) = obj else {
                    return Err(self.err(
                        target.span,
                        format!("Cannot assign field '{}' on non-instance", field),
                    ));
                };
                let coerced = {
                    let b = rc.borrow();
                    let existing = b.fields.get(field).ok_or_else(|| {
                        self.err(
                            target.span,
                            format!("Class '{}' has no field '{}'", b.class_name, field),
                        )
                    })?;
                    Self::coerce_matching_existing(val.clone_for_value_binding(), existing)
                };
                let old = {
                    let mut b = rc.borrow_mut();
                    b.fields.insert(field.clone(), coerced)
                };
                if let Some(old_val) = old {
                    self.cleanup_value(old_val, out)?;
                }
            }
            AssignTargetKind::Index {
                target: arr_expr,
                index,
            } => {
                let idx = self.eval_usize(index, out)?;
                let arr_val = self.eval_expr(arr_expr, out)?;
                let bound_val = val.clone_for_value_binding();
                match arr_val {
                    Value::Array(rc) => {
                        let coerced = {
                            let b = rc.borrow();
                            let existing = b.get(idx).ok_or_else(|| {
                                self.err(
                                    index.span,
                                    format!("Index {} out of bounds (len {})", idx, b.len()),
                                )
                            })?;
                            Self::coerce_matching_existing(bound_val, existing)
                        };
                        let old = {
                            let mut b = rc.borrow_mut();
                            std::mem::replace(&mut b[idx], coerced)
                        };
                        self.cleanup_value(old, out)?;
                    }
                    Value::SliceView(sv) => {
                        if idx >= sv.len() {
                            return Err(self.err(
                                index.span,
                                format!("Slice index {} out of bounds (len {})", idx, sv.len()),
                            ));
                        }
                        if let Some((buf_rc, abs_idx)) = sv.get_slot(idx) {
                            self.write_slot(&buf_rc, abs_idx, bound_val, index.span, out)?;
                        } else if let ViewStorage::Nested(views) = &sv.storage {
                            self.assign_to_view_row(&views[idx], bound_val, index.span, out)?;
                        } else {
                            return Err(self.err(index.span, "Slice underlying index out of bounds"));
                        }
                    }
                    _ => {
                        return Err(self.err(
                            target.span,
                            "Index assignment target is not an array or slice view",
                        ));
                    }
                }
            }
            AssignTargetKind::Slice {
                target: arr_expr,
                start,
                end,
            } => {
                let s = match start {
                    Some(e) => self.eval_usize(e, out)?,
                    None => 0,
                };
                let e_opt = match end {
                    Some(e) => Some(self.eval_usize(e, out)?),
                    None => None,
                };
                let arr_val = self.eval_expr(arr_expr, out)?;
                let sliced_view = match arr_val {
                    Value::Array(rc) => {
                        let len = rc.borrow().len();
                        let e_clamped = e_opt.unwrap_or(len).min(len);
                        let s_clamped = s.min(e_clamped);
                        SliceView::contiguous(rc, s_clamped, e_clamped)
                    }
                    Value::SliceView(sv) => sv.slice(s, e_opt),
                    _ => {
                        return Err(self.err(
                            target.span,
                            "Slice fill assignment target must be an array or slice view",
                        ));
                    }
                };
                match &sliced_view.storage {
                    ViewStorage::Contiguous { .. } | ViewStorage::Slots(_) => {
                        for i in 0..sliced_view.len() {
                            if let Some((buf_rc, slot_idx)) = sliced_view.get_slot(i) {
                                self.write_slot(
                                    &buf_rc,
                                    slot_idx,
                                    val.clone_for_value_binding(),
                                    target.span,
                                    out,
                                )?;
                            }
                        }
                    }
                    ViewStorage::Nested(views) => {
                        for row_sv in views {
                            self.assign_to_view_row(
                                row_sv,
                                val.clone_for_value_binding(),
                                target.span,
                                out,
                            )?;
                        }
                    }
                }
            }
            AssignTargetKind::MultiIndex {
                target: arr_expr,
                selectors,
            } => {
                let eval_sels = self.eval_selectors(selectors, out)?;
                let arr_val = self.eval_expr(arr_expr, out)?;
                let has_range = eval_sels
                    .iter()
                    .any(|s| matches!(s, EvalSelector::Range { .. }));
                if !has_range {
                    let (buf_rc, abs_idx) = self
                        .resolve_slot(arr_val, &eval_sels, target.span)?
                        .ok_or_else(|| {
                            self.err(target.span, "Multi-index assignment target is not assignable")
                        })?;
                    self.write_slot(
                        &buf_rc,
                        abs_idx,
                        val.clone_for_value_binding(),
                        target.span,
                        out,
                    )?;
                } else {
                    let view_val = self.apply_selectors(arr_val, &eval_sels, target.span)?;
                    let Value::SliceView(sv) = view_val else {
                        return Err(self.err(
                            target.span,
                            "Slice fill assignment target must evaluate to a slice view",
                        ));
                    };
                    let mut slots = Vec::new();
                    sv.collect_leaf_slots(&mut slots);
                    for (buf_rc, slot_idx) in slots {
                        self.write_slot(
                            &buf_rc,
                            slot_idx,
                            val.clone_for_value_binding(),
                            target.span,
                            out,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn exec_block(&mut self, stmts: &[Stmt], out: &mut dyn Write) -> Result<FlowCtrl, Diagnostic> {
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

    fn exec_stmt(&mut self, stmt: &Stmt, out: &mut dyn Write) -> Result<FlowCtrl, Diagnostic> {
        match &stmt.node {
            StmtKind::Import { .. } => Ok(FlowCtrl::None),
            StmtKind::FnDecl(fd) => {
                self.current_mod_mut()
                    .functions
                    .insert(fd.name.clone(), fd.clone());
                Ok(FlowCtrl::None)
            }
            StmtKind::ClassDecl(cd) => {
                self.current_mod_mut()
                    .classes
                    .insert(cd.name.clone(), cd.clone());
                Ok(FlowCtrl::None)
            }
            StmtKind::VarDecl { names, ty, init } => {
                for name in names {
                    if let Some(init_expr) = init {
                        if matches!(init_expr.node, ExprKind::InputCall) {
                            let val = self.create_value_for_type_expr(ty, true, out)?;
                            self.insert_current_scope(name.clone(), val);
                        } else if matches!(ty.node, TypeExprKind::Ref(_)) {
                            let val = self.eval_expr_raw(init_expr, out)?;
                            self.insert_current_scope(name.clone(), val);
                        } else {
                            let raw = self.eval_expr(init_expr, out)?;
                            let rt = self.resolve_runtime_type(ty, out)?;
                            let bound = raw.clone_for_value_binding();
                            let coerced = Self::coerce_for_runtime_type(bound, &rt);
                            self.insert_current_scope(name.clone(), coerced);
                        }
                    } else {
                        let val = self.create_value_for_type_expr(ty, false, out)?;
                        self.insert_current_scope(name.clone(), val);
                    }
                }
                Ok(FlowCtrl::None)
            }
            StmtKind::Assign { target, value } => {
                let val = if matches!(target.node, AssignTargetKind::Ident(_))
                    && matches!(value.node, ExprKind::Unary { op: UnaryOp::Ref, .. })
                {
                    self.eval_expr_raw(value, out)?
                } else {
                    self.eval_expr(value, out)?
                };
                self.apply_assign_target(target, val, out)?;
                Ok(FlowCtrl::None)
            }
            StmtKind::MultiAssign { targets, values } => {
                if targets.len() != values.len() {
                    return Err(self.err(
                        stmt.span,
                        format!(
                            "Multi-assignment arity mismatch: {} targets vs {} values",
                            targets.len(),
                            values.len()
                        ),
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
            StmtKind::If {
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
            StmtKind::For {
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
                        return Err(self.err(
                            range_start.span,
                            format!("For range start must be integer, got '{}'", other.display_str()),
                        ));
                    }
                };
                let end = match end_val {
                    Value::Int(i) => i,
                    Value::Uint(u) => u as i64,
                    other => {
                        return Err(self.err(
                            range_end.span,
                            format!("For range end must be integer, got '{}'", other.display_str()),
                        ));
                    }
                };
                let is_uint = if let Some(te) = var_ty {
                    self.resolve_runtime_type(te, out)? == RuntimeType::Uint
                } else {
                    false
                };
                for i in start..end {
                    let loop_val = if is_uint {
                        Value::Uint(i as u64)
                    } else {
                        Value::Int(i)
                    };
                    let mut iter_frame = ScopeFrame::new();
                    iter_frame.insert_new(var_name.clone(), loop_val);
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
            StmtKind::While { cond, body } => {
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
            StmtKind::Break => Ok(FlowCtrl::Break),
            StmtKind::Continue => Ok(FlowCtrl::Continue),
            StmtKind::Return(expr_opt) => {
                let val = match expr_opt {
                    Some(e) => self.eval_expr(e, out)?,
                    None => Value::Void,
                };
                Ok(FlowCtrl::Return(val))
            }
            StmtKind::ExprStmt(expr) => {
                let val = self.eval_expr_raw(expr, out)?;
                self.cleanup_value(val, out)?;
                Ok(FlowCtrl::None)
            }
            StmtKind::Print { args } => {
                if args.is_empty() {
                    writeln!(out).map_err(|e| self.err(stmt.span, e.to_string()))?;
                } else {
                    let mut parts = Vec::with_capacity(args.len());
                    for arg in args {
                        let val = self.eval_expr(arg, out)?;
                        parts.push(val.display_str());
                    }
                    writeln!(out, "{}", parts.join(" "))
                        .map_err(|e| self.err(stmt.span, e.to_string()))?;
                }
                Ok(FlowCtrl::None)
            }
            StmtKind::FnInput { targets } => {
                for target in targets {
                    match target {
                        InputTarget::Decl { names, ty, .. } => {
                            for name in names {
                                let val = self.create_value_for_type_expr(ty, true, out)?;
                                self.insert_current_scope(name.clone(), val);
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

    fn instantiate_class_from_call(
        &mut self,
        mod_id: &str,
        class_name: &str,
        explicit_type_args: &[TypeExpr],
        arg_vals: Vec<Value>,
        span: Span,
        out: &mut dyn Write,
    ) -> Result<Value, Diagnostic> {
        let cd = self
            .mod_ref(mod_id)
            .classes
            .get(class_name)
            .cloned()
            .ok_or_else(|| self.err(span, format!("Undefined class '{}'", class_name)))?;

        let resolved_targs = if cd.type_params.is_empty() {
            Vec::new()
        } else if !explicit_type_args.is_empty() {
            let mut res = Vec::with_capacity(explicit_type_args.len());
            for ta in explicit_type_args {
                res.push(self.resolve_runtime_type(ta, out)?);
            }
            res
        } else {
            let mut inferred = HashMap::new();
            if let Some(init_fn) = cd.methods.iter().find(|m| m.name == "init") {
                for (p, v) in init_fn.params.iter().skip(1).zip(arg_vals.iter()) {
                    if let Some(pte) = &p.ty {
                        let v_rt = runtime_type_from_value(v);
                        infer_runtime_type_bindings(pte, &v_rt, &cd.type_params, &mut inferred);
                    }
                }
            }
            cd.type_params
                .iter()
                .map(|tp| inferred.get(tp).cloned().unwrap_or(RuntimeType::Void))
                .collect()
        };

        self.instantiate_class_resolved(mod_id, class_name, resolved_targs, arg_vals, span, out)
    }

    fn instantiate_class_resolved(
        &mut self,
        mod_id: &str,
        class_name: &str,
        resolved_targs: Vec<RuntimeType>,
        arg_vals: Vec<Value>,
        span: Span,
        out: &mut dyn Write,
    ) -> Result<Value, Diagnostic> {
        let cd = self
            .mod_ref(mod_id)
            .classes
            .get(class_name)
            .cloned()
            .ok_or_else(|| self.err(span, format!("Undefined class '{}'", class_name)))?;

        let mut new_bindings = HashMap::new();
        for (tp, ta) in cd.type_params.iter().zip(resolved_targs.iter()) {
            new_bindings.insert(tp.clone(), ta.clone());
        }

        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let saved_locals = std::mem::take(&mut self.local_frames);
        let prev_bindings = std::mem::replace(&mut self.type_bindings, new_bindings);

        let mut fields = HashMap::new();
        let mut field_res = Ok(());
        for f in &cd.fields {
            let val_res = if let Some(def_expr) = &f.default_expr {
                match self.eval_expr(def_expr, out) {
                    Ok(v) => {
                        let bound = v.clone_for_value_binding();
                        if let Some(ts) = &f.ty {
                            self.resolve_runtime_type(ts, out)
                                .map(|rt| Self::coerce_for_runtime_type(bound, &rt))
                        } else {
                            Ok(bound)
                        }
                    }
                    Err(e) => Err(e),
                }
            } else if let Some(ts) = &f.ty {
                self.create_value_for_type_expr(ts, false, out)
            } else {
                Ok(Value::Void)
            };

            match val_res {
                Ok(val) => {
                    fields.insert(f.name.clone(), val);
                }
                Err(e) => {
                    field_res = Err(e);
                    break;
                }
            }
        }

        self.type_bindings = prev_bindings;
        self.local_frames = saved_locals;
        self.current_module = prev_mod;
        field_res?;

        let rc = Rc::new(RefCell::new(ClassInstance {
            module_id: mod_id.to_string(),
            class_name: cd.name.clone(),
            type_args: resolved_targs,
            fields,
            dropped: false,
        }));

        if let Some(init_fn) = cd.methods.iter().find(|m| m.name == "init").cloned() {
            self.call_method_on_rc(&rc, &init_fn, &[], arg_vals, span, out)?;
        } else if !arg_vals.is_empty() {
            return Err(self.err(
                span,
                format!(
                    "Class '{}' has no 'init' constructor taking {} argument(s)",
                    class_name,
                    arg_vals.len()
                ),
            ));
        }

        Ok(Value::Instance(rc))
    }

    fn drop_instance(
        &mut self,
        rc: Rc<RefCell<ClassInstance>>,
        out: &mut dyn Write,
    ) -> Result<(), Diagnostic> {
        if rc.borrow().dropped {
            return Ok(());
        }
        rc.borrow_mut().dropped = true;
        let (mod_id, class_name) = {
            let b = rc.borrow();
            (b.module_id.clone(), b.class_name.clone())
        };
        if let Some(drop_fn) = self
            .mod_ref(&mod_id)
            .classes
            .get(&class_name)
            .and_then(|c| c.methods.iter().find(|m| m.name == "drop"))
            .cloned()
        {
            let span = drop_fn.span;
            self.call_method_on_rc(&rc, &drop_fn, &[], Vec::new(), span, out)?;
        }
        Ok(())
    }

    fn call_method_on_rc(
        &mut self,
        rc: &Rc<RefCell<ClassInstance>>,
        method: &FnDecl,
        explicit_method_type_args: &[TypeExpr],
        arg_vals: Vec<Value>,
        span: Span,
        out: &mut dyn Write,
    ) -> Result<Value, Diagnostic> {
        let (mod_id, class_name, cls_targs) = {
            let b = rc.borrow();
            (
                b.module_id.clone(),
                b.class_name.clone(),
                b.type_args.clone(),
            )
        };
        let mut bindings = HashMap::new();
        if let Some(cd) = self.mod_ref(&mod_id).classes.get(&class_name) {
            for (tp, ta) in cd.type_params.iter().zip(cls_targs.iter()) {
                bindings.insert(tp.clone(), ta.clone());
            }
        }
        if !method.type_params.is_empty() {
            if !explicit_method_type_args.is_empty() {
                for (tp, te) in method.type_params.iter().zip(explicit_method_type_args.iter()) {
                    let rt = self.resolve_runtime_type(te, out)?;
                    bindings.insert(tp.clone(), rt);
                }
            } else {
                for (p, v) in method.params.iter().skip(1).zip(arg_vals.iter()) {
                    if let Some(pte) = &p.ty {
                        let v_rt = runtime_type_from_value(v);
                        infer_runtime_type_bindings(pte, &v_rt, &method.type_params, &mut bindings);
                    }
                }
            }
        }
        let mut all_args = Vec::with_capacity(arg_vals.len() + 1);
        all_args.push(Value::Instance(Rc::clone(rc)));
        all_args.extend(arg_vals);
        self.exec_fn_with_args(&mod_id, method, all_args, bindings, span, out)
    }

    fn exec_fn_with_args(
        &mut self,
        mod_id: &str,
        fd: &FnDecl,
        arg_vals: Vec<Value>,
        call_bindings: HashMap<String, RuntimeType>,
        span: Span,
        out: &mut dyn Write,
    ) -> Result<Value, Diagnostic> {
        if arg_vals.len() != fd.params.len() {
            return Err(self.err(
                span,
                format!(
                    "Function '{}' expects {} argument(s), got {}",
                    fd.name,
                    fd.params.len(),
                    arg_vals.len()
                ),
            ));
        }

        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let saved_locals = std::mem::take(&mut self.local_frames);
        let prev_bindings = std::mem::replace(&mut self.type_bindings, call_bindings);

        let mut fn_frame = ScopeFrame::new();
        for (param, val) in fd.params.iter().zip(arg_vals.into_iter()) {
            let is_ref_param = param
                .ty
                .as_ref()
                .is_some_and(|te| matches!(te.node, TypeExprKind::Ref(_)));
            if is_ref_param {
                fn_frame.insert_new(param.name.clone(), val);
            } else {
                let bound = val.clone_for_value_binding();
                let coerced = if let Some(te) = &param.ty {
                    if let Ok(rt) = self.resolve_runtime_type(te, out) {
                        Self::coerce_for_runtime_type(bound, &rt)
                    } else {
                        bound
                    }
                } else {
                    bound
                };
                fn_frame.insert_new(param.name.clone(), coerced);
            }
        }

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
        self.type_bindings = prev_bindings;
        self.local_frames = saved_locals;
        self.current_module = prev_mod;

        body_res?;
        cleanup_res?;

        match ctrl {
            FlowCtrl::None => Ok(Value::Void),
            FlowCtrl::Return(v) => Ok(v),
            FlowCtrl::Break | FlowCtrl::Continue => {
                Err(self.err(span, "break/continue used outside of a loop"))
            }
        }
    }

    fn eval_call_args_for_params(
        &mut self,
        params: &[Param],
        args: &[Expr],
        out: &mut dyn Write,
    ) -> Result<Vec<Value>, Diagnostic> {
        let mut vals = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let is_ref = params
                .get(i)
                .and_then(|p| p.ty.as_ref())
                .is_some_and(|te| matches!(te.node, TypeExprKind::Ref(_)));
            if is_ref {
                vals.push(self.eval_expr_raw(a, out)?);
            } else {
                vals.push(self.eval_expr(a, out)?);
            }
        }
        Ok(vals)
    }

    fn eval_call_in_module(
        &mut self,
        target_mod_id: &str,
        callee: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
        span: Span,
        out: &mut dyn Write,
    ) -> Result<Value, Diagnostic> {
        if self.mod_ref(target_mod_id).classes.contains_key(callee) {
            let cd = self.mod_ref(target_mod_id).classes.get(callee).cloned().unwrap();
            let arg_vals = if let Some(init_fn) = cd.methods.iter().find(|m| m.name == "init") {
                self.eval_call_args_for_params(&init_fn.params[1..], args, out)?
            } else {
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.eval_expr(a, out)?);
                }
                vals
            };
            return self.instantiate_class_from_call(
                target_mod_id,
                callee,
                type_args,
                arg_vals,
                span,
                out,
            );
        }

        let fd = self
            .mod_ref(target_mod_id)
            .functions
            .get(callee)
            .cloned()
            .ok_or_else(|| self.err(span, format!("Undefined function '{}'", callee)))?;

        let arg_vals = self.eval_call_args_for_params(&fd.params, args, out)?;
        let mut fn_bindings = HashMap::new();
        if !fd.type_params.is_empty() {
            if !type_args.is_empty() {
                for (tp, te) in fd.type_params.iter().zip(type_args.iter()) {
                    let rt = self.resolve_runtime_type(te, out)?;
                    fn_bindings.insert(tp.clone(), rt);
                }
            } else {
                for (p, v) in fd.params.iter().zip(arg_vals.iter()) {
                    if let Some(pte) = &p.ty {
                        let v_rt = runtime_type_from_value(v);
                        infer_runtime_type_bindings(pte, &v_rt, &fd.type_params, &mut fn_bindings);
                    }
                }
            }
        }
        self.exec_fn_with_args(target_mod_id, &fd, arg_vals, fn_bindings, span, out)
    }

    fn eval_expr(&mut self, expr: &Expr, out: &mut dyn Write) -> Result<Value, Diagnostic> {
        let raw = self.eval_expr_raw(expr, out)?;
        raw.deref_read().map_err(|m| self.err(expr.span, m))
    }

    fn eval_expr_raw(&mut self, expr: &Expr, out: &mut dyn Write) -> Result<Value, Diagnostic> {
        match &expr.node {
            ExprKind::Int(v) => Ok(Value::Int(*v)),
            ExprKind::Float(v) => Ok(Value::Float(*v)),
            ExprKind::Char(v) => Ok(Value::Char(*v)),
            ExprKind::Str(v) => Ok(Value::String(v.clone())),
            ExprKind::Bool(v) => Ok(Value::Bool(*v)),
            ExprKind::ArrayLiteral(elems) => {
                let mut items = Vec::with_capacity(elems.len());
                for e in elems {
                    let v = self.eval_expr(e, out)?.clone_for_value_binding();
                    items.push(v);
                }
                Ok(Value::Array(Rc::new(RefCell::new(items))))
            }
            ExprKind::SelfRef => {
                let cell = self
                    .lookup_var_cell("self")
                    .ok_or_else(|| self.err(expr.span, "'self' used outside of a class method"))?;
                Ok(cell.borrow().clone())
            }
            ExprKind::Ident(name) => {
                let cell = self.lookup_var_cell(name).ok_or_else(|| {
                    self.err(expr.span, format!("Undefined variable '{}'", name))
                })?;
                Ok(cell.borrow().clone())
            }
            ExprKind::Unary { op, expr: inner } => match op {
                UnaryOp::Neg => {
                    let v = self.eval_expr(inner, out)?;
                    match v {
                        Value::Int(x) => Ok(Value::Int(-x)),
                        Value::Float(x) => Ok(Value::Float(-x)),
                        _ => Err(self.err(expr.span, "Invalid unary '-' operation")),
                    }
                }
                UnaryOp::Not => {
                    let v = self.eval_expr(inner, out)?;
                    match v {
                        Value::Bool(b) => Ok(Value::Bool(!b)),
                        _ => Err(self.err(expr.span, "Invalid unary '!' operation")),
                    }
                }
                UnaryOp::Ref => self.eval_ref_expr(inner, expr.span, out),
            },
            ExprKind::Binary { op, left, right } => {
                let l = self.eval_expr(left, out)?;
                let r = self.eval_expr(right, out)?;
                eval_binary(*op, l, r).map_err(|m| self.err(expr.span, m))
            }
            ExprKind::Call {
                callee,
                type_args,
                args,
            } => {
                if callee == "len" {
                    if args.len() != 1 {
                        return Err(self.err(
                            expr.span,
                            format!("len() expects 1 argument, got {}", args.len()),
                        ));
                    }
                    let v = self.eval_expr(&args[0], out)?;
                    return match v {
                        Value::Array(rc) => Ok(Value::Int(rc.borrow().len() as i64)),
                        Value::SliceView(sv) => Ok(Value::Int(sv.len() as i64)),
                        Value::Table(rows) => Ok(Value::Int(rows.len() as i64)),
                        Value::String(s) => Ok(Value::Int(s.chars().count() as i64)),
                        _ => Err(self.err(expr.span, "len() expects an array or string")),
                    };
                }
                let mid = self.current_module.clone();
                self.eval_call_in_module(&mid, callee, type_args, args, expr.span, out)
            }
            ExprKind::FieldAccess { target, field } => {
                if let ExprKind::Ident(alias) = &target.node {
                    if let Some(dep_mod_id) = self.current_mod().imports.get(alias).cloned() {
                        if self.lookup_var_cell(alias).is_none() {
                            let cell = self
                                .mod_ref(&dep_mod_id)
                                .globals
                                .vars
                                .get(field)
                                .cloned()
                                .ok_or_else(|| {
                                    self.err(
                                        expr.span,
                                        format!("Module '{}' has no variable '{}'", alias, field),
                                    )
                                })?;
                            return Ok(cell.borrow().clone());
                        }
                    }
                }
                let obj = self.eval_expr(target, out)?;
                let Value::Instance(rc) = obj else {
                    return Err(self.err(
                        expr.span,
                        format!("Cannot access field '{}' on non-instance", field),
                    ));
                };
                let borrowed = rc.borrow();
                borrowed.fields.get(field).cloned().ok_or_else(|| {
                    self.err(
                        expr.span,
                        format!("Class '{}' has no field '{}'", borrowed.class_name, field),
                    )
                })
            }
            ExprKind::MethodCall {
                target,
                method,
                type_args,
                args,
            } => {
                if let ExprKind::Ident(alias) = &target.node {
                    if let Some(dep_mod_id) = self.current_mod().imports.get(alias).cloned() {
                        if self.lookup_var_cell(alias).is_none() {
                            return self.eval_call_in_module(
                                &dep_mod_id,
                                method,
                                type_args,
                                args,
                                expr.span,
                                out,
                            );
                        }
                    }
                }

                let obj = self.eval_expr(target, out)?;
                match obj {
                    Value::Array(rc) => match method.as_str() {
                        "len" => Ok(Value::Int(rc.borrow().len() as i64)),
                        "push" => {
                            let v = self.eval_expr(&args[0], out)?.clone_for_value_binding();
                            rc.borrow_mut().push(v);
                            Ok(Value::Void)
                        }
                        "pop" => {
                            let popped = rc.borrow_mut().pop().ok_or_else(|| {
                                self.err(expr.span, "Cannot call pop() on an empty array")
                            })?;
                            Ok(popped)
                        }
                        "clear" => {
                            let old_items: Vec<Value> = {
                                let mut b = rc.borrow_mut();
                                std::mem::take(&mut *b)
                            };
                            for item in old_items.into_iter().rev() {
                                self.cleanup_value(item, out)?;
                            }
                            Ok(Value::Void)
                        }
                        other => Err(self.err(
                            expr.span,
                            format!("Array has no method '{}'", other),
                        )),
                    },
                    Value::SliceView(sv) => match method.as_str() {
                        "len" => Ok(Value::Int(sv.len() as i64)),
                        "push" | "pop" | "clear" => Err(self
                            .err(
                                expr.span,
                                format!("Cannot call '{}()' on a slice view", method),
                            )
                            .with_hint("Call this method on the array variable itself, not a slice")),
                        other => Err(self.err(
                            expr.span,
                            format!("Slice view has no method '{}'", other),
                        )),
                    },
                    Value::String(s) => match method.as_str() {
                        "len" => Ok(Value::Int(s.chars().count() as i64)),
                        other => Err(self.err(
                            expr.span,
                            format!("string has no method '{}'", other),
                        )),
                    },
                    Value::Table(rows) => match method.as_str() {
                        "len" => Ok(Value::Int(rows.len() as i64)),
                        other => Err(self.err(
                            expr.span,
                            format!("Table has no method '{}'", other),
                        )),
                    },
                    Value::Instance(rc) => {
                        let (mod_id, cname) = {
                            let b = rc.borrow();
                            (b.module_id.clone(), b.class_name.clone())
                        };
                        let method_fn = self
                            .mod_ref(&mod_id)
                            .classes
                            .get(&cname)
                            .and_then(|c| c.methods.iter().find(|m| m.name == *method))
                            .cloned()
                            .ok_or_else(|| {
                                self.err(
                                    expr.span,
                                    format!("Class '{}' has no method '{}'", cname, method),
                                )
                            })?;
                        let arg_vals =
                            self.eval_call_args_for_params(&method_fn.params[1..], args, out)?;
                        self.call_method_on_rc(&rc, &method_fn, type_args, arg_vals, expr.span, out)
                    }
                    _ => Err(self.err(
                        expr.span,
                        format!("Cannot call method '{}' on this value", method),
                    )),
                }
            }
            ExprKind::Index { target, index } => {
                let idx = self.eval_usize(index, out)?;
                let tval = self.eval_expr(target, out)?;
                match tval {
                    Value::Table(rows) => {
                        let row_rc = rows.get(idx).cloned().ok_or_else(|| {
                            self.err(
                                index.span,
                                format!("Row {} out of bounds (rows {})", idx, rows.len()),
                            )
                        })?;
                        Ok(Value::Array(row_rc))
                    }
                    Value::Array(rc) => {
                        let b = rc.borrow();
                        b.get(idx).cloned().ok_or_else(|| {
                            self.err(
                                index.span,
                                format!("Index {} out of bounds (len {})", idx, b.len()),
                            )
                        })
                    }
                    Value::SliceView(sv) => sv.get(idx).ok_or_else(|| {
                        self.err(
                            index.span,
                            format!("Slice index {} out of bounds (len {})", idx, sv.len()),
                        )
                    }),
                    Value::String(s) => s
                        .chars()
                        .nth(idx)
                        .map(Value::Char)
                        .ok_or_else(|| self.err(index.span, format!("String index {} out of bounds", idx))),
                    _ => Err(self.err(expr.span, "Cannot index non-array/non-string value")),
                }
            }
            ExprKind::SliceView { target, start, end } => {
                let s = match start {
                    Some(e) => self.eval_usize(e, out)?,
                    None => 0,
                };
                let e_opt = match end {
                    Some(e) => Some(self.eval_usize(e, out)?),
                    None => None,
                };
                let tval = self.eval_expr(target, out)?;
                match tval {
                    Value::Array(rc) => {
                        let len = rc.borrow().len();
                        let e_clamped = e_opt.unwrap_or(len).min(len);
                        let s_clamped = s.min(e_clamped);
                        Ok(Value::SliceView(SliceView::contiguous(rc, s_clamped, e_clamped)))
                    }
                    Value::SliceView(sv) => Ok(Value::SliceView(sv.slice(s, e_opt))),
                    Value::String(str_val) => {
                        let chars: Vec<char> = str_val.chars().collect();
                        let e_clamped = e_opt.unwrap_or(chars.len()).min(chars.len());
                        let s_clamped = s.min(e_clamped);
                        Ok(Value::String(chars[s_clamped..e_clamped].iter().collect()))
                    }
                    _ => Err(self.err(expr.span, "SliceView target must be an array or string")),
                }
            }
            ExprKind::MultiIndex { target, selectors } => {
                let eval_sels = self.eval_selectors(selectors, out)?;
                let tval = self.eval_expr(target, out)?;
                self.apply_selectors(tval, &eval_sels, expr.span)
            }
            ExprKind::InputCall => Err(self.err(
                expr.span,
                "input() cannot be used in this expression context; use 'N: int = input()'",
            )),
        }
    }

    fn eval_ref_expr(
        &mut self,
        inner: &Expr,
        span: Span,
        out: &mut dyn Write,
    ) -> Result<Value, Diagnostic> {
        match &inner.node {
            ExprKind::Ident(name) => {
                let cell = self.lookup_var_cell(name).ok_or_else(|| {
                    self.err(inner.span, format!("Undefined variable '{}'", name))
                })?;
                if let Value::Ref(rt) = &*cell.borrow() {
                    return Ok(Value::Ref(rt.clone()));
                }
                Ok(Value::Ref(RefTarget::Var(cell)))
            }
            ExprKind::FieldAccess { target, field } => {
                let obj = self.eval_expr(target, out)?;
                let Value::Instance(rc) = obj else {
                    return Err(self.err(
                        span,
                        format!("Cannot take reference to field '{}' on non-instance", field),
                    ));
                };
                if !rc.borrow().fields.contains_key(field) {
                    return Err(self.err(span, format!("Field '{}' not found", field)));
                }
                Ok(Value::Ref(RefTarget::Field {
                    instance: rc,
                    field: field.clone(),
                }))
            }
            ExprKind::Index { target, index } => {
                let idx = self.eval_usize(index, out)?;
                let tval = self.eval_expr(target, out)?;
                match tval {
                    Value::Array(rc) => {
                        let len = rc.borrow().len();
                        if idx >= len {
                            return Err(self.err(
                                index.span,
                                format!("Index {} out of bounds (len {})", idx, len),
                            ));
                        }
                        Ok(Value::Ref(RefTarget::Index {
                            buffer: rc,
                            index: idx,
                        }))
                    }
                    Value::SliceView(sv) => {
                        if idx >= sv.len() {
                            return Err(self.err(
                                index.span,
                                format!("Slice index {} out of bounds (len {})", idx, sv.len()),
                            ));
                        }
                        let (buffer, index) = sv.get_slot(idx).ok_or_else(|| {
                            self.err(span, "Cannot take reference '&' to a nested view row")
                        })?;
                        Ok(Value::Ref(RefTarget::Index { buffer, index }))
                    }
                    _ => Err(self.err(span, "Cannot take reference to this indexed target")),
                }
            }
            ExprKind::MultiIndex { target, selectors } => {
                if selectors
                    .iter()
                    .any(|s| matches!(s, SliceSelector::Range { .. }))
                {
                    return Err(self.err(span, "Cannot take reference '&' to a slice range"));
                }
                let eval_sels = self.eval_selectors(selectors, out)?;
                let tval = self.eval_expr(target, out)?;
                let (buffer, index) = self
                    .resolve_slot(tval, &eval_sels, span)?
                    .ok_or_else(|| self.err(span, "Cannot take reference to this indexed target"))?;
                Ok(Value::Ref(RefTarget::Index { buffer, index }))
            }
            _ => Err(self.err(
                span,
                "Cannot take reference '&' of a temporary expression",
            )),
        }
    }
}

fn infer_runtime_type_bindings(
    pattern: &TypeExpr,
    actual: &RuntimeType,
    type_params: &[String],
    out_map: &mut HashMap<String, RuntimeType>,
) {
    match &pattern.node {
        TypeExprKind::Named {
            module: None,
            name,
            type_args,
        } if type_args.is_empty() && type_params.contains(name) => {
            out_map.entry(name.clone()).or_insert_with(|| actual.clone());
        }
        TypeExprKind::Ref(inner) => {
            infer_runtime_type_bindings(inner, actual, type_params, out_map);
        }
        TypeExprKind::Array { elem, .. } => {
            if let RuntimeType::Array(act_elem) = actual {
                infer_runtime_type_bindings(elem, act_elem, type_params, out_map);
            }
        }
        _ => {}
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
