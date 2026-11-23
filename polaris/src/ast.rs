use crate::diagnostic::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    pub node: TypeExprKind,
    pub span: Span,
}

impl TypeExpr {
    pub fn new(node: TypeExprKind, span: Span) -> Self {
        Self { node, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExprKind {
    Named {
        module: Option<String>,
        name: String,
        type_args: Vec<TypeExpr>,
    },
    Ref(Box<TypeExpr>),
    Array {
        elem: Box<TypeExpr>,
        len: Option<Box<Expr>>,
    },
    Table {
        row_types: Vec<TypeExpr>,
        len: Option<Box<Expr>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub node: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(node: ExprKind, span: Span) -> Self {
        Self { node, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Char(char),
    Str(String),
    Bool(bool),
    ArrayLiteral(Vec<Expr>),
    Ident(String),
    SelfRef,
    Call {
        callee: String,
        type_args: Vec<TypeExpr>,
        args: Vec<Expr>,
    },
    FieldAccess {
        target: Box<Expr>,
        field: String,
    },
    MethodCall {
        target: Box<Expr>,
        method: String,
        type_args: Vec<TypeExpr>,
        args: Vec<Expr>,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    SliceView {
        target: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    MultiIndex {
        target: Box<Expr>,
        selectors: Vec<SliceSelector>,
    },
    InputCall,
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

#[derive(Debug, Clone, PartialEq)]
pub enum SliceSelector {
    Index(Expr),
    Range {
        start: Option<Expr>,
        end: Option<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    Ref,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
pub enum InputTarget {
    Decl {
        names: Vec<String>,
        ty: TypeExpr,
        span: Span,
    },
    Var {
        target: Expr,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssignTarget {
    pub node: AssignTargetKind,
    pub span: Span,
}

impl AssignTarget {
    pub fn new(node: AssignTargetKind, span: Span) -> Self {
        Self { node, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignTargetKind {
    Ident(String),
    Field {
        target: Expr,
        field: String,
    },
    Index {
        target: Expr,
        index: Expr,
    },
    Slice {
        target: Expr,
        start: Option<Expr>,
        end: Option<Expr>,
    },
    MultiIndex {
        target: Expr,
        selectors: Vec<SliceSelector>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub default_expr: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<FnDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub node: StmtKind,
    pub span: Span,
}

impl Stmt {
    pub fn new(node: StmtKind, span: Span) -> Self {
        Self { node, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Import {
        path: String,
        is_file_path: bool,
        alias: String,
    },
    VarDecl {
        names: Vec<String>,
        ty: TypeExpr,
        init: Option<Expr>,
    },
    Assign {
        target: AssignTarget,
        value: Expr,
    },
    MultiAssign {
        targets: Vec<AssignTarget>,
        values: Vec<Expr>,
    },
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    For {
        var_name: String,
        var_ty: Option<TypeExpr>,
        range_start: Expr,
        range_end: Expr,
        body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Return(Option<Expr>),
    ExprStmt(Expr),
    Print {
        args: Vec<Expr>,
    },
    FnInput {
        targets: Vec<InputTarget>,
    },
    FnDecl(FnDecl),
    ClassDecl(ClassDecl),
}
