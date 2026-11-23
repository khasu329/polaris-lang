#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Int,
    Uint,
    Float,
    Char,
    String,
    Bool,
    Void,
    Custom(String),
}

impl TypeRef {
    pub fn display_name(&self) -> String {
        match self {
            TypeRef::Int => "int".to_string(),
            TypeRef::Uint => "uint".to_string(),
            TypeRef::Float => "float".to_string(),
            TypeRef::Char => "char".to_string(),
            TypeRef::String => "string".to_string(),
            TypeRef::Bool => "bool".to_string(),
            TypeRef::Void => "void".to_string(),
            TypeRef::Custom(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeSpec {
    Scalar(TypeRef),
    Array1D {
        elem: TypeRef,
        len: Option<Expr>,
    },
    Table {
        row_types: Vec<TypeRef>,
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
    SelfRef,
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    FieldAccess {
        target: Box<Expr>,
        field: String,
    },
    MethodCall {
        target: Box<Expr>,
        method: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
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
        ty: TypeSpec,
    },
    Var {
        target: Expr,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeSpec>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub ty: Option<TypeSpec>,
    pub default_expr: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub name: String,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<FnDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl {
        names: Vec<String>,
        ty: TypeSpec,
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
        var_ty: Option<TypeRef>,
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
