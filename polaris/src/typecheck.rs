use std::collections::HashMap;

use crate::ast::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    Uint,
    Float,
    Char,
    String,
    Bool,
    Void,
    Array1D(Box<Type>),
    Table(Vec<Type>),
    Class(String),
}

impl Type {
    pub fn display_name(&self) -> String {
        match self {
            Type::Int => "int".to_string(),
            Type::Uint => "uint".to_string(),
            Type::Float => "float".to_string(),
            Type::Char => "char".to_string(),
            Type::String => "string".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Void => "void".to_string(),
            Type::Array1D(elem) => format!("{}[]", elem.display_name()),
            Type::Table(rows) => {
                let inner: Vec<String> = rows.iter().map(|r| r.display_name()).collect();
                format!("[{}][]", inner.join(", "))
            }
            Type::Class(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct FnSig {
    params: Vec<Type>,
    ret: Option<Type>,
}

#[derive(Debug, Clone)]
struct ClassInfo {
    fields: HashMap<String, Type>,
    methods: HashMap<String, FnSig>,
}

pub struct TypeChecker {
    globals: HashMap<String, Type>,
    scopes: Vec<HashMap<String, Type>>,
    functions: HashMap<String, FnSig>,
    fn_decls: HashMap<String, FnDecl>,
    checked_fns: std::collections::HashSet<String>,
    checking_fns: std::collections::HashSet<String>,
    classes: HashMap<String, ClassInfo>,
    loop_depth: usize,
    return_stack: Vec<(Option<Type>, Vec<Type>)>,
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            globals: HashMap::new(),
            scopes: Vec::new(),
            functions: HashMap::new(),
            fn_decls: HashMap::new(),
            checked_fns: std::collections::HashSet::new(),
            checking_fns: std::collections::HashSet::new(),
            classes: HashMap::new(),
            loop_depth: 0,
            return_stack: Vec::new(),
        }
    }

    pub fn check_program(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        // Pass 1: Pre-register classes and top-level function headers
        for stmt in stmts {
            if let Stmt::ClassDecl(cd) = stmt {
                self.register_class_header(cd)?;
            }
        }
        for stmt in stmts {
            if let Stmt::ClassDecl(cd) = stmt {
                self.register_class_members(cd)?;
            } else if let Stmt::FnDecl(fd) = stmt {
                self.register_fn_header(fd)?;
                self.fn_decls.insert(fd.name.clone(), fd.clone());
            }
        }

        // Pass 2: Check all statements in source order (globals declared above a fn are visible)
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    fn resolve_type_ref(&self, r: &TypeRef) -> Result<Type, String> {
        match r {
            TypeRef::Int => Ok(Type::Int),
            TypeRef::Uint => Ok(Type::Uint),
            TypeRef::Float => Ok(Type::Float),
            TypeRef::Char => Ok(Type::Char),
            TypeRef::String => Ok(Type::String),
            TypeRef::Bool => Ok(Type::Bool),
            TypeRef::Void => Ok(Type::Void),
            TypeRef::Custom(name) => {
                if self.classes.contains_key(name) {
                    Ok(Type::Class(name.clone()))
                } else {
                    Err(format!("Unknown type '{}'", name))
                }
            }
        }
    }

    fn resolve_type_spec(&mut self, spec: &TypeSpec) -> Result<Type, String> {
        match spec {
            TypeSpec::Scalar(r) => self.resolve_type_ref(r),
            TypeSpec::Array1D { elem, len } => {
                if let Some(len_expr) = len {
                    let lt = self.check_expr(len_expr)?;
                    if !matches!(lt, Type::Int | Type::Uint) {
                        return Err(format!(
                            "Array length must be integer, got {}",
                            lt.display_name()
                        ));
                    }
                }
                let et = self.resolve_type_ref(elem)?;
                if et == Type::Void {
                    return Err("Array element type cannot be void".to_string());
                }
                Ok(Type::Array1D(Box::new(et)))
            }
            TypeSpec::Table { row_types, len } => {
                if let Some(len_expr) = len {
                    let lt = self.check_expr(len_expr)?;
                    if !matches!(lt, Type::Int | Type::Uint) {
                        return Err(format!(
                            "Table length must be integer, got {}",
                            lt.display_name()
                        ));
                    }
                }
                let mut rows = Vec::with_capacity(row_types.len());
                for rt in row_types {
                    let t = self.resolve_type_ref(rt)?;
                    if t == Type::Void {
                        return Err("Table element type cannot be void".to_string());
                    }
                    rows.push(t);
                }
                Ok(Type::Table(rows))
            }
        }
    }

    fn is_compatible(&self, expected: &Type, actual: &Type, expr: Option<&Expr>) -> bool {
        if expected == actual {
            return true;
        }
        if *expected == Type::Uint && *actual == Type::Int {
            if let Some(Expr::Int(n)) = expr {
                return *n >= 0;
            }
        }
        false
    }

    fn lookup_var(&self, name: &str) -> Option<Type> {
        for frame in self.scopes.iter().rev() {
            if let Some(t) = frame.get(name) {
                return Some(t.clone());
            }
        }
        self.globals.get(name).cloned()
    }

    fn insert_var_in_current_scope(&mut self, name: String, ty: Type) {
        if let Some(frame) = self.scopes.last_mut() {
            frame.insert(name, ty);
        } else {
            self.globals.insert(name, ty);
        }
    }

    fn register_class_header(&mut self, cd: &ClassDecl) -> Result<(), String> {
        if self.classes.contains_key(&cd.name) || self.functions.contains_key(&cd.name) {
            return Err(format!("Duplicate class or function name '{}'", cd.name));
        }
        self.classes.insert(
            cd.name.clone(),
            ClassInfo {
                fields: HashMap::new(),
                methods: HashMap::new(),
            },
        );
        Ok(())
    }

    fn register_class_members(&mut self, cd: &ClassDecl) -> Result<(), String> {
        let mut fields = HashMap::new();
        for f in &cd.fields {
            if fields.contains_key(&f.name) {
                return Err(format!(
                    "Duplicate field '{}' in class '{}'",
                    f.name, cd.name
                ));
            }
            let ftype = match (&f.ty, &f.default_expr) {
                (Some(ts), Some(def)) => {
                    let expected = self.resolve_type_spec(ts)?;
                    let actual = self.check_expr(def)?;
                    if !self.is_compatible(&expected, &actual, Some(def)) {
                        return Err(format!(
                            "Field '{}' in class '{}' has type {}, cannot initialize with {}",
                            f.name,
                            cd.name,
                            expected.display_name(),
                            actual.display_name()
                        ));
                    }
                    expected
                }
                (Some(ts), None) => self.resolve_type_spec(ts)?,
                (None, Some(def)) => {
                    let inferred = self.check_expr(def)?;
                    if inferred == Type::Void {
                        return Err(format!(
                            "Field '{}' in class '{}' cannot have type void",
                            f.name, cd.name
                        ));
                    }
                    inferred
                }
                (None, None) => {
                    return Err(format!(
                        "Field '{}' in class '{}' needs a type or initializer",
                        f.name, cd.name
                    ));
                }
            };
            fields.insert(f.name.clone(), ftype);
        }

        let mut methods = HashMap::new();
        for m in &cd.methods {
            if methods.contains_key(&m.name) {
                return Err(format!(
                    "Duplicate method '{}' in class '{}'",
                    m.name, cd.name
                ));
            }
            if m.params.is_empty() || m.params[0].name != "self" {
                return Err(format!(
                    "Method '{}' in class '{}' must take 'self' as its first parameter",
                    m.name, cd.name
                ));
            }
            let mut param_types = Vec::new();
            for (idx, p) in m.params.iter().enumerate() {
                if idx == 0 {
                    if let Some(ts) = &p.ty {
                        let t = self.resolve_type_spec(ts)?;
                        if t != Type::Class(cd.name.clone()) {
                            return Err(format!(
                                "'self' parameter in class '{}' must have type {}",
                                cd.name, cd.name
                            ));
                        }
                    }
                    param_types.push(Type::Class(cd.name.clone()));
                } else {
                    let ts = p
                        .ty
                        .as_ref()
                        .ok_or_else(|| format!("Parameter '{}' requires a type", p.name))?;
                    param_types.push(self.resolve_type_spec(ts)?);
                }
            }
            let ret = match &m.return_type {
                Some(ts) => Some(self.resolve_type_spec(ts)?),
                None => None,
            };
            if m.name == "init" {
                if let Some(rt) = &ret {
                    if *rt != Type::Void {
                        return Err("Constructor 'init' cannot return a non-void value".to_string());
                    }
                }
            }
            if m.name == "drop" {
                if m.params.len() != 1 {
                    return Err("Destructor 'drop' must only take 'self'".to_string());
                }
                if let Some(rt) = &ret {
                    if *rt != Type::Void {
                        return Err("Destructor 'drop' cannot return a non-void value".to_string());
                    }
                }
            }
            methods.insert(
                m.name.clone(),
                FnSig {
                    params: param_types,
                    ret,
                },
            );
        }

        if let Some(info) = self.classes.get_mut(&cd.name) {
            info.fields = fields;
            info.methods = methods;
        }
        Ok(())
    }

    fn register_fn_header(&mut self, fd: &FnDecl) -> Result<(), String> {
        if self.functions.contains_key(&fd.name) || self.classes.contains_key(&fd.name) {
            return Err(format!("Duplicate function or class name '{}'", fd.name));
        }
        if fd.name == "print" || fd.name == "input" || fd.name == "len" {
            return Err(format!("Cannot redefine built-in function '{}'", fd.name));
        }
        let mut params = Vec::with_capacity(fd.params.len());
        for p in &fd.params {
            let ts = p
                .ty
                .as_ref()
                .ok_or_else(|| format!("Parameter '{}' in fn '{}' requires a type", p.name, fd.name))?;
            params.push(self.resolve_type_spec(ts)?);
        }
        let ret = match &fd.return_type {
            Some(ts) => Some(self.resolve_type_spec(ts)?),
            None => None,
        };
        self.functions.insert(fd.name.clone(), FnSig { params, ret });
        Ok(())
    }

    fn ensure_fn_checked(&mut self, name: &str) -> Result<(), String> {
        if self.checked_fns.contains(name) {
            return Ok(());
        }
        let fd = self
            .fn_decls
            .get(name)
            .cloned()
            .ok_or_else(|| format!("Undefined function '{}'", name))?;
        if !self.functions.contains_key(name) {
            self.register_fn_header(&fd)?;
        }
        let sig = self.functions.get(name).cloned().unwrap();
        let params: Vec<(String, Type)> = fd
            .params
            .iter()
            .map(|p| p.name.clone())
            .zip(sig.params.clone())
            .collect();

        self.checking_fns.insert(name.to_string());
        let res = self.check_fn_body(name, &params, sig.ret, &fd.body);
        self.checking_fns.remove(name);
        let inferred_ret = res?;

        if let Some(entry) = self.functions.get_mut(name) {
            entry.ret = Some(inferred_ret);
        }
        self.checked_fns.insert(name.to_string());
        Ok(())
    }

    fn check_fn_body(
        &mut self,
        fn_name: &str,
        params: &[(String, Type)],
        explicit_ret: Option<Type>,
        body: &[Stmt],
    ) -> Result<Type, String> {
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_loop_depth = self.loop_depth;
        self.loop_depth = 0;

        let mut fn_scope = HashMap::new();
        for (pname, pty) in params {
            fn_scope.insert(pname.clone(), pty.clone());
        }
        self.scopes.push(fn_scope);
        self.return_stack.push((explicit_ret.clone(), Vec::new()));

        let mut body_res = Ok(());
        for stmt in body {
            if let Err(e) = self.check_stmt(stmt) {
                body_res = Err(e);
                break;
            }
        }

        let (_, observed_returns) = self.return_stack.pop().unwrap();
        self.scopes = saved_scopes;
        self.loop_depth = saved_loop_depth;
        body_res?;

        let final_ret = match explicit_ret {
            Some(rt) => {
                if rt != Type::Void && !definitely_returns(body) {
                    return Err(format!(
                        "Function '{}' declares return type {} but may not return a value on all paths",
                        fn_name,
                        rt.display_name()
                    ));
                }
                rt
            }
            None => {
                if observed_returns.is_empty() {
                    Type::Void
                } else {
                    let first = observed_returns[0].clone();
                    for t in &observed_returns[1..] {
                        if *t != first {
                            return Err(format!(
                                "Conflicting inferred return types in '{}': {} vs {}",
                                fn_name,
                                first.display_name(),
                                t.display_name()
                            ));
                        }
                    }
                    first
                }
            }
        };

        Ok(final_ret)
    }

    fn check_block(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        self.scopes.push(HashMap::new());
        let mut res = Ok(());
        for stmt in stmts {
            if let Err(e) = self.check_stmt(stmt) {
                res = Err(e);
                break;
            }
        }
        self.scopes.pop();
        res
    }

    fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::FnDecl(fd) => {
                self.fn_decls.insert(fd.name.clone(), fd.clone());
                self.ensure_fn_checked(&fd.name)
            }
            Stmt::ClassDecl(cd) => {
                if !self.classes.contains_key(&cd.name) {
                    self.register_class_header(cd)?;
                    self.register_class_members(cd)?;
                }
                let class_info = self.classes.get(&cd.name).cloned().unwrap();
                for m in &cd.methods {
                    let sig = class_info.methods.get(&m.name).cloned().unwrap();
                    let params: Vec<(String, Type)> = m
                        .params
                        .iter()
                        .map(|p| p.name.clone())
                        .zip(sig.params.clone())
                        .collect();
                    let inferred_ret =
                        self.check_fn_body(&format!("{}.{}", cd.name, m.name), &params, sig.ret, &m.body)?;
                    if let Some(cinfo) = self.classes.get_mut(&cd.name) {
                        if let Some(msig) = cinfo.methods.get_mut(&m.name) {
                            msig.ret = Some(inferred_ret);
                        }
                    }
                }
                Ok(())
            }
            Stmt::VarDecl { names, ty, init } => {
                let resolved_ty = self.resolve_type_spec(ty)?;
                if resolved_ty == Type::Void {
                    return Err("Cannot declare variable of type void".to_string());
                }
                if let Some(init_expr) = init {
                    if *init_expr == Expr::InputCall {
                        if matches!(resolved_ty, Type::Table(_) | Type::Class(_)) {
                            return Err(format!(
                                "Direct input() initialization is not supported for {}",
                                resolved_ty.display_name()
                            ));
                        }
                    } else {
                        let actual = self.check_expr(init_expr)?;
                        if !self.is_compatible(&resolved_ty, &actual, Some(init_expr)) {
                            return Err(format!(
                                "Type mismatch in declaration of '{}': expected {}, got {}",
                                names.join(", "),
                                resolved_ty.display_name(),
                                actual.display_name()
                            ));
                        }
                    }
                } else {
                    let check_zero_arg_init = |cname: &str| -> Result<(), String> {
                        if let Some(cinfo) = self.classes.get(cname) {
                            if let Some(init_sig) = cinfo.methods.get("init") {
                                if init_sig.params.len() > 1 {
                                    return Err(format!(
                                        "Class '{}' requires {} constructor arguments; initialize with '{}(...)'",
                                        cname,
                                        init_sig.params.len() - 1,
                                        cname
                                    ));
                                }
                            }
                        }
                        Ok(())
                    };
                    match &resolved_ty {
                        Type::Class(cname) => check_zero_arg_init(cname)?,
                        Type::Array1D(elem) => {
                            if let Type::Class(cname) = elem.as_ref() {
                                check_zero_arg_init(cname)?;
                            }
                        }
                        _ => {}
                    }
                }
                for name in names {
                    self.insert_var_in_current_scope(name.clone(), resolved_ty.clone());
                }
                Ok(())
            }
            Stmt::Assign { target, value } => match target {
                AssignTarget::Ident(name) => {
                    if *value == Expr::InputCall {
                        let existing = self.lookup_var(name).ok_or_else(|| {
                            format!(
                                "Cannot infer type for '{}' from input(); use '{}: <type> = input()'",
                                name, name
                            )
                        })?;
                        if matches!(existing, Type::Table(_) | Type::Class(_)) {
                            return Err(format!(
                                "Direct input() is not supported for {}",
                                existing.display_name()
                            ));
                        }
                        return Ok(());
                    }
                    let val_ty = self.check_expr(value)?;
                    if val_ty == Type::Void {
                        return Err(format!("Cannot assign void value to '{}'", name));
                    }
                    if let Some(existing_ty) = self.lookup_var(name) {
                        if !self.is_compatible(&existing_ty, &val_ty, Some(value)) {
                            return Err(format!(
                                "Type mismatch assigning to '{}': expected {}, got {}",
                                name,
                                existing_ty.display_name(),
                                val_ty.display_name()
                            ));
                        }
                    } else {
                        // Undeclared x = expr automatically declares x with inferred type
                        self.insert_var_in_current_scope(name.clone(), val_ty);
                    }
                    Ok(())
                }
                AssignTarget::Field { target, field } => {
                    let obj_ty = self.check_expr(target)?;
                    let field_ty = self.lookup_class_field(&obj_ty, field)?;
                    let val_ty = self.check_expr(value)?;
                    if !self.is_compatible(&field_ty, &val_ty, Some(value)) {
                        return Err(format!(
                            "Type mismatch assigning to field '{}': expected {}, got {}",
                            field,
                            field_ty.display_name(),
                            val_ty.display_name()
                        ));
                    }
                    Ok(())
                }
                AssignTarget::Index { target, index } => {
                    let elem_ty = self.check_index_target(target, index)?;
                    let val_ty = self.check_expr(value)?;
                    if !self.is_compatible(&elem_ty, &val_ty, Some(value)) {
                        return Err(format!(
                            "Type mismatch in indexed assignment: expected {}, got {}",
                            elem_ty.display_name(),
                            val_ty.display_name()
                        ));
                    }
                    Ok(())
                }
                AssignTarget::Slice { target, start, end } => {
                    let elem_ty = self.check_slice_assign_target(target, start.as_ref(), end.as_ref())?;
                    let val_ty = self.check_expr(value)?;
                    if !self.is_compatible(&elem_ty, &val_ty, Some(value)) {
                        return Err(format!(
                            "Type mismatch in slice fill assignment: expected {}, got {}",
                            elem_ty.display_name(),
                            val_ty.display_name()
                        ));
                    }
                    Ok(())
                }
            },
            Stmt::MultiAssign { targets, values } => {
                if targets.len() != values.len() {
                    return Err(format!(
                        "Multi-assignment arity mismatch: {} targets vs {} values",
                        targets.len(),
                        values.len()
                    ));
                }
                let mut val_types = Vec::with_capacity(values.len());
                for v in values {
                    let vt = self.check_expr(v)?;
                    if vt == Type::Void {
                        return Err("Cannot assign void in multi-assignment".to_string());
                    }
                    val_types.push(vt);
                }
                for ((t, vt), vexpr) in targets.iter().zip(val_types.into_iter()).zip(values.iter()) {
                    match t {
                        AssignTarget::Ident(name) => {
                            if let Some(existing) = self.lookup_var(name) {
                                if !self.is_compatible(&existing, &vt, Some(vexpr)) {
                                    return Err(format!(
                                        "Type mismatch assigning to '{}': expected {}, got {}",
                                        name,
                                        existing.display_name(),
                                        vt.display_name()
                                    ));
                                }
                            } else {
                                self.insert_var_in_current_scope(name.clone(), vt);
                            }
                        }
                        AssignTarget::Field { target, field } => {
                            let obj_ty = self.check_expr(target)?;
                            let fty = self.lookup_class_field(&obj_ty, field)?;
                            if !self.is_compatible(&fty, &vt, Some(vexpr)) {
                                return Err(format!(
                                    "Type mismatch assigning to field '{}': expected {}, got {}",
                                    field,
                                    fty.display_name(),
                                    vt.display_name()
                                ));
                            }
                        }
                        AssignTarget::Index { target, index } => {
                            let ety = self.check_index_target(target, index)?;
                            if !self.is_compatible(&ety, &vt, Some(vexpr)) {
                                return Err(format!(
                                    "Type mismatch in multi-assignment index: expected {}, got {}",
                                    ety.display_name(),
                                    vt.display_name()
                                ));
                            }
                        }
                        AssignTarget::Slice { target, start, end } => {
                            let ety =
                                self.check_slice_assign_target(target, start.as_ref(), end.as_ref())?;
                            if !self.is_compatible(&ety, &vt, Some(vexpr)) {
                                return Err(format!(
                                    "Type mismatch in multi-assignment slice: expected {}, got {}",
                                    ety.display_name(),
                                    vt.display_name()
                                ));
                            }
                        }
                    }
                }
                Ok(())
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let ct = self.check_expr(cond)?;
                if ct != Type::Bool {
                    return Err(format!(
                        "If condition must be bool, got {}",
                        ct.display_name()
                    ));
                }
                self.check_block(then_branch)?;
                if let Some(eb) = else_branch {
                    self.check_block(eb)?;
                }
                Ok(())
            }
            Stmt::For {
                var_name,
                var_ty,
                range_start,
                range_end,
                body,
            } => {
                let st = self.check_expr(range_start)?;
                let et = self.check_expr(range_end)?;
                if !matches!(st, Type::Int | Type::Uint) {
                    return Err(format!(
                        "For range start must be integer, got {}",
                        st.display_name()
                    ));
                }
                if !matches!(et, Type::Int | Type::Uint) {
                    return Err(format!(
                        "For range end must be integer, got {}",
                        et.display_name()
                    ));
                }
                let loop_var_ty = match var_ty {
                    Some(tr) => {
                        let resolved = self.resolve_type_ref(tr)?;
                        if !matches!(resolved, Type::Int | Type::Uint) {
                            return Err(format!(
                                "For loop variable '{}' must be int or uint, got {}",
                                var_name,
                                resolved.display_name()
                            ));
                        }
                        resolved
                    }
                    None => Type::Int,
                };
                self.scopes.push(HashMap::new());
                self.insert_var_in_current_scope(var_name.clone(), loop_var_ty);
                self.loop_depth += 1;
                let mut res = Ok(());
                for s in body {
                    if let Err(e) = self.check_stmt(s) {
                        res = Err(e);
                        break;
                    }
                }
                self.loop_depth -= 1;
                self.scopes.pop();
                res
            }
            Stmt::While { cond, body } => {
                let ct = self.check_expr(cond)?;
                if ct != Type::Bool {
                    return Err(format!(
                        "While condition must be bool, got {}",
                        ct.display_name()
                    ));
                }
                self.scopes.push(HashMap::new());
                self.loop_depth += 1;
                let mut res = Ok(());
                for s in body {
                    if let Err(e) = self.check_stmt(s) {
                        res = Err(e);
                        break;
                    }
                }
                self.loop_depth -= 1;
                self.scopes.pop();
                res
            }
            Stmt::Break | Stmt::Continue => {
                if self.loop_depth == 0 {
                    Err("break/continue used outside of a loop".to_string())
                } else {
                    Ok(())
                }
            }
            Stmt::Return(expr_opt) => {
                if self.return_stack.is_empty() {
                    return Err("'return' used outside of a function".to_string());
                }
                let actual_ty = match expr_opt {
                    Some(e) => self.check_expr(e)?,
                    None => Type::Void,
                };
                let (expected_opt, _) = self.return_stack.last().unwrap().clone();
                if let Some(expected) = expected_opt {
                    if !self.is_compatible(&expected, &actual_ty, expr_opt.as_ref()) {
                        return Err(format!(
                            "Return type mismatch: expected {}, got {}",
                            expected.display_name(),
                            actual_ty.display_name()
                        ));
                    }
                }
                if let Some((_, observed)) = self.return_stack.last_mut() {
                    observed.push(actual_ty);
                }
                Ok(())
            }
            Stmt::ExprStmt(expr) => {
                let _ = self.check_expr(expr)?;
                Ok(())
            }
            Stmt::Print { args } => {
                for a in args {
                    let t = self.check_expr(a)?;
                    if t == Type::Void {
                        return Err("Cannot print void expression".to_string());
                    }
                }
                Ok(())
            }
            Stmt::FnInput { targets } => {
                for target in targets {
                    match target {
                        InputTarget::Decl { names, ty } => {
                            let resolved = self.resolve_type_spec(ty)?;
                            if matches!(resolved, Type::Void | Type::Class(_)) {
                                return Err(format!(
                                    "Cannot read input() into type {}",
                                    resolved.display_name()
                                ));
                            }
                            for n in names {
                                self.insert_var_in_current_scope(n.clone(), resolved.clone());
                            }
                        }
                        InputTarget::Var { target } => {
                            let t = self.check_expr(target)?;
                            if matches!(t, Type::Void | Type::Class(_)) {
                                return Err(format!(
                                    "Cannot read input() into type {}",
                                    t.display_name()
                                ));
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }

    fn lookup_class_field(&self, obj_ty: &Type, field: &str) -> Result<Type, String> {
        let Type::Class(cname) = obj_ty else {
            return Err(format!(
                "Cannot access field '{}' on non-class type {}",
                field,
                obj_ty.display_name()
            ));
        };
        let info = self
            .classes
            .get(cname)
            .ok_or_else(|| format!("Unknown class '{}'", cname))?;
        info.fields
            .get(field)
            .cloned()
            .ok_or_else(|| format!("Class '{}' has no field '{}'", cname, field))
    }

    fn check_index_target(&mut self, target: &Expr, index: &Expr) -> Result<Type, String> {
        let it = self.check_expr(index)?;
        if !matches!(it, Type::Int | Type::Uint) {
            return Err(format!(
                "Index must be integer, got {}",
                it.display_name()
            ));
        }
        let tt = self.check_expr(target)?;
        match tt {
            Type::Array1D(elem) => Ok(*elem),
            Type::Table(rows) => {
                if let Expr::Int(r_idx) = index {
                    if *r_idx < 0 || (*r_idx as usize) >= rows.len() {
                        return Err(format!("Row index {} out of bounds", r_idx));
                    }
                    Ok(Type::Array1D(Box::new(rows[*r_idx as usize].clone())))
                } else if let Some(first) = rows.first() {
                    if rows.iter().all(|r| r == first) {
                        Ok(Type::Array1D(Box::new(first.clone())))
                    } else {
                        Err("Dynamic row index on heterogeneous table is not supported during static type checking".to_string())
                    }
                } else {
                    Err("Empty table".to_string())
                }
            }
            Type::String => Ok(Type::Char),
            other => Err(format!("Cannot index type {}", other.display_name())),
        }
    }

    fn check_slice_assign_target(
        &mut self,
        target: &Expr,
        start: Option<&Expr>,
        end: Option<&Expr>,
    ) -> Result<Type, String> {
        if let Some(s) = start {
            let st = self.check_expr(s)?;
            if !matches!(st, Type::Int | Type::Uint) {
                return Err(format!(
                    "Slice start must be integer, got {}",
                    st.display_name()
                ));
            }
        }
        if let Some(e) = end {
            let et = self.check_expr(e)?;
            if !matches!(et, Type::Int | Type::Uint) {
                return Err(format!(
                    "Slice end must be integer, got {}",
                    et.display_name()
                ));
            }
        }
        let tt = self.check_expr(target)?;
        match tt {
            Type::Array1D(elem) => Ok(*elem),
            other => Err(format!(
                "Slice fill assignment target must be an array, got {}",
                other.display_name()
            )),
        }
    }

    fn check_expr(&mut self, expr: &Expr) -> Result<Type, String> {
        match expr {
            Expr::Int(_) => Ok(Type::Int),
            Expr::Float(_) => Ok(Type::Float),
            Expr::Char(_) => Ok(Type::Char),
            Expr::Str(_) => Ok(Type::String),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::SelfRef => self
                .lookup_var("self")
                .ok_or_else(|| "'self' used outside of a class method".to_string()),
            Expr::Ident(name) => self
                .lookup_var(name)
                .ok_or_else(|| format!("Undefined variable '{}'", name)),
            Expr::InputCall => Err(
                "input() without arguments requires a typed declaration like 'N: int = input()'"
                    .to_string(),
            ),
            Expr::Unary { op, expr } => {
                let t = self.check_expr(expr)?;
                match (op, &t) {
                    (UnaryOp::Neg, Type::Int | Type::Float) => Ok(t),
                    (UnaryOp::Not, Type::Bool) => Ok(Type::Bool),
                    _ => Err(format!(
                        "Invalid unary operation {:?} for type {}",
                        op,
                        t.display_name()
                    )),
                }
            }
            Expr::Binary { op, left, right } => {
                let lt = self.check_expr(left)?;
                let rt = self.check_expr(right)?;
                check_binary_type(*op, &lt, &rt, left, right)
            }
            Expr::Index { target, index } => self.check_index_target(target, index),
            Expr::SliceView { target, start, end } => {
                if let Some(s) = start {
                    let st = self.check_expr(s)?;
                    if !matches!(st, Type::Int | Type::Uint) {
                        return Err(format!(
                            "Slice start must be integer, got {}",
                            st.display_name()
                        ));
                    }
                }
                if let Some(e) = end {
                    let et = self.check_expr(e)?;
                    if !matches!(et, Type::Int | Type::Uint) {
                        return Err(format!(
                            "Slice end must be integer, got {}",
                            et.display_name()
                        ));
                    }
                }
                let tt = self.check_expr(target)?;
                match tt {
                    Type::Array1D(elem) => Ok(Type::Array1D(elem)),
                    Type::String => Ok(Type::String),
                    other => Err(format!(
                        "SliceView target must be an array or string, got {}",
                        other.display_name()
                    )),
                }
            }
            Expr::FieldAccess { target, field } => {
                let obj_ty = self.check_expr(target)?;
                self.lookup_class_field(&obj_ty, field)
            }
            Expr::Call { callee, args } => {
                if callee == "len" {
                    if args.len() != 1 {
                        return Err(format!("len() expects 1 argument, got {}", args.len()));
                    }
                    let at = self.check_expr(&args[0])?;
                    if matches!(at, Type::Array1D(_) | Type::String) {
                        return Ok(Type::Int);
                    }
                    return Err(format!(
                        "len() expects array or string, got {}",
                        at.display_name()
                    ));
                }

                if let Some(class_info) = self.classes.get(callee).cloned() {
                    if let Some(init_sig) = class_info.methods.get("init") {
                        let expected_params = &init_sig.params[1..];
                        if args.len() != expected_params.len() {
                            return Err(format!(
                                "Constructor '{}' expects {} arguments, got {}",
                                callee,
                                expected_params.len(),
                                args.len()
                            ));
                        }
                        for (i, (arg_expr, exp_ty)) in
                            args.iter().zip(expected_params.iter()).enumerate()
                        {
                            let act_ty = self.check_expr(arg_expr)?;
                            if !self.is_compatible(exp_ty, &act_ty, Some(arg_expr)) {
                                return Err(format!(
                                    "Argument {} of '{}.init' expects {}, got {}",
                                    i + 1,
                                    callee,
                                    exp_ty.display_name(),
                                    act_ty.display_name()
                                ));
                            }
                        }
                    } else if !args.is_empty() {
                        return Err(format!(
                            "Class '{}' has no 'init' constructor taking {} arguments",
                            callee,
                            args.len()
                        ));
                    }
                    return Ok(Type::Class(callee.clone()));
                }

                if self.functions.contains_key(callee) {
                    let need_infer = self
                        .functions
                        .get(callee)
                        .is_some_and(|s| s.ret.is_none());
                    if need_infer {
                        if self.checking_fns.contains(callee) {
                            return Err(format!(
                                "Cannot infer return type for recursive function '{}'; please specify '-> <type>'",
                                callee
                            ));
                        }
                        self.ensure_fn_checked(callee)?;
                    }
                    let sig = self.functions.get(callee).cloned().unwrap();
                    if args.len() != sig.params.len() {
                        return Err(format!(
                            "Function '{}' expects {} arguments, got {}",
                            callee,
                            sig.params.len(),
                            args.len()
                        ));
                    }
                    for (i, (arg_expr, exp_ty)) in args.iter().zip(sig.params.iter()).enumerate() {
                        let act_ty = self.check_expr(arg_expr)?;
                        if !self.is_compatible(exp_ty, &act_ty, Some(arg_expr)) {
                            return Err(format!(
                                "Argument {} of '{}' expects {}, got {}",
                                i + 1,
                                callee,
                                exp_ty.display_name(),
                                act_ty.display_name()
                            ));
                        }
                    }
                    return Ok(sig.ret.unwrap_or(Type::Void));
                }

                Err(format!("Undefined function or class '{}'", callee))
            }
            Expr::MethodCall {
                target,
                method,
                args,
            } => {
                let obj_ty = self.check_expr(target)?;
                let Type::Class(cname) = obj_ty else {
                    return Err(format!(
                        "Cannot call method '{}' on non-class type {}",
                        method,
                        obj_ty.display_name()
                    ));
                };
                let sig = self
                    .classes
                    .get(&cname)
                    .and_then(|c| c.methods.get(method))
                    .cloned()
                    .ok_or_else(|| format!("Class '{}' has no method '{}'", cname, method))?;

                let expected_params = &sig.params[1..];
                if args.len() != expected_params.len() {
                    return Err(format!(
                        "Method '{}.{}' expects {} arguments, got {}",
                        cname,
                        method,
                        expected_params.len(),
                        args.len()
                    ));
                }
                for (i, (arg_expr, exp_ty)) in args.iter().zip(expected_params.iter()).enumerate() {
                    let act_ty = self.check_expr(arg_expr)?;
                    if !self.is_compatible(exp_ty, &act_ty, Some(arg_expr)) {
                        return Err(format!(
                            "Argument {} of '{}.{}' expects {}, got {}",
                            i + 1,
                            cname,
                            method,
                            exp_ty.display_name(),
                            act_ty.display_name()
                        ));
                    }
                }
                Ok(sig.ret.unwrap_or(Type::Void))
            }
        }
    }
}

fn check_binary_type(
    op: BinaryOp,
    l: &Type,
    r: &Type,
    left_expr: &Expr,
    right_expr: &Expr,
) -> Result<Type, String> {
    let same_or_uint_lit = l == r
        || (*l == Type::Uint && matches!(right_expr, Expr::Int(n) if *n >= 0))
        || (*r == Type::Uint && matches!(left_expr, Expr::Int(n) if *n >= 0));

    if !same_or_uint_lit {
        return Err(format!(
            "Type mismatch for {:?}: {} and {}",
            op,
            l.display_name(),
            r.display_name()
        ));
    }

    let unified = if *l == Type::Uint || *r == Type::Uint {
        Type::Uint
    } else {
        l.clone()
    };

    match op {
        BinaryOp::Add => match unified {
            Type::Int | Type::Uint | Type::Float | Type::String => Ok(unified),
            _ => Err(format!("Cannot add values of type {}", unified.display_name())),
        },
        BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => match unified {
            Type::Int | Type::Uint | Type::Float => Ok(unified),
            _ => Err(format!(
                "Invalid arithmetic {:?} on type {}",
                op,
                unified.display_name()
            )),
        },
        BinaryOp::Mod => match unified {
            Type::Int | Type::Uint => Ok(unified),
            _ => Err(format!(
                "Modulo requires integer type, got {}",
                unified.display_name()
            )),
        },
        BinaryOp::Eq | BinaryOp::Neq => {
            if unified == Type::Void {
                Err("Cannot compare void values".to_string())
            } else {
                Ok(Type::Bool)
            }
        }
        BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => match unified {
            Type::Int | Type::Uint | Type::Float | Type::Char | Type::String => Ok(Type::Bool),
            _ => Err(format!(
                "Cannot order-compare type {}",
                unified.display_name()
            )),
        },
        BinaryOp::And | BinaryOp::Or => {
            if unified == Type::Bool {
                Ok(Type::Bool)
            } else {
                Err(format!(
                    "Logical operator requires bool, got {}",
                    unified.display_name()
                ))
            }
        }
    }
}

fn definitely_returns(stmts: &[Stmt]) -> bool {
    for s in stmts {
        match s {
            Stmt::Return(_) => return true,
            Stmt::If {
                then_branch,
                else_branch: Some(eb),
                ..
            } => {
                if definitely_returns(then_branch) && definitely_returns(eb) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}
