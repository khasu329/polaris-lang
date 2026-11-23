use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostic::{Diagnostic, Span};
use crate::module::ModuleProgram;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Uint,
    Float,
    Char,
    String,
    Bool,
    Void,
    Array(Box<Type>),
    Table(Vec<Type>),
    Ref(Box<Type>),
    Class {
        module: String,
        name: String,
        type_args: Vec<Type>,
    },
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
            Type::Array(elem) => format!("{}[]", elem.display_name()),
            Type::Table(rows) => {
                let inner: Vec<String> = rows.iter().map(|r| r.display_name()).collect();
                format!("[{}][]", inner.join(", "))
            }
            Type::Ref(inner) => format!("&{}", inner.display_name()),
            Type::Class {
                name, type_args, ..
            } => {
                if type_args.is_empty() {
                    name.clone()
                } else {
                    let args: Vec<String> = type_args.iter().map(|t| t.display_name()).collect();
                    format!("{}<{}>", name, args.join(", "))
                }
            }
        }
    }

    pub fn deref(&self) -> &Type {
        match self {
            Type::Ref(inner) => inner.deref(),
            other => other,
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
    method_decls: HashMap<String, FnDecl>,
}

struct ModuleEnv {
    display_name: String,
    imports: HashMap<String, String>,
    globals: HashMap<String, Type>,
    fn_decls: HashMap<String, FnDecl>,
    functions: HashMap<String, FnSig>,
    checked_fns: HashSet<String>,
    checking_fns: HashSet<String>,
    instantiated_fns: HashMap<(String, Vec<Type>), FnSig>,
    checking_inst_fns: HashSet<(String, Vec<Type>)>,
    class_decls: HashMap<String, ClassDecl>,
    classes: HashMap<(String, Vec<Type>), ClassInfo>,
    checked_classes: HashSet<(String, Vec<Type>)>,
    checking_classes: HashSet<(String, Vec<Type>)>,
    instantiated_methods: HashMap<(String, Vec<Type>, String, Vec<Type>), FnSig>,
    checking_inst_methods: HashSet<(String, Vec<Type>, String, Vec<Type>)>,
}

impl ModuleEnv {
    fn new(display_name: String, imports: HashMap<String, String>) -> Self {
        Self {
            display_name,
            imports,
            globals: HashMap::new(),
            fn_decls: HashMap::new(),
            functions: HashMap::new(),
            checked_fns: HashSet::new(),
            checking_fns: HashSet::new(),
            instantiated_fns: HashMap::new(),
            checking_inst_fns: HashSet::new(),
            class_decls: HashMap::new(),
            classes: HashMap::new(),
            checked_classes: HashSet::new(),
            checking_classes: HashSet::new(),
            instantiated_methods: HashMap::new(),
            checking_inst_methods: HashSet::new(),
        }
    }
}

pub struct TypeChecker {
    modules: HashMap<String, ModuleEnv>,
    current_module: String,
    scopes: Vec<HashMap<String, Type>>,
    type_param_bindings: HashMap<String, Type>,
    loop_depth: usize,
    return_stack: Vec<(Option<Type>, Vec<Type>)>,
    instantiation_depth: usize,
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut modules = HashMap::new();
        modules.insert(
            "__main__".to_string(),
            ModuleEnv::new("<main>".to_string(), HashMap::new()),
        );
        Self {
            modules,
            current_module: "__main__".to_string(),
            scopes: Vec::new(),
            type_param_bindings: HashMap::new(),
            loop_depth: 0,
            return_stack: Vec::new(),
            instantiation_depth: 0,
        }
    }

    fn err(&self, span: Span, msg: impl Into<String>) -> Diagnostic {
        let file = self
            .modules
            .get(&self.current_module)
            .map(|m| m.display_name.clone())
            .unwrap_or_else(|| "<main>".to_string());
        Diagnostic::new("type", span, msg).with_file(file)
    }

    fn current_env(&self) -> &ModuleEnv {
        self.modules
            .get(&self.current_module)
            .expect("current module must exist")
    }

    fn env(&self, mod_id: &str) -> &ModuleEnv {
        self.modules.get(mod_id).expect("module must exist")
    }

    fn env_mut(&mut self, mod_id: &str) -> &mut ModuleEnv {
        self.modules.get_mut(mod_id).expect("module must exist")
    }

    pub fn check_program(&mut self, stmts: &[Stmt]) -> Result<(), Diagnostic> {
        self.modules.clear();
        self.modules.insert(
            "__main__".to_string(),
            ModuleEnv::new("<main>".to_string(), HashMap::new()),
        );
        self.current_module = "__main__".to_string();
        self.check_single_module("__main__", stmts)
    }

    pub fn check_module_program(&mut self, program: &ModuleProgram) -> Result<(), Diagnostic> {
        self.modules.clear();
        for (id, lm) in &program.modules {
            self.modules.insert(
                id.clone(),
                ModuleEnv::new(lm.display_name.clone(), lm.imports.clone()),
            );
        }

        for mod_id in &program.load_order {
            let stmts = &program.modules[mod_id].stmts;
            self.check_single_module(mod_id, stmts)?;
        }
        Ok(())
    }

    fn check_single_module(&mut self, mod_id: &str, stmts: &[Stmt]) -> Result<(), Diagnostic> {
        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let saved_scopes = std::mem::take(&mut self.scopes);
        let saved_bindings = std::mem::take(&mut self.type_param_bindings);
        let saved_loop = self.loop_depth;
        self.loop_depth = 0;

        let res = (|| {
            // Pass 1a: Pre-register all class declarations
            for stmt in stmts {
                if let StmtKind::ClassDecl(cd) = &stmt.node {
                    self.register_class_decl(mod_id, cd)?;
                }
            }
            // Pass 1b: Pre-register non-generic class members and all fn declarations
            for stmt in stmts {
                match &stmt.node {
                    StmtKind::ClassDecl(cd) if cd.type_params.is_empty() => {
                        self.ensure_class_instantiated(mod_id, &cd.name, &[], stmt.span)?;
                    }
                    StmtKind::FnDecl(fd) => {
                        self.register_fn_decl(mod_id, fd)?;
                    }
                    _ => {}
                }
            }
            // Pass 2: Check top-level statements in source order
            for stmt in stmts {
                self.check_stmt(stmt, true)?;
            }
            Ok(())
        })();

        self.current_module = prev_mod;
        self.scopes = saved_scopes;
        self.type_param_bindings = saved_bindings;
        self.loop_depth = saved_loop;
        res
    }

    fn lookup_var(&self, name: &str) -> Option<Type> {
        for frame in self.scopes.iter().rev() {
            if let Some(t) = frame.get(name) {
                return Some(t.clone());
            }
        }
        self.current_env().globals.get(name).cloned()
    }

    fn insert_var_in_current_scope(&mut self, name: String, ty: Type, span: Span) -> Result<(), Diagnostic> {
        if self.scopes.is_empty() && self.current_env().imports.contains_key(&name) {
            return Err(self
                .err(span, format!("Variable name '{}' conflicts with imported module alias", name))
                .with_hint("Choose another variable name or alias the import with 'as'"));
        }
        if let Some(frame) = self.scopes.last_mut() {
            frame.insert(name, ty);
        } else {
            let mid = self.current_module.clone();
            self.env_mut(&mid).globals.insert(name, ty);
        }
        Ok(())
    }

    fn resolve_target_module(&self, alias: Option<&str>, span: Span) -> Result<String, Diagnostic> {
        match alias {
            None => Ok(self.current_module.clone()),
            Some(a) => self
                .current_env()
                .imports
                .get(a)
                .cloned()
                .ok_or_else(|| self.err(span, format!("Unknown module '{}'", a))),
        }
    }

    fn resolve_type_expr(&mut self, te: &TypeExpr) -> Result<Type, Diagnostic> {
        match &te.node {
            TypeExprKind::Ref(inner) => {
                let inner_ty = self.resolve_type_expr(inner)?;
                if inner_ty == Type::Void {
                    return Err(self.err(te.span, "Cannot create reference to void"));
                }
                if matches!(inner_ty, Type::Ref(_)) {
                    return Err(self.err(te.span, "Nested reference type '&&T' is not supported"));
                }
                Ok(Type::Ref(Box::new(inner_ty)))
            }
            TypeExprKind::Array { elem, len } => {
                if let Some(len_expr) = len {
                    let lt = self.check_expr_value(len_expr, None)?;
                    if !matches!(lt, Type::Int | Type::Uint) {
                        return Err(self.err(
                            len_expr.span,
                            format!("Array length must be integer, got {}", lt.display_name()),
                        ));
                    }
                }
                let et = self.resolve_type_expr(elem)?;
                if et == Type::Void {
                    return Err(self.err(te.span, "Array element type cannot be void"));
                }
                if matches!(et, Type::Ref(_)) {
                    return Err(self.err(te.span, "Arrays of references are not supported"));
                }
                Ok(Type::Array(Box::new(et)))
            }
            TypeExprKind::Table { row_types, len } => {
                if let Some(len_expr) = len {
                    let lt = self.check_expr_value(len_expr, None)?;
                    if !matches!(lt, Type::Int | Type::Uint) {
                        return Err(self.err(
                            len_expr.span,
                            format!("Table length must be integer, got {}", lt.display_name()),
                        ));
                    }
                }
                let mut rows = Vec::with_capacity(row_types.len());
                for rt in row_types {
                    let t = self.resolve_type_expr(rt)?;
                    if t == Type::Void || matches!(t, Type::Ref(_)) {
                        return Err(self.err(
                            rt.span,
                            format!("Invalid table row element type {}", t.display_name()),
                        ));
                    }
                    rows.push(t);
                }
                Ok(Type::Table(rows))
            }
            TypeExprKind::Named {
                module,
                name,
                type_args,
            } => {
                if module.is_none() {
                    if let Some(bound) = self.type_param_bindings.get(name).cloned() {
                        if !type_args.is_empty() {
                            return Err(self.err(
                                te.span,
                                format!("Type parameter '{}' cannot accept type arguments", name),
                            ));
                        }
                        return Ok(bound);
                    }
                    let prim = match name.as_str() {
                        "int" => Some(Type::Int),
                        "uint" => Some(Type::Uint),
                        "float" => Some(Type::Float),
                        "char" => Some(Type::Char),
                        "string" => Some(Type::String),
                        "bool" => Some(Type::Bool),
                        "void" => Some(Type::Void),
                        _ => None,
                    };
                    if let Some(p) = prim {
                        if !type_args.is_empty() {
                            return Err(self.err(
                                te.span,
                                format!("Primitive type '{}' does not accept type arguments", name),
                            ));
                        }
                        return Ok(p);
                    }
                }

                let target_mod = self.resolve_target_module(module.as_deref(), te.span)?;
                let mut resolved_args = Vec::with_capacity(type_args.len());
                for ta in type_args {
                    let t = self.resolve_type_expr(ta)?;
                    if t == Type::Void {
                        return Err(self.err(ta.span, "Generic type argument cannot be void"));
                    }
                    resolved_args.push(t);
                }

                if self.env(&target_mod).class_decls.contains_key(name) {
                    self.ensure_class_instantiated(&target_mod, name, &resolved_args, te.span)?;
                    Ok(Type::Class {
                        module: target_mod,
                        name: name.clone(),
                        type_args: resolved_args,
                    })
                } else {
                    Err(self.err(te.span, format!("Unknown type '{}'", name)))
                }
            }
        }
    }

    fn is_compatible(&self, expected: &Type, actual: &Type, expr: Option<&Expr>) -> bool {
        if expected == actual {
            return true;
        }
        if *expected == Type::Uint && *actual == Type::Int {
            if let Some(e) = expr {
                if let ExprKind::Int(n) = e.node {
                    return n >= 0;
                }
            }
        }
        false
    }

    fn register_class_decl(&mut self, mod_id: &str, cd: &ClassDecl) -> Result<(), Diagnostic> {
        let conflict = self.env(mod_id).class_decls.contains_key(&cd.name)
            || self.env(mod_id).fn_decls.contains_key(&cd.name)
            || self.env(mod_id).imports.contains_key(&cd.name);
        if conflict {
            return Err(self.err(
                cd.span,
                format!("Duplicate class, function, or import name '{}'", cd.name),
            ));
        }
        if is_builtin_type_name(&cd.name) {
            return Err(self.err(
                cd.span,
                format!("Cannot name a class after built-in type '{}'", cd.name),
            ));
        }
        self.env_mut(mod_id)
            .class_decls
            .insert(cd.name.clone(), cd.clone());
        Ok(())
    }

    fn register_fn_decl(&mut self, mod_id: &str, fd: &FnDecl) -> Result<(), Diagnostic> {
        let conflict = self.env(mod_id).fn_decls.contains_key(&fd.name)
            || self.env(mod_id).class_decls.contains_key(&fd.name)
            || self.env(mod_id).imports.contains_key(&fd.name);
        if conflict {
            return Err(self.err(
                fd.span,
                format!("Duplicate function, class, or import name '{}'", fd.name),
            ));
        }
        if matches!(fd.name.as_str(), "print" | "input" | "len") {
            return Err(self.err(
                fd.span,
                format!("Cannot redefine built-in function '{}'", fd.name),
            ));
        }
        self.env_mut(mod_id)
            .fn_decls
            .insert(fd.name.clone(), fd.clone());

        if fd.type_params.is_empty() {
            let sig = self.build_fn_sig(mod_id, fd, &HashMap::new())?;
            if let Some(Type::Ref(_)) = sig.ret {
                return Err(self
                    .err(fd.span, "Functions cannot return a reference type '&T'")
                    .with_hint("Return a value or modify an argument passed via '&T'"));
            }
            self.env_mut(mod_id).functions.insert(fd.name.clone(), sig);
        }
        Ok(())
    }

    fn build_fn_sig(
        &mut self,
        mod_id: &str,
        fd: &FnDecl,
        bindings: &HashMap<String, Type>,
    ) -> Result<FnSig, Diagnostic> {
        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let prev_bindings = std::mem::replace(&mut self.type_param_bindings, bindings.clone());

        let res = (|| {
            let mut params = Vec::with_capacity(fd.params.len());
            for p in &fd.params {
                let ts = p.ty.as_ref().ok_or_else(|| {
                    self.err(
                        p.span,
                        format!("Parameter '{}' in fn '{}' requires a type", p.name, fd.name),
                    )
                })?;
                let resolved = self.resolve_type_expr(ts)?;
                if resolved == Type::Void {
                    return Err(self.err(p.span, format!("Parameter '{}' cannot be void", p.name)));
                }
                params.push(resolved);
            }
            let ret = match &fd.return_type {
                Some(ts) => {
                    let rt = self.resolve_type_expr(ts)?;
                    if matches!(rt, Type::Ref(_)) {
                        return Err(self
                            .err(ts.span, "Functions cannot return a reference type '&T'")
                            .with_hint("Return by value or mutate a parameter passed as '&T'"));
                    }
                    Some(rt)
                }
                None => None,
            };
            Ok(FnSig { params, ret })
        })();

        self.current_module = prev_mod;
        self.type_param_bindings = prev_bindings;
        res
    }

    fn ensure_class_instantiated(
        &mut self,
        mod_id: &str,
        class_name: &str,
        type_args: &[Type],
        use_span: Span,
    ) -> Result<(), Diagnostic> {
        let key = (class_name.to_string(), type_args.to_vec());
        if self.env(mod_id).classes.contains_key(&key) {
            return Ok(());
        }

        let cd = self
            .env(mod_id)
            .class_decls
            .get(class_name)
            .cloned()
            .ok_or_else(|| self.err(use_span, format!("Unknown class '{}'", class_name)))?;

        if cd.type_params.len() != type_args.len() {
            return Err(self.err(
                use_span,
                format!(
                    "Class '{}' expects {} type argument(s), got {}",
                    class_name,
                    cd.type_params.len(),
                    type_args.len()
                ),
            ));
        }

        if self.instantiation_depth >= 64 {
            return Err(self.err(
                use_span,
                format!("Generic instantiation depth exceeded while instantiating '{}'", class_name),
            ));
        }

        let mut bindings = HashMap::new();
        for (tp, ta) in cd.type_params.iter().zip(type_args.iter()) {
            bindings.insert(tp.clone(), ta.clone());
        }

        // Insert placeholder first to allow self-referential fields like Node<T>[]
        self.env_mut(mod_id).classes.insert(
            key.clone(),
            ClassInfo {
                fields: HashMap::new(),
                methods: HashMap::new(),
                method_decls: HashMap::new(),
            },
        );

        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let prev_bindings = std::mem::replace(&mut self.type_param_bindings, bindings.clone());
        let saved_scopes = std::mem::take(&mut self.scopes);
        self.instantiation_depth += 1;

        let build_res = (|| -> Result<ClassInfo, Diagnostic> {
            let mut fields = HashMap::new();
            for f in &cd.fields {
                if fields.contains_key(&f.name) {
                    return Err(self.err(
                        f.span,
                        format!("Duplicate field '{}' in class '{}'", f.name, cd.name),
                    ));
                }
                let ftype = match (&f.ty, &f.default_expr) {
                    (Some(ts), Some(def)) => {
                        let expected = self.resolve_type_expr(ts)?;
                        let actual = self.check_expr_value(def, Some(&expected))?;
                        if !self.is_compatible(&expected, &actual, Some(def)) {
                            return Err(self.err(
                                f.span,
                                format!(
                                    "Field '{}' in class '{}' has type {}, cannot initialize with {}",
                                    f.name,
                                    cd.name,
                                    expected.display_name(),
                                    actual.display_name()
                                ),
                            ));
                        }
                        expected
                    }
                    (Some(ts), None) => {
                        let t = self.resolve_type_expr(ts)?;
                        if t == Type::Void || matches!(t, Type::Ref(_)) {
                            return Err(self.err(
                                f.span,
                                format!(
                                    "Field '{}' in class '{}' cannot have type {}",
                                    f.name,
                                    cd.name,
                                    t.display_name()
                                ),
                            ));
                        }
                        t
                    }
                    (None, Some(def)) => {
                        let inferred = self.check_expr_value(def, None)?;
                        if inferred == Type::Void || matches!(inferred, Type::Ref(_)) {
                            return Err(self.err(
                                f.span,
                                format!(
                                    "Field '{}' in class '{}' cannot have type {}",
                                    f.name,
                                    cd.name,
                                    inferred.display_name()
                                ),
                            ));
                        }
                        inferred
                    }
                    (None, None) => {
                        return Err(self.err(
                            f.span,
                            format!("Field '{}' in class '{}' needs a type or initializer", f.name, cd.name),
                        ));
                    }
                };
                fields.insert(f.name.clone(), ftype);
            }

            let self_ty = Type::Class {
                module: mod_id.to_string(),
                name: cd.name.clone(),
                type_args: type_args.to_vec(),
            };

            let mut methods = HashMap::new();
            let mut method_decls = HashMap::new();
            for m in &cd.methods {
                if method_decls.contains_key(&m.name) {
                    return Err(self.err(
                        m.span,
                        format!("Duplicate method '{}' in class '{}'", m.name, cd.name),
                    ));
                }
                if m.params.is_empty() || m.params[0].name != "self" {
                    return Err(self.err(
                        m.span,
                        format!(
                            "Method '{}' in class '{}' must take 'self' as its first parameter",
                            m.name, cd.name
                        ),
                    ));
                }
                method_decls.insert(m.name.clone(), m.clone());

                if m.type_params.is_empty() {
                    let mut param_types = Vec::new();
                    for (idx, p) in m.params.iter().enumerate() {
                        if idx == 0 {
                            if let Some(ts) = &p.ty {
                                let t = self.resolve_type_expr(ts)?;
                                if t != self_ty {
                                    return Err(self.err(
                                        p.span,
                                        format!(
                                            "'self' parameter in class '{}' must have type {}",
                                            cd.name,
                                            self_ty.display_name()
                                        ),
                                    ));
                                }
                            }
                            param_types.push(self_ty.clone());
                        } else {
                            let ts = p.ty.as_ref().ok_or_else(|| {
                                self.err(p.span, format!("Parameter '{}' requires a type", p.name))
                            })?;
                            let resolved = self.resolve_type_expr(ts)?;
                            if resolved == Type::Void {
                                return Err(self.err(
                                    p.span,
                                    format!("Parameter '{}' cannot be void", p.name),
                                ));
                            }
                            param_types.push(resolved);
                        }
                    }
                    let ret = match &m.return_type {
                        Some(ts) => {
                            let rt = self.resolve_type_expr(ts)?;
                            if matches!(rt, Type::Ref(_)) {
                                return Err(self.err(
                                    ts.span,
                                    "Methods cannot return a reference type '&T'",
                                ));
                            }
                            Some(rt)
                        }
                        None => None,
                    };
                    if m.name == "init" {
                        if let Some(rt) = &ret {
                            if *rt != Type::Void {
                                return Err(self.err(
                                    m.span,
                                    "Constructor 'init' cannot return a non-void value",
                                ));
                            }
                        }
                    }
                    if m.name == "drop" {
                        if m.params.len() != 1 || !m.type_params.is_empty() {
                            return Err(self.err(m.span, "Destructor 'drop' must only take 'self'"));
                        }
                        if let Some(rt) = &ret {
                            if *rt != Type::Void {
                                return Err(self.err(
                                    m.span,
                                    "Destructor 'drop' cannot return a non-void value",
                                ));
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
            }

            Ok(ClassInfo {
                fields,
                methods,
                method_decls,
            })
        })();

        self.instantiation_depth -= 1;
        self.scopes = saved_scopes;
        self.type_param_bindings = prev_bindings;
        self.current_module = prev_mod;

        match build_res {
            Ok(info) => {
                self.env_mut(mod_id).classes.insert(key.clone(), info);
            }
            Err(e) => {
                self.env_mut(mod_id).classes.remove(&key);
                return Err(e);
            }
        }

        // Check all non-generic method bodies for this class instantiation (with cycle protection)
        self.check_class_bodies_instantiation(mod_id, class_name, type_args)?;
        Ok(())
    }

    fn check_class_bodies_instantiation(
        &mut self,
        mod_id: &str,
        class_name: &str,
        type_args: &[Type],
    ) -> Result<(), Diagnostic> {
        let key = (class_name.to_string(), type_args.to_vec());
        if self.env(mod_id).checked_classes.contains(&key)
            || self.env(mod_id).checking_classes.contains(&key)
        {
            return Ok(());
        }
        self.env_mut(mod_id).checking_classes.insert(key.clone());

        let cd = self.env(mod_id).class_decls.get(class_name).cloned().unwrap();
        let class_info = self.env(mod_id).classes.get(&key).cloned().unwrap();

        let mut bindings = HashMap::new();
        for (tp, ta) in cd.type_params.iter().zip(type_args.iter()) {
            bindings.insert(tp.clone(), ta.clone());
        }

        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let prev_bindings = std::mem::replace(&mut self.type_param_bindings, bindings);

        let mut res = Ok(());
        for m in &cd.methods {
            if !m.type_params.is_empty() {
                continue;
            }
            let sig = class_info.methods.get(&m.name).cloned().unwrap();
            let params: Vec<(String, Type)> = m
                .params
                .iter()
                .map(|p| p.name.clone())
                .zip(sig.params.clone())
                .collect();
            match self.check_fn_body(
                &format!("{}.{}", cd.name, m.name),
                &params,
                sig.ret,
                &m.body,
                m.span,
            ) {
                Ok(inferred_ret) => {
                    if let Some(cinfo) = self.env_mut(mod_id).classes.get_mut(&key) {
                        if let Some(msig) = cinfo.methods.get_mut(&m.name) {
                            msig.ret = Some(inferred_ret);
                        }
                    }
                }
                Err(e) => {
                    res = Err(e);
                    break;
                }
            }
        }

        self.type_param_bindings = prev_bindings;
        self.current_module = prev_mod;
        self.env_mut(mod_id).checking_classes.remove(&key);
        if res.is_ok() {
            self.env_mut(mod_id).checked_classes.insert(key);
        }
        res
    }

    fn ensure_fn_checked(&mut self, mod_id: &str, name: &str, span: Span) -> Result<(), Diagnostic> {
        if self.env(mod_id).checked_fns.contains(name) {
            return Ok(());
        }
        let fd = self
            .env(mod_id)
            .fn_decls
            .get(name)
            .cloned()
            .ok_or_else(|| self.err(span, format!("Undefined function '{}'", name)))?;

        if !self.env(mod_id).functions.contains_key(name) {
            let sig = self.build_fn_sig(mod_id, &fd, &HashMap::new())?;
            self.env_mut(mod_id).functions.insert(name.to_string(), sig);
        }
        let sig = self.env(mod_id).functions.get(name).cloned().unwrap();
        let params: Vec<(String, Type)> = fd
            .params
            .iter()
            .map(|p| p.name.clone())
            .zip(sig.params.clone())
            .collect();

        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let prev_bindings = std::mem::take(&mut self.type_param_bindings);
        self.env_mut(mod_id).checking_fns.insert(name.to_string());

        let res = self.check_fn_body(name, &params, sig.ret, &fd.body, fd.span);

        self.env_mut(mod_id).checking_fns.remove(name);
        self.type_param_bindings = prev_bindings;
        self.current_module = prev_mod;

        let inferred_ret = res?;
        if let Some(entry) = self.env_mut(mod_id).functions.get_mut(name) {
            entry.ret = Some(inferred_ret);
        }
        self.env_mut(mod_id).checked_fns.insert(name.to_string());
        Ok(())
    }

    fn ensure_generic_fn_instantiated(
        &mut self,
        mod_id: &str,
        name: &str,
        type_args: &[Type],
        use_span: Span,
    ) -> Result<FnSig, Diagnostic> {
        let key = (name.to_string(), type_args.to_vec());
        if let Some(sig) = self.env(mod_id).instantiated_fns.get(&key) {
            return Ok(sig.clone());
        }

        let fd = self
            .env(mod_id)
            .fn_decls
            .get(name)
            .cloned()
            .ok_or_else(|| self.err(use_span, format!("Undefined generic function '{}'", name)))?;

        if fd.type_params.len() != type_args.len() {
            return Err(self.err(
                use_span,
                format!(
                    "Generic function '{}' expects {} type argument(s), got {}",
                    name,
                    fd.type_params.len(),
                    type_args.len()
                ),
            ));
        }

        if self.env(mod_id).checking_inst_fns.contains(&key) {
            let mut bindings = HashMap::new();
            for (tp, ta) in fd.type_params.iter().zip(type_args.iter()) {
                bindings.insert(tp.clone(), ta.clone());
            }
            let sig = self.build_fn_sig(mod_id, &fd, &bindings)?;
            if sig.ret.is_none() {
                return Err(self
                    .err(
                        use_span,
                        format!(
                            "Cannot infer return type for recursive generic function '{}'; please specify '-> <type>'",
                            name
                        ),
                    ));
            }
            return Ok(sig);
        }

        if self.instantiation_depth >= 64 {
            return Err(self.err(
                use_span,
                format!(
                    "Generic instantiation depth exceeded while instantiating '{}'",
                    name
                ),
            ));
        }

        let mut bindings = HashMap::new();
        for (tp, ta) in fd.type_params.iter().zip(type_args.iter()) {
            bindings.insert(tp.clone(), ta.clone());
        }

        let mut sig = self.build_fn_sig(mod_id, &fd, &bindings)?;
        let params: Vec<(String, Type)> = fd
            .params
            .iter()
            .map(|p| p.name.clone())
            .zip(sig.params.clone())
            .collect();

        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let prev_bindings = std::mem::replace(&mut self.type_param_bindings, bindings);
        self.instantiation_depth += 1;
        self.env_mut(mod_id).checking_inst_fns.insert(key.clone());

        let body_res = self.check_fn_body(name, &params, sig.ret.clone(), &fd.body, fd.span);

        self.env_mut(mod_id).checking_inst_fns.remove(&key);
        self.instantiation_depth -= 1;
        self.type_param_bindings = prev_bindings;
        self.current_module = prev_mod;

        let inferred_ret = body_res.map_err(|mut d| {
            let targs_str: Vec<String> = type_args.iter().map(|t| t.display_name()).collect();
            let note = format!("While instantiating '{}<{}>' from line {}:{}", name, targs_str.join(", "), use_span.line, use_span.col);
            d.hint = Some(match d.hint {
                Some(h) => format!("{}\n  note: {}", h, note),
                None => note,
            });
            d
        })?;

        sig.ret = Some(inferred_ret);
        self.env_mut(mod_id)
            .instantiated_fns
            .insert(key, sig.clone());
        Ok(sig)
    }

    fn check_fn_body(
        &mut self,
        fn_name: &str,
        params: &[(String, Type)],
        explicit_ret: Option<Type>,
        body: &[Stmt],
        fn_span: Span,
    ) -> Result<Type, Diagnostic> {
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
            if let Err(e) = self.check_stmt(stmt, false) {
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
                    return Err(self.err(
                        fn_span,
                        format!(
                            "Function '{}' declares return type {} but may not return a value on all paths",
                            fn_name,
                            rt.display_name()
                        ),
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
                            return Err(self.err(
                                fn_span,
                                format!(
                                    "Conflicting inferred return types in '{}': {} vs {}",
                                    fn_name,
                                    first.display_name(),
                                    t.display_name()
                                ),
                            ));
                        }
                    }
                    first
                }
            }
        };

        Ok(final_ret)
    }

    fn check_block(&mut self, stmts: &[Stmt]) -> Result<(), Diagnostic> {
        self.scopes.push(HashMap::new());
        let mut res = Ok(());
        for stmt in stmts {
            if let Err(e) = self.check_stmt(stmt, false) {
                res = Err(e);
                break;
            }
        }
        self.scopes.pop();
        res
    }

    fn check_stmt(&mut self, stmt: &Stmt, is_top_level: bool) -> Result<(), Diagnostic> {
        match &stmt.node {
            StmtKind::Import { .. } => {
                if !is_top_level {
                    return Err(self.err(
                        stmt.span,
                        "'import' statements are only allowed at the top level",
                    ));
                }
                Ok(())
            }
            StmtKind::FnDecl(fd) => {
                let mid = self.current_module.clone();
                if !self.env(&mid).fn_decls.contains_key(&fd.name) {
                    self.register_fn_decl(&mid, fd)?;
                }
                if fd.type_params.is_empty() {
                    self.ensure_fn_checked(&mid, &fd.name, fd.span)?;
                }
                Ok(())
            }
            StmtKind::ClassDecl(cd) => {
                let mid = self.current_module.clone();
                if !self.env(&mid).class_decls.contains_key(&cd.name) {
                    self.register_class_decl(&mid, cd)?;
                }
                if cd.type_params.is_empty() {
                    self.ensure_class_instantiated(&mid, &cd.name, &[], cd.span)?;
                }
                Ok(())
            }
            StmtKind::VarDecl { names, ty, init } => {
                let resolved_ty = self.resolve_type_expr(ty)?;
                if resolved_ty == Type::Void {
                    return Err(self.err(stmt.span, "Cannot declare variable of type void"));
                }
                if let Some(init_expr) = init {
                    if matches!(init_expr.node, ExprKind::InputCall) {
                        if matches!(
                            resolved_ty,
                            Type::Table(_) | Type::Class { .. } | Type::Ref(_)
                        ) {
                            return Err(self.err(
                                init_expr.span,
                                format!(
                                    "Direct input() initialization is not supported for {}",
                                    resolved_ty.display_name()
                                ),
                            ));
                        }
                    } else {
                        let actual = if matches!(resolved_ty, Type::Ref(_)) {
                            self.check_expr_raw(init_expr, Some(&resolved_ty))?
                        } else {
                            self.check_expr_value(init_expr, Some(&resolved_ty))?
                        };
                        if !self.is_compatible(&resolved_ty, &actual, Some(init_expr)) {
                            return Err(self.err(
                                init_expr.span,
                                format!(
                                    "Type mismatch in declaration of '{}': expected {}, got {}",
                                    names.join(", "),
                                    resolved_ty.display_name(),
                                    actual.display_name()
                                ),
                            ));
                        }
                    }
                } else {
                    if matches!(resolved_ty, Type::Ref(_)) {
                        return Err(self
                            .err(
                                stmt.span,
                                format!(
                                    "Reference variable '{}' must be initialized upon declaration",
                                    names.join(", ")
                                ),
                            )
                            .with_hint("Initialize it with '&target'"));
                    }
                    self.check_default_constructible(&resolved_ty, stmt.span)?;
                }
                for name in names {
                    self.insert_var_in_current_scope(name.clone(), resolved_ty.clone(), stmt.span)?;
                }
                Ok(())
            }
            StmtKind::Assign { target, value } => self.check_assign_one(target, value),
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
                let mut val_types = Vec::with_capacity(values.len());
                for v in values {
                    let vt = self.check_expr_value(v, None)?;
                    if vt == Type::Void {
                        return Err(self.err(v.span, "Cannot assign void in multi-assignment"));
                    }
                    val_types.push(vt);
                }
                for ((t, vt), vexpr) in targets.iter().zip(val_types.into_iter()).zip(values.iter()) {
                    self.check_assign_target_with_type(t, &vt, vexpr)?;
                }
                Ok(())
            }
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let ct = self.check_expr_value(cond, None)?;
                if ct != Type::Bool {
                    return Err(self.err(
                        cond.span,
                        format!("If condition must be bool, got {}", ct.display_name()),
                    ));
                }
                self.check_block(then_branch)?;
                if let Some(eb) = else_branch {
                    self.check_block(eb)?;
                }
                Ok(())
            }
            StmtKind::For {
                var_name,
                var_ty,
                range_start,
                range_end,
                body,
            } => {
                let st = self.check_expr_value(range_start, None)?;
                let et = self.check_expr_value(range_end, None)?;
                if !matches!(st, Type::Int | Type::Uint) {
                    return Err(self.err(
                        range_start.span,
                        format!("For range start must be integer, got {}", st.display_name()),
                    ));
                }
                if !matches!(et, Type::Int | Type::Uint) {
                    return Err(self.err(
                        range_end.span,
                        format!("For range end must be integer, got {}", et.display_name()),
                    ));
                }
                let loop_var_ty = match var_ty {
                    Some(te) => {
                        let resolved = self.resolve_type_expr(te)?;
                        if !matches!(resolved, Type::Int | Type::Uint) {
                            return Err(self.err(
                                te.span,
                                format!(
                                    "For loop variable '{}' must be int or uint, got {}",
                                    var_name,
                                    resolved.display_name()
                                ),
                            ));
                        }
                        resolved
                    }
                    None => Type::Int,
                };
                self.scopes.push(HashMap::new());
                self.insert_var_in_current_scope(var_name.clone(), loop_var_ty, stmt.span)?;
                self.loop_depth += 1;
                let mut res = Ok(());
                for s in body {
                    if let Err(e) = self.check_stmt(s, false) {
                        res = Err(e);
                        break;
                    }
                }
                self.loop_depth -= 1;
                self.scopes.pop();
                res
            }
            StmtKind::While { cond, body } => {
                let ct = self.check_expr_value(cond, None)?;
                if ct != Type::Bool {
                    return Err(self.err(
                        cond.span,
                        format!("While condition must be bool, got {}", ct.display_name()),
                    ));
                }
                self.scopes.push(HashMap::new());
                self.loop_depth += 1;
                let mut res = Ok(());
                for s in body {
                    if let Err(e) = self.check_stmt(s, false) {
                        res = Err(e);
                        break;
                    }
                }
                self.loop_depth -= 1;
                self.scopes.pop();
                res
            }
            StmtKind::Break | StmtKind::Continue => {
                if self.loop_depth == 0 {
                    Err(self.err(stmt.span, "break/continue used outside of a loop"))
                } else {
                    Ok(())
                }
            }
            StmtKind::Return(expr_opt) => {
                if self.return_stack.is_empty() {
                    return Err(self.err(stmt.span, "'return' used outside of a function"));
                }
                let (expected_opt, _) = self.return_stack.last().unwrap().clone();
                let actual_ty = match expr_opt {
                    Some(e) => {
                        if matches!(e.node, ExprKind::Unary { op: UnaryOp::Ref, .. }) {
                            return Err(self
                                .err(e.span, "Cannot return a reference from a function")
                                .with_hint("Return a value instead to avoid dangling references"));
                        }
                        self.check_expr_value(e, expected_opt.as_ref())?
                    }
                    None => Type::Void,
                };
                if let Some(expected) = expected_opt {
                    if !self.is_compatible(&expected, &actual_ty, expr_opt.as_ref()) {
                        return Err(self.err(
                            stmt.span,
                            format!(
                                "Return type mismatch: expected {}, got {}",
                                expected.display_name(),
                                actual_ty.display_name()
                            ),
                        ));
                    }
                }
                if let Some((_, observed)) = self.return_stack.last_mut() {
                    observed.push(actual_ty);
                }
                Ok(())
            }
            StmtKind::ExprStmt(expr) => {
                let _ = self.check_expr_raw(expr, None)?;
                Ok(())
            }
            StmtKind::Print { args } => {
                for a in args {
                    let t = self.check_expr_value(a, None)?;
                    if t == Type::Void {
                        return Err(self.err(a.span, "Cannot print void expression"));
                    }
                }
                Ok(())
            }
            StmtKind::FnInput { targets } => {
                for target in targets {
                    match target {
                        InputTarget::Decl { names, ty, span } => {
                            let resolved = self.resolve_type_expr(ty)?;
                            if matches!(
                                resolved,
                                Type::Void | Type::Class { .. } | Type::Ref(_) | Type::Table(_)
                            ) {
                                return Err(self.err(
                                    *span,
                                    format!("Cannot read input() into type {}", resolved.display_name()),
                                ));
                            }
                            for n in names {
                                self.insert_var_in_current_scope(n.clone(), resolved.clone(), *span)?;
                            }
                        }
                        InputTarget::Var { target } => {
                            let t = self.check_expr_value(target, None)?;
                            if matches!(t, Type::Void | Type::Class { .. }) {
                                return Err(self.err(
                                    target.span,
                                    format!("Cannot read input() into type {}", t.display_name()),
                                ));
                            }
                        }
                    }
                }
                Ok(())
            }
        }
    }

    fn check_default_constructible(&self, ty: &Type, span: Span) -> Result<(), Diagnostic> {
        match ty {
            Type::Class {
                module,
                name,
                type_args,
            } => {
                let key = (name.clone(), type_args.clone());
                if let Some(cinfo) = self.env(module).classes.get(&key) {
                    if let Some(init_sig) = cinfo.methods.get("init") {
                        if init_sig.params.len() > 1 {
                            return Err(self.err(
                                span,
                                format!(
                                    "Class '{}' requires {} constructor arguments; initialize with '{}(...)'",
                                    ty.display_name(),
                                    init_sig.params.len() - 1,
                                    name
                                ),
                            ));
                        }
                    }
                }
                Ok(())
            }
            Type::Array(elem) => self.check_default_constructible(elem, span),
            _ => Ok(()),
        }
    }

    fn check_assign_one(&mut self, target: &AssignTarget, value: &Expr) -> Result<(), Diagnostic> {
        if let AssignTargetKind::Ident(name) = &target.node {
            if matches!(value.node, ExprKind::InputCall) {
                let existing = self.lookup_var(name).ok_or_else(|| {
                    self.err(
                        value.span,
                        format!(
                            "Cannot infer type for '{}' from input(); use '{}: <type> = input()'",
                            name, name
                        ),
                    )
                })?;
                if matches!(existing.deref(), Type::Table(_) | Type::Class { .. }) {
                    return Err(self.err(
                        value.span,
                        format!(
                            "Direct input() is not supported for {}",
                            existing.display_name()
                        ),
                    ));
                }
                return Ok(());
            }

            if let Some(existing_ty) = self.lookup_var(name) {
                // Assigning to an existing reference variable writes through to its referent!
                let target_val_ty = existing_ty.deref().clone();
                let val_ty = self.check_expr_value(value, Some(&target_val_ty))?;
                if val_ty == Type::Void {
                    return Err(self.err(value.span, format!("Cannot assign void value to '{}'", name)));
                }
                if !self.is_compatible(&target_val_ty, &val_ty, Some(value)) {
                    return Err(self.err(
                        value.span,
                        format!(
                            "Type mismatch assigning to '{}': expected {}, got {}",
                            name,
                            target_val_ty.display_name(),
                            val_ty.display_name()
                        ),
                    ));
                }
            } else {
                // Undeclared x = expr automatically declares x with inferred type
                let val_ty = if matches!(value.node, ExprKind::Unary { op: UnaryOp::Ref, .. }) {
                    self.check_expr_raw(value, None)?
                } else {
                    self.check_expr_value(value, None)?
                };
                if val_ty == Type::Void {
                    return Err(self.err(value.span, format!("Cannot assign void value to '{}'", name)));
                }
                self.insert_var_in_current_scope(name.clone(), val_ty, target.span)?;
            }
            return Ok(());
        }

        let elem_ty = self.resolve_assign_target_type(target)?;
        let val_ty = self.check_expr_value(value, Some(&elem_ty))?;
        if val_ty == Type::Void {
            return Err(self.err(value.span, "Cannot assign void value"));
        }
        if !self.is_compatible(&elem_ty, &val_ty, Some(value)) {
            return Err(self.err(
                value.span,
                format!(
                    "Type mismatch in assignment: expected {}, got {}",
                    elem_ty.display_name(),
                    val_ty.display_name()
                ),
            ));
        }
        Ok(())
    }

    fn check_assign_target_with_type(
        &mut self,
        target: &AssignTarget,
        val_ty: &Type,
        val_expr: &Expr,
    ) -> Result<(), Diagnostic> {
        if let AssignTargetKind::Ident(name) = &target.node {
            if let Some(existing) = self.lookup_var(name) {
                let target_val_ty = existing.deref();
                if !self.is_compatible(target_val_ty, val_ty, Some(val_expr)) {
                    return Err(self.err(
                        target.span,
                        format!(
                            "Type mismatch assigning to '{}': expected {}, got {}",
                            name,
                            target_val_ty.display_name(),
                            val_ty.display_name()
                        ),
                    ));
                }
            } else {
                self.insert_var_in_current_scope(name.clone(), val_ty.clone(), target.span)?;
            }
            return Ok(());
        }

        let expected = self.resolve_assign_target_type(target)?;
        if !self.is_compatible(&expected, val_ty, Some(val_expr)) {
            return Err(self.err(
                target.span,
                format!(
                    "Type mismatch in multi-assignment: expected {}, got {}",
                    expected.display_name(),
                    val_ty.display_name()
                ),
            ));
        }
        Ok(())
    }

    fn resolve_assign_target_type(&mut self, target: &AssignTarget) -> Result<Type, Diagnostic> {
        match &target.node {
            AssignTargetKind::Ident(name) => self
                .lookup_var(name)
                .map(|t| t.deref().clone())
                .ok_or_else(|| self.err(target.span, format!("Undefined variable '{}'", name))),
            AssignTargetKind::Field { target: obj, field } => {
                if let ExprKind::Ident(alias) = &obj.node {
                    if let Some(dep_mod_id) = self.current_env().imports.get(alias).cloned() {
                        if self.lookup_var(alias).is_none() {
                            return self
                                .env(&dep_mod_id)
                                .globals
                                .get(field)
                                .map(|t| t.deref().clone())
                                .ok_or_else(|| {
                                    self.err(
                                        target.span,
                                        format!("Module '{}' has no variable '{}'", alias, field),
                                    )
                                });
                        }
                    }
                }
                let obj_ty = self.check_expr_value(obj, None)?;
                self.lookup_class_field(&obj_ty, field, target.span)
            }
            AssignTargetKind::Index { target: arr, index } => {
                self.check_index_target(arr, index, target.span)
            }
            AssignTargetKind::Slice {
                target: arr,
                start,
                end,
            } => self.check_slice_assign_target(arr, start.as_ref(), end.as_ref(), target.span),
            AssignTargetKind::MultiIndex {
                target: arr,
                selectors,
            } => {
                let (result_ty, leaf_ty, has_range, leaf_is_string_char) =
                    self.check_multi_index(arr, selectors, target.span)?;
                if leaf_is_string_char {
                    return Err(self.err(
                        target.span,
                        "Cannot assign to an indexed character of a string",
                    ));
                }
                if has_range {
                    Ok(leaf_ty)
                } else {
                    Ok(result_ty)
                }
            }
        }
    }

    fn lookup_class_field(
        &self,
        obj_ty: &Type,
        field: &str,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let Type::Class {
            module,
            name,
            type_args,
        } = obj_ty.deref()
        else {
            return Err(self.err(
                span,
                format!(
                    "Cannot access field '{}' on non-class type {}",
                    field,
                    obj_ty.display_name()
                ),
            ));
        };
        let key = (name.clone(), type_args.clone());
        let info = self
            .env(module)
            .classes
            .get(&key)
            .ok_or_else(|| self.err(span, format!("Unknown class '{}'", obj_ty.display_name())))?;
        info.fields.get(field).cloned().ok_or_else(|| {
            self.err(
                span,
                format!("Class '{}' has no field '{}'", obj_ty.display_name(), field),
            )
        })
    }

    fn check_index_target(
        &mut self,
        target: &Expr,
        index: &Expr,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let it = self.check_expr_value(index, None)?;
        if !matches!(it, Type::Int | Type::Uint) {
            return Err(self.err(
                index.span,
                format!("Index must be integer, got {}", it.display_name()),
            ));
        }
        let tt = self.check_expr_value(target, None)?;
        match tt {
            Type::Array(elem) => Ok(*elem),
            Type::Table(rows) => {
                if let ExprKind::Int(r_idx) = index.node {
                    if r_idx < 0 || (r_idx as usize) >= rows.len() {
                        return Err(self.err(index.span, format!("Row index {} out of bounds", r_idx)));
                    }
                    Ok(Type::Array(Box::new(rows[r_idx as usize].clone())))
                } else if let Some(first) = rows.first() {
                    if rows.iter().all(|r| r == first) {
                        Ok(Type::Array(Box::new(first.clone())))
                    } else {
                        Err(self.err(
                            span,
                            "Dynamic row index on heterogeneous table is not supported during static type checking",
                        ))
                    }
                } else {
                    Err(self.err(span, "Empty table"))
                }
            }
            Type::String => Ok(Type::Char),
            other => Err(self.err(span, format!("Cannot index type {}", other.display_name()))),
        }
    }

    fn check_slice_assign_target(
        &mut self,
        target: &Expr,
        start: Option<&Expr>,
        end: Option<&Expr>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        if let Some(s) = start {
            let st = self.check_expr_value(s, None)?;
            if !matches!(st, Type::Int | Type::Uint) {
                return Err(self.err(
                    s.span,
                    format!("Slice start must be integer, got {}", st.display_name()),
                ));
            }
        }
        if let Some(e) = end {
            let et = self.check_expr_value(e, None)?;
            if !matches!(et, Type::Int | Type::Uint) {
                return Err(self.err(
                    e.span,
                    format!("Slice end must be integer, got {}", et.display_name()),
                ));
            }
        }
        let tt = self.check_expr_value(target, None)?;
        match tt {
            Type::Array(elem) => Ok(*elem),
            other => Err(self.err(
                span,
                format!(
                    "Slice fill assignment target must be an array, got {}",
                    other.display_name()
                ),
            )),
        }
    }

    fn check_multi_index(
        &mut self,
        target: &Expr,
        selectors: &[SliceSelector],
        span: Span,
    ) -> Result<(Type, Type, bool, bool), Diagnostic> {
        let mut cur_ty = self.check_expr_value(target, None)?;
        let mut kept_dims: usize = 0;
        let mut has_range = false;
        let mut leaf_is_string_char = false;

        for (i, sel) in selectors.iter().enumerate() {
            match sel {
                SliceSelector::Index(idx_expr) => {
                    let it = self.check_expr_value(idx_expr, None)?;
                    if !matches!(it, Type::Int | Type::Uint) {
                        return Err(self.err(
                            idx_expr.span,
                            format!("Index must be integer, got {}", it.display_name()),
                        ));
                    }
                    match cur_ty {
                        Type::Array(elem) => {
                            cur_ty = *elem;
                        }
                        Type::Table(rows) => {
                            if let ExprKind::Int(r_idx) = idx_expr.node {
                                if r_idx < 0 || (r_idx as usize) >= rows.len() {
                                    return Err(self.err(
                                        idx_expr.span,
                                        format!("Row index {} out of bounds", r_idx),
                                    ));
                                }
                                cur_ty = Type::Array(Box::new(rows[r_idx as usize].clone()));
                            } else if let Some(first) = rows.first() {
                                if rows.iter().all(|r| r == first) {
                                    cur_ty = Type::Array(Box::new(first.clone()));
                                } else {
                                    return Err(self.err(
                                        span,
                                        "Dynamic row index on heterogeneous table is not supported during static type checking",
                                    ));
                                }
                            } else {
                                return Err(self.err(span, "Empty table"));
                            }
                        }
                        Type::String => {
                            if i + 1 < selectors.len() {
                                return Err(self.err(
                                    idx_expr.span,
                                    "Cannot index further into a character",
                                ));
                            }
                            if kept_dims > 0 {
                                return Err(self.err(
                                    span,
                                    "Cannot project a slice view into characters of a string",
                                ));
                            }
                            cur_ty = Type::Char;
                            leaf_is_string_char = true;
                        }
                        other => {
                            return Err(self.err(
                                idx_expr.span,
                                format!("Cannot index type {}", other.display_name()),
                            ));
                        }
                    }
                }
                SliceSelector::Range { start, end } => {
                    has_range = true;
                    if let Some(s) = start {
                        let st = self.check_expr_value(s, None)?;
                        if !matches!(st, Type::Int | Type::Uint) {
                            return Err(self.err(
                                s.span,
                                format!("Slice start must be integer, got {}", st.display_name()),
                            ));
                        }
                    }
                    if let Some(e) = end {
                        let et = self.check_expr_value(e, None)?;
                        if !matches!(et, Type::Int | Type::Uint) {
                            return Err(self.err(
                                e.span,
                                format!("Slice end must be integer, got {}", et.display_name()),
                            ));
                        }
                    }
                    match cur_ty {
                        Type::Array(elem) => {
                            cur_ty = *elem;
                            kept_dims += 1;
                        }
                        Type::Table(_) => {
                            return Err(self.err(
                                span,
                                "Cannot slice across rows of a table directly; select a row first like tbl[row, start..end]",
                            ));
                        }
                        Type::String => {
                            return Err(self.err(
                                span,
                                "String slicing cannot be combined with multi-axis indexing; use S[start..end]",
                            ));
                        }
                        other => {
                            return Err(self.err(
                                span,
                                format!("Cannot slice type {}", other.display_name()),
                            ));
                        }
                    }
                }
            }
        }

        let leaf_ty = cur_ty.clone();
        let mut result_ty = cur_ty;
        for _ in 0..kept_dims {
            result_ty = Type::Array(Box::new(result_ty));
        }
        Ok((result_ty, leaf_ty, has_range, leaf_is_string_char))
    }

    fn check_expr_value(
        &mut self,
        expr: &Expr,
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        let raw = self.check_expr_raw(expr, expected)?;
        Ok(raw.deref().clone())
    }

    fn check_expr_raw(
        &mut self,
        expr: &Expr,
        expected: Option<&Type>,
    ) -> Result<Type, Diagnostic> {
        match &expr.node {
            ExprKind::Int(_) => Ok(Type::Int),
            ExprKind::Float(_) => Ok(Type::Float),
            ExprKind::Char(_) => Ok(Type::Char),
            ExprKind::Str(_) => Ok(Type::String),
            ExprKind::Bool(_) => Ok(Type::Bool),
            ExprKind::ArrayLiteral(elems) => {
                let expected_elem = expected.and_then(|t| match t.deref() {
                    Type::Array(elem) => Some(elem.as_ref()),
                    _ => None,
                });
                if elems.is_empty() {
                    if let Some(et) = expected_elem {
                        return Ok(Type::Array(Box::new(et.clone())));
                    }
                    return Err(self
                        .err(expr.span, "Cannot infer element type of empty array literal '[]'")
                        .with_hint("Provide a type annotation, e.g. 'items: int[] = []'"));
                }
                let first_ty = self.check_expr_value(&elems[0], expected_elem)?;
                if first_ty == Type::Void {
                    return Err(self.err(elems[0].span, "Array element cannot have type void"));
                }
                let unified_elem = if let Some(exp) = expected_elem {
                    if self.is_compatible(exp, &first_ty, Some(&elems[0])) {
                        exp.clone()
                    } else {
                        first_ty
                    }
                } else {
                    first_ty
                };
                for item in &elems[1..] {
                    let it = self.check_expr_value(item, Some(&unified_elem))?;
                    if !self.is_compatible(&unified_elem, &it, Some(item)) {
                        return Err(self.err(
                            item.span,
                            format!(
                                "Array element type mismatch: expected {}, got {}",
                                unified_elem.display_name(),
                                it.display_name()
                            ),
                        ));
                    }
                }
                Ok(Type::Array(Box::new(unified_elem)))
            }
            ExprKind::SelfRef => self
                .lookup_var("self")
                .ok_or_else(|| self.err(expr.span, "'self' used outside of a class method")),
            ExprKind::Ident(name) => self.lookup_var(name).ok_or_else(|| {
                if self.current_env().imports.contains_key(name) {
                    self.err(
                        expr.span,
                        format!("Module '{}' cannot be used as a value", name),
                    )
                    .with_hint(format!("Access a member with '{}.<symbol>'", name))
                } else {
                    self.err(expr.span, format!("Undefined variable '{}'", name))
                }
            }),
            ExprKind::InputCall => Err(self
                .err(
                    expr.span,
                    "input() without arguments requires a typed declaration like 'N: int = input()'",
                )),
            ExprKind::Unary { op, expr: inner } => match op {
                UnaryOp::Neg => {
                    let t = self.check_expr_value(inner, expected)?;
                    if matches!(t, Type::Int | Type::Float) {
                        Ok(t)
                    } else {
                        Err(self.err(
                            expr.span,
                            format!("Cannot negate value of type {}", t.display_name()),
                        ))
                    }
                }
                UnaryOp::Not => {
                    let t = self.check_expr_value(inner, Some(&Type::Bool))?;
                    if t == Type::Bool {
                        Ok(Type::Bool)
                    } else {
                        Err(self.err(
                            expr.span,
                            format!("Logical '!' requires bool, got {}", t.display_name()),
                        ))
                    }
                }
                UnaryOp::Ref => self.check_ref_expr(inner, expr.span),
            },
            ExprKind::Binary { op, left, right } => {
                let lt = self.check_expr_value(left, None)?;
                let rt = self.check_expr_value(right, None)?;
                check_binary_type(*op, &lt, &rt, left, right, expr.span)
                    .map_err(|msg| self.err(expr.span, msg))
            }
            ExprKind::Index { target, index } => {
                self.check_index_target(target, index, expr.span)
            }
            ExprKind::SliceView { target, start, end } => {
                if let Some(s) = start {
                    let st = self.check_expr_value(s, None)?;
                    if !matches!(st, Type::Int | Type::Uint) {
                        return Err(self.err(
                            s.span,
                            format!("Slice start must be integer, got {}", st.display_name()),
                        ));
                    }
                }
                if let Some(e) = end {
                    let et = self.check_expr_value(e, None)?;
                    if !matches!(et, Type::Int | Type::Uint) {
                        return Err(self.err(
                            e.span,
                            format!("Slice end must be integer, got {}", et.display_name()),
                        ));
                    }
                }
                let tt = self.check_expr_value(target, None)?;
                match tt {
                    Type::Array(elem) => Ok(Type::Array(elem)),
                    Type::String => Ok(Type::String),
                    other => Err(self.err(
                        expr.span,
                        format!(
                            "SliceView target must be an array or string, got {}",
                            other.display_name()
                        ),
                    )),
                }
            }
            ExprKind::MultiIndex { target, selectors } => {
                let (result_ty, _, _, _) = self.check_multi_index(target, selectors, expr.span)?;
                Ok(result_ty)
            }
            ExprKind::FieldAccess { target, field } => {
                if let ExprKind::Ident(alias) = &target.node {
                    if let Some(dep_mod_id) = self.current_env().imports.get(alias).cloned() {
                        if self.lookup_var(alias).is_none() {
                            return self
                                .env(&dep_mod_id)
                                .globals
                                .get(field)
                                .cloned()
                                .ok_or_else(|| {
                                    self.err(
                                        expr.span,
                                        format!(
                                            "Module '{}' has no exported variable '{}'",
                                            alias, field
                                        ),
                                    )
                                });
                        }
                    }
                }
                let obj_ty = self.check_expr_value(target, None)?;
                self.lookup_class_field(&obj_ty, field, expr.span)
            }
            ExprKind::Call {
                callee,
                type_args,
                args,
            } => {
                if callee == "len" {
                    if !type_args.is_empty() {
                        return Err(self.err(expr.span, "len() does not accept type arguments"));
                    }
                    if args.len() != 1 {
                        return Err(self.err(
                            expr.span,
                            format!("len() expects 1 argument, got {}", args.len()),
                        ));
                    }
                    let at = self.check_expr_value(&args[0], None)?;
                    if matches!(at, Type::Array(_) | Type::String | Type::Table(_)) {
                        return Ok(Type::Int);
                    }
                    return Err(self.err(
                        args[0].span,
                        format!("len() expects array or string, got {}", at.display_name()),
                    ));
                }
                let mid = self.current_module.clone();
                self.check_call_in_module(&mid, callee, type_args, args, expr.span)
            }
            ExprKind::MethodCall {
                target,
                method,
                type_args,
                args,
            } => {
                if let ExprKind::Ident(alias) = &target.node {
                    if let Some(dep_mod_id) = self.current_env().imports.get(alias).cloned() {
                        if self.lookup_var(alias).is_none() {
                            return self.check_call_in_module(
                                &dep_mod_id,
                                method,
                                type_args,
                                args,
                                expr.span,
                            );
                        }
                    }
                }

                let obj_ty = self.check_expr_value(target, None)?;
                match obj_ty {
                    Type::Array(elem) => {
                        if !type_args.is_empty() {
                            return Err(self.err(
                                expr.span,
                                format!("Array method '{}' does not take type arguments", method),
                            ));
                        }
                        match method.as_str() {
                            "len" => {
                                if !args.is_empty() {
                                    return Err(self.err(expr.span, "len() takes 0 arguments"));
                                }
                                Ok(Type::Int)
                            }
                            "push" => {
                                if args.len() != 1 {
                                    return Err(self.err(
                                        expr.span,
                                        format!("push() expects 1 argument, got {}", args.len()),
                                    ));
                                }
                                let at = self.check_expr_value(&args[0], Some(&elem))?;
                                if !self.is_compatible(&elem, &at, Some(&args[0])) {
                                    return Err(self.err(
                                        args[0].span,
                                        format!(
                                            "push() expected {}, got {}",
                                            elem.display_name(),
                                            at.display_name()
                                        ),
                                    ));
                                }
                                Ok(Type::Void)
                            }
                            "pop" => {
                                if !args.is_empty() {
                                    return Err(self.err(expr.span, "pop() takes 0 arguments"));
                                }
                                Ok(*elem)
                            }
                            "clear" => {
                                if !args.is_empty() {
                                    return Err(self.err(expr.span, "clear() takes 0 arguments"));
                                }
                                Ok(Type::Void)
                            }
                            other => Err(self.err(
                                expr.span,
                                format!("Array has no method '{}'", other),
                            )),
                        }
                    }
                    Type::String => {
                        if !type_args.is_empty() {
                            return Err(self.err(expr.span, "String methods do not take type arguments"));
                        }
                        match method.as_str() {
                            "len" => {
                                if !args.is_empty() {
                                    return Err(self.err(expr.span, "len() takes 0 arguments"));
                                }
                                Ok(Type::Int)
                            }
                            other => Err(self.err(
                                expr.span,
                                format!("string has no method '{}'", other),
                            )),
                        }
                    }
                    Type::Table(_) => {
                        if !type_args.is_empty() {
                            return Err(self.err(expr.span, "Table methods do not take type arguments"));
                        }
                        match method.as_str() {
                            "len" => {
                                if !args.is_empty() {
                                    return Err(self.err(expr.span, "len() takes 0 arguments"));
                                }
                                Ok(Type::Int)
                            }
                            other => Err(self.err(
                                expr.span,
                                format!("Table has no method '{}'", other),
                            )),
                        }
                    }
                    Type::Class {
                        module,
                        name,
                        type_args: cls_targs,
                    } => self.check_class_method_call(
                        &module,
                        &name,
                        &cls_targs,
                        method,
                        type_args,
                        args,
                        expr.span,
                    ),
                    other => Err(self.err(
                        expr.span,
                        format!(
                            "Cannot call method '{}' on type {}",
                            method,
                            other.display_name()
                        ),
                    )),
                }
            }
        }
    }

    fn check_ref_expr(&mut self, inner: &Expr, span: Span) -> Result<Type, Diagnostic> {
        match &inner.node {
            ExprKind::Ident(name) => {
                let ty = self.lookup_var(name).ok_or_else(|| {
                    self.err(inner.span, format!("Undefined variable '{}'", name))
                })?;
                if matches!(ty, Type::Ref(_)) {
                    return Err(self
                        .err(
                            span,
                            format!(
                                "Variable '{}' is already a reference ({})",
                                name,
                                ty.display_name()
                            ),
                        )
                        .with_hint(format!("Pass '{}' directly instead of '&{}'", name, name)));
                }
                Ok(Type::Ref(Box::new(ty)))
            }
            ExprKind::FieldAccess { target, field } => {
                if let ExprKind::Ident(alias) = &target.node {
                    if self.current_env().imports.contains_key(alias)
                        && self.lookup_var(alias).is_none()
                    {
                        return Err(self.err(
                            span,
                            "Cannot take mutable reference '&' to a module variable directly",
                        ));
                    }
                }
                let obj_ty = self.check_expr_value(target, None)?;
                let field_ty = self.lookup_class_field(&obj_ty, field, inner.span)?;
                if matches!(field_ty, Type::Ref(_)) {
                    return Err(self.err(span, "Cannot take reference to a reference"));
                }
                Ok(Type::Ref(Box::new(field_ty)))
            }
            ExprKind::Index { target, index } => {
                let it = self.check_expr_value(index, None)?;
                if !matches!(it, Type::Int | Type::Uint) {
                    return Err(self.err(
                        index.span,
                        format!("Index must be integer, got {}", it.display_name()),
                    ));
                }
                let tt = self.check_expr_value(target, None)?;
                match tt {
                    Type::Array(elem) => Ok(Type::Ref(elem)),
                    Type::String => Err(self.err(
                        span,
                        "Cannot take mutable reference '&' to a character inside a string",
                    )),
                    other => Err(self.err(
                        span,
                        format!("Cannot take reference to indexed {}", other.display_name()),
                    )),
                }
            }
            ExprKind::MultiIndex { target, selectors } => {
                let (result_ty, _, has_range, leaf_is_string_char) =
                    self.check_multi_index(target, selectors, span)?;
                if has_range {
                    return Err(self
                        .err(span, "Cannot take mutable reference '&' to a slice range")
                        .with_hint("Index a concrete element like '&A[i, j]' instead"));
                }
                if leaf_is_string_char {
                    return Err(self.err(
                        span,
                        "Cannot take mutable reference '&' to a character inside a string",
                    ));
                }
                Ok(Type::Ref(Box::new(result_ty)))
            }
            _ => Err(self
                .err(span, "Cannot take reference '&' of a temporary expression (rvalue)")
                .with_hint("Only variables (&x), array elements (&A[i]), and fields (&obj.field) can be referenced")),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_class_method_call(
        &mut self,
        mod_id: &str,
        class_name: &str,
        cls_targs: &[Type],
        method: &str,
        call_type_args: &[TypeExpr],
        args: &[Expr],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        self.ensure_class_instantiated(mod_id, class_name, cls_targs, span)?;
        let key = (class_name.to_string(), cls_targs.to_vec());
        let cinfo = self.env(mod_id).classes.get(&key).cloned().unwrap();

        let method_decl = cinfo
            .method_decls
            .get(method)
            .cloned()
            .ok_or_else(|| self.err(span, format!("Class '{}' has no method '{}'", class_name, method)))?;

        if method_decl.type_params.is_empty() {
            if !call_type_args.is_empty() {
                return Err(self.err(
                    span,
                    format!("Method '{}.{}' is not generic", class_name, method),
                ));
            }
            let sig = cinfo.methods.get(method).cloned().unwrap();
            let expected_params = &sig.params[1..];
            self.check_call_args(&format!("{}.{}", class_name, method), expected_params, args, span)?;
            Ok(sig.ret.unwrap_or(Type::Void))
        } else {
            let resolved_method_targs = if !call_type_args.is_empty() {
                if call_type_args.len() != method_decl.type_params.len() {
                    return Err(self.err(
                        span,
                        format!(
                            "Method '{}.{}' expects {} type argument(s), got {}",
                            class_name,
                            method,
                            method_decl.type_params.len(),
                            call_type_args.len()
                        ),
                    ));
                }
                let mut res = Vec::with_capacity(call_type_args.len());
                for ta in call_type_args {
                    res.push(self.resolve_type_expr(ta)?);
                }
                res
            } else {
                let mut param_tys = Vec::new();
                for p in method_decl.params.iter().skip(1) {
                    param_tys.push(p.ty.as_ref().ok_or_else(|| {
                        self.err(p.span, format!("Parameter '{}' requires a type", p.name))
                    })?);
                }
                self.infer_type_params_from_args(
                    &format!("{}.{}", class_name, method),
                    &method_decl.type_params,
                    &param_tys,
                    args,
                    span,
                )?
            };

            let inst_key = (
                class_name.to_string(),
                cls_targs.to_vec(),
                method.to_string(),
                resolved_method_targs.clone(),
            );
            let sig = if let Some(s) = self.env(mod_id).instantiated_methods.get(&inst_key) {
                s.clone()
            } else {
                let cd = self.env(mod_id).class_decls.get(class_name).cloned().unwrap();
                let mut bindings = HashMap::new();
                for (tp, ta) in cd.type_params.iter().zip(cls_targs.iter()) {
                    bindings.insert(tp.clone(), ta.clone());
                }
                for (tp, ta) in method_decl.type_params.iter().zip(resolved_method_targs.iter()) {
                    bindings.insert(tp.clone(), ta.clone());
                }
                let self_ty = Type::Class {
                    module: mod_id.to_string(),
                    name: class_name.to_string(),
                    type_args: cls_targs.to_vec(),
                };
                let mut sig = self.build_method_sig(mod_id, &method_decl, &self_ty, &bindings)?;
                if !self.env(mod_id).checking_inst_methods.contains(&inst_key) {
                    let params: Vec<(String, Type)> = method_decl
                        .params
                        .iter()
                        .map(|p| p.name.clone())
                        .zip(sig.params.clone())
                        .collect();
                    let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
                    let prev_bindings =
                        std::mem::replace(&mut self.type_param_bindings, bindings);
                    self.env_mut(mod_id)
                        .checking_inst_methods
                        .insert(inst_key.clone());
                    let ret_res = self.check_fn_body(
                        &format!("{}.{}", class_name, method),
                        &params,
                        sig.ret.clone(),
                        &method_decl.body,
                        method_decl.span,
                    );
                    self.env_mut(mod_id)
                        .checking_inst_methods
                        .remove(&inst_key);
                    self.type_param_bindings = prev_bindings;
                    self.current_module = prev_mod;
                    sig.ret = Some(ret_res?);
                    self.env_mut(mod_id)
                        .instantiated_methods
                        .insert(inst_key, sig.clone());
                }
                sig
            };

            let expected_params = &sig.params[1..];
            self.check_call_args(&format!("{}.{}", class_name, method), expected_params, args, span)?;
            Ok(sig.ret.unwrap_or(Type::Void))
        }
    }

    fn build_method_sig(
        &mut self,
        mod_id: &str,
        m: &FnDecl,
        self_ty: &Type,
        bindings: &HashMap<String, Type>,
    ) -> Result<FnSig, Diagnostic> {
        let prev_mod = std::mem::replace(&mut self.current_module, mod_id.to_string());
        let prev_bindings = std::mem::replace(&mut self.type_param_bindings, bindings.clone());
        let res = (|| {
            let mut param_types = Vec::with_capacity(m.params.len());
            for (idx, p) in m.params.iter().enumerate() {
                if idx == 0 {
                    param_types.push(self_ty.clone());
                } else {
                    let ts = p.ty.as_ref().ok_or_else(|| {
                        self.err(p.span, format!("Parameter '{}' requires a type", p.name))
                    })?;
                    param_types.push(self.resolve_type_expr(ts)?);
                }
            }
            let ret = match &m.return_type {
                Some(ts) => {
                    let rt = self.resolve_type_expr(ts)?;
                    if matches!(rt, Type::Ref(_)) {
                        return Err(self.err(
                            ts.span,
                            "Methods cannot return a reference type '&T'",
                        ));
                    }
                    Some(rt)
                }
                None => None,
            };
            Ok(FnSig {
                params: param_types,
                ret,
            })
        })();
        self.type_param_bindings = prev_bindings;
        self.current_module = prev_mod;
        res
    }

    fn check_call_in_module(
        &mut self,
        target_mod_id: &str,
        callee: &str,
        type_args: &[TypeExpr],
        args: &[Expr],
        span: Span,
    ) -> Result<Type, Diagnostic> {
        if let Some(cd) = self.env(target_mod_id).class_decls.get(callee).cloned() {
            let resolved_targs = if cd.type_params.is_empty() {
                if !type_args.is_empty() {
                    return Err(self.err(
                        span,
                        format!("Class '{}' is not generic", callee),
                    ));
                }
                Vec::new()
            } else if !type_args.is_empty() {
                if type_args.len() != cd.type_params.len() {
                    return Err(self.err(
                        span,
                        format!(
                            "Generic class '{}' expects {} type argument(s), got {}",
                            callee,
                            cd.type_params.len(),
                            type_args.len()
                        ),
                    ));
                }
                let mut res = Vec::with_capacity(type_args.len());
                for ta in type_args {
                    let t = self.resolve_type_expr(ta)?;
                    if t == Type::Void {
                        return Err(self.err(ta.span, "Generic type argument cannot be void"));
                    }
                    res.push(t);
                }
                res
            } else {
                let init_fn = cd.methods.iter().find(|m| m.name == "init").ok_or_else(|| {
                    self.err(
                        span,
                        format!(
                            "Cannot infer type arguments for generic class '{}' without an 'init' constructor",
                            callee
                        ),
                    )
                    .with_hint(format!("Specify type arguments explicitly: {}<...>(...)", callee))
                })?;
                let mut param_tys = Vec::new();
                for p in init_fn.params.iter().skip(1) {
                    param_tys.push(p.ty.as_ref().ok_or_else(|| {
                        self.err(p.span, format!("Parameter '{}' requires a type", p.name))
                    })?);
                }
                self.infer_type_params_from_args(callee, &cd.type_params, &param_tys, args, span)?
            };

            self.ensure_class_instantiated(target_mod_id, callee, &resolved_targs, span)?;
            let key = (callee.to_string(), resolved_targs.clone());
            let cinfo = self.env(target_mod_id).classes.get(&key).cloned().unwrap();

            if let Some(init_sig) = cinfo.methods.get("init") {
                let expected_params = &init_sig.params[1..];
                self.check_call_args(&format!("{}.init", callee), expected_params, args, span)?;
            } else if !args.is_empty() {
                return Err(self.err(
                    span,
                    format!(
                        "Class '{}' has no 'init' constructor taking {} argument(s)",
                        callee,
                        args.len()
                    ),
                ));
            }
            return Ok(Type::Class {
                module: target_mod_id.to_string(),
                name: callee.to_string(),
                type_args: resolved_targs,
            });
        }

        if let Some(fd) = self.env(target_mod_id).fn_decls.get(callee).cloned() {
            if fd.type_params.is_empty() {
                if !type_args.is_empty() {
                    return Err(self.err(
                        span,
                        format!("Function '{}' is not generic", callee),
                    ));
                }
                let need_infer = self
                    .env(target_mod_id)
                    .functions
                    .get(callee)
                    .is_some_and(|s| s.ret.is_none());
                if need_infer {
                    if self.env(target_mod_id).checking_fns.contains(callee) {
                        return Err(self.err(
                            span,
                            format!(
                                "Cannot infer return type for recursive function '{}'; please specify '-> <type>'",
                                callee
                            ),
                        ));
                    }
                    self.ensure_fn_checked(target_mod_id, callee, span)?;
                }
                let sig = self.env(target_mod_id).functions.get(callee).cloned().unwrap();
                self.check_call_args(callee, &sig.params, args, span)?;
                return Ok(sig.ret.unwrap_or(Type::Void));
            } else {
                let resolved_targs = if !type_args.is_empty() {
                    if type_args.len() != fd.type_params.len() {
                        return Err(self.err(
                            span,
                            format!(
                                "Generic function '{}' expects {} type argument(s), got {}",
                                callee,
                                fd.type_params.len(),
                                type_args.len()
                            ),
                        ));
                    }
                    let mut res = Vec::with_capacity(type_args.len());
                    for ta in type_args {
                        let t = self.resolve_type_expr(ta)?;
                        if t == Type::Void {
                            return Err(self.err(ta.span, "Generic type argument cannot be void"));
                        }
                        res.push(t);
                    }
                    res
                } else {
                    let mut param_tys = Vec::new();
                    for p in &fd.params {
                        param_tys.push(p.ty.as_ref().ok_or_else(|| {
                            self.err(p.span, format!("Parameter '{}' requires a type", p.name))
                        })?);
                    }
                    self.infer_type_params_from_args(callee, &fd.type_params, &param_tys, args, span)?
                };

                let sig = self.ensure_generic_fn_instantiated(
                    target_mod_id,
                    callee,
                    &resolved_targs,
                    span,
                )?;
                self.check_call_args(callee, &sig.params, args, span)?;
                return Ok(sig.ret.unwrap_or(Type::Void));
            }
        }

        Err(self.err(span, format!("Undefined function or class '{}'", callee)))
    }

    fn infer_type_params_from_args(
        &mut self,
        callee: &str,
        type_params: &[String],
        param_te_list: &[&TypeExpr],
        args: &[Expr],
        span: Span,
    ) -> Result<Vec<Type>, Diagnostic> {
        if param_te_list.len() != args.len() {
            return Err(self.err(
                span,
                format!(
                    "'{}' expects {} argument(s), got {}",
                    callee,
                    param_te_list.len(),
                    args.len()
                ),
            ));
        }

        let mut inferred: HashMap<String, Type> = HashMap::new();
        for (pte, arg_expr) in param_te_list.iter().zip(args.iter()) {
            let act_ty = match &pte.node {
                TypeExprKind::Ref(_) => self.check_expr_raw(arg_expr, None)?,
                _ => self.check_expr_value(arg_expr, None)?,
            };
            self.unify_type_expr(pte, &act_ty, type_params, &mut inferred, arg_expr.span, callee)?;
        }

        let mut result = Vec::with_capacity(type_params.len());
        for tp in type_params {
            let bound = inferred.get(tp).cloned().ok_or_else(|| {
                self.err(
                    span,
                    format!(
                        "Cannot infer generic type parameter '{}' in call to '{}'",
                        tp, callee
                    ),
                )
                .with_hint(format!("Specify type arguments explicitly: {}<...>(...)", callee))
            })?;
            result.push(bound);
        }
        Ok(result)
    }

    fn unify_type_expr(
        &self,
        pattern: &TypeExpr,
        actual: &Type,
        type_params: &[String],
        inferred: &mut HashMap<String, Type>,
        span: Span,
        callee: &str,
    ) -> Result<(), Diagnostic> {
        match &pattern.node {
            TypeExprKind::Named {
                module: None,
                name,
                type_args,
            } if type_args.is_empty() && type_params.contains(name) => {
                if let Some(prev) = inferred.get(name) {
                    if prev != actual {
                        return Err(self.err(
                            span,
                            format!(
                                "Conflicting inferred types for '{}' in call to '{}': {} vs {}",
                                name,
                                callee,
                                prev.display_name(),
                                actual.display_name()
                            ),
                        ));
                    }
                } else {
                    inferred.insert(name.clone(), actual.deref().clone());
                }
                Ok(())
            }
            TypeExprKind::Ref(inner_pat) => {
                let Type::Ref(inner_act) = actual else {
                    return Err(self
                        .err(
                            span,
                            format!(
                                "Expected reference argument '&...' in call to '{}', got {}",
                                callee,
                                actual.display_name()
                            ),
                        )
                        .with_hint("Pass a reference using '&expr'"));
                };
                self.unify_type_expr(inner_pat, inner_act, type_params, inferred, span, callee)
            }
            TypeExprKind::Array { elem: elem_pat, .. } => {
                let Type::Array(elem_act) = actual.deref() else {
                    return Err(self.err(
                        span,
                        format!(
                            "Expected array argument in call to '{}', got {}",
                            callee,
                            actual.display_name()
                        ),
                    ));
                };
                self.unify_type_expr(elem_pat, elem_act, type_params, inferred, span, callee)
            }
            TypeExprKind::Named {
                name: pat_name,
                type_args: pat_args,
                ..
            } if !pat_args.is_empty() => {
                let Type::Class {
                    name: act_name,
                    type_args: act_args,
                    ..
                } = actual.deref()
                else {
                    return Ok(());
                };
                if pat_name == act_name && pat_args.len() == act_args.len() {
                    for (pa, aa) in pat_args.iter().zip(act_args.iter()) {
                        self.unify_type_expr(pa, aa, type_params, inferred, span, callee)?;
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn check_call_args(
        &mut self,
        callee_label: &str,
        expected_params: &[Type],
        args: &[Expr],
        call_span: Span,
    ) -> Result<(), Diagnostic> {
        if args.len() != expected_params.len() {
            return Err(self.err(
                call_span,
                format!(
                    "'{}' expects {} argument(s), got {}",
                    callee_label,
                    expected_params.len(),
                    args.len()
                ),
            ));
        }
        for (i, (arg_expr, exp_ty)) in args.iter().zip(expected_params.iter()).enumerate() {
            if let Type::Ref(exp_inner) = exp_ty {
                let act_ty = self.check_expr_raw(arg_expr, Some(exp_ty))?;
                let Type::Ref(act_inner) = &act_ty else {
                    return Err(self
                        .err(
                            arg_expr.span,
                            format!(
                                "Argument {} of '{}' expects {}, got {}",
                                i + 1,
                                callee_label,
                                exp_ty.display_name(),
                                act_ty.display_name()
                            ),
                        )
                        .with_hint("Pass a mutable reference using '&variable' or '&A[i]'"));
                };
                if exp_inner != act_inner {
                    return Err(self.err(
                        arg_expr.span,
                        format!(
                            "Argument {} of '{}' expects {}, got {}",
                            i + 1,
                            callee_label,
                            exp_ty.display_name(),
                            act_ty.display_name()
                        ),
                    ));
                }
            } else {
                let act_ty = self.check_expr_value(arg_expr, Some(exp_ty))?;
                if !self.is_compatible(exp_ty, &act_ty, Some(arg_expr)) {
                    return Err(self.err(
                        arg_expr.span,
                        format!(
                            "Argument {} of '{}' expects {}, got {}",
                            i + 1,
                            callee_label,
                            exp_ty.display_name(),
                            act_ty.display_name()
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

fn is_builtin_type_name(name: &str) -> bool {
    matches!(
        name,
        "int" | "uint" | "float" | "char" | "string" | "bool" | "void"
    )
}

fn check_binary_type(
    op: BinaryOp,
    l: &Type,
    r: &Type,
    left_expr: &Expr,
    right_expr: &Expr,
    _span: Span,
) -> Result<Type, String> {
    let same_or_uint_lit = l == r
        || (*l == Type::Uint && matches!(right_expr.node, ExprKind::Int(n) if n >= 0))
        || (*r == Type::Uint && matches!(left_expr.node, ExprKind::Int(n) if n >= 0));

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
        match &s.node {
            StmtKind::Return(_) => return true,
            StmtKind::If {
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
