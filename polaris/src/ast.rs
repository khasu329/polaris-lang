#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimType {
    Int,
    Uint,
    Float,
    Char,
    String,
    Bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeSpec {
    Scalar(PrimType),
    Array1D {
        elem: PrimType,
        len: Option<Expr>, // None => [] / [...], Some(N) => [N] / [...N]
    },
    Table {
        row_types: Vec<PrimType>,
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
pub enum PatternElem {
    Var(String),
    Lit(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractMode {
    Continuous,
    Any,
    Head,
    Tail,
    Single,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Extraction {
    pub patterns: Vec<Vec<PatternElem>>, // 1D: len=1, 2D: len=rows
    pub target: String,
    pub mode: ExtractMode,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WhileCond {
    Extract(Extraction),
    OrIn,
    AndIn,
    BoolExpr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RewriteRule {
    pub lhs: Vec<Vec<PatternElem>>,
    pub guard: Option<Expr>,
    pub rhs: Vec<Vec<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignValue {
    Scalar(Expr),
    Slice {
        source: String,
        row: Option<usize>,
        len: Option<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Token,
    Line,
    Char,
    All,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputTarget {
    Decl {
        names: Vec<String>,
        ty: TypeSpec,
    },
    Slice {
        target: String,
        row: Option<usize>,
        len: Option<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutputTarget {
    pub target: String,
    pub row: Option<usize>,
    pub len: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Ident(String),
    Index {
        target: String,
        row: Option<Expr>,
        index: Expr,
    },
    Slice {
        target: String,
        row: Option<usize>,
        start: Option<Expr>,
        end: Option<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopCtrl {
    None,
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    VarDecl {
        names: Vec<String>,
        ty: TypeSpec,
        init: Option<Expr>,
        input_mode: Option<InputMode>,
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
        var_ty: Option<PrimType>,
        range_start: Expr,
        range_end: Expr,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Print {
        args: Vec<Expr>,
    },
    SliceAssign {
        target: String,
        row: Option<usize>,
        len: Option<Expr>,
        value: AssignValue,
    },
    Output {
        target: String,
        row: Option<usize>,
        len: Option<Expr>,
    },
    FnInput {
        targets: Vec<InputTarget>,
    },
    FnOutput {
        targets: Vec<OutputTarget>,
    },
    While {
        cond: WhileCond,
        step: Vec<Extraction>,
        body: Vec<RewriteRule>,
    },
}