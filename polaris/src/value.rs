use crate::ast::PrimType;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Uint(u64),
    Float(f64),
    Char(char),
    String(String),
    Bool(bool),
    Array(Vec<Value>),
}

impl Value {
    pub fn default_for(ty: PrimType) -> Self {
        match ty {
            PrimType::Int => Value::Int(0),
            PrimType::Uint => Value::Uint(0),
            PrimType::Float => Value::Float(0.0),
            PrimType::Char => Value::Char('\0'),
            PrimType::String => Value::String(String::new()),
            PrimType::Bool => Value::Bool(false),
        }
    }

    pub fn display_str(&self) -> String {
        match self {
            Value::Int(v) => v.to_string(),
            Value::Uint(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::Char(v) => v.to_string(),
            Value::String(v) => v.clone(),
            Value::Bool(v) => v.to_string(),
            Value::Array(v) => v
                .iter()
                .map(|item| item.display_str())
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Executed,
    NoMatch,
}
