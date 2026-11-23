use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::TypeRef;

#[derive(Debug, Clone)]
pub struct ClassInstance {
    pub class_name: String,
    pub fields: HashMap<String, Value>,
    pub dropped: bool,
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Uint(u64),
    Float(f64),
    Char(char),
    String(String),
    Bool(bool),
    Array(Vec<Value>),
    Instance(Rc<RefCell<ClassInstance>>),
    Void,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Uint(a), Value::Uint(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
            (Value::Void, Value::Void) => true,
            _ => false,
        }
    }
}

impl Value {
    pub fn default_for(ty: &TypeRef) -> Self {
        match ty {
            TypeRef::Int => Value::Int(0),
            TypeRef::Uint => Value::Uint(0),
            TypeRef::Float => Value::Float(0.0),
            TypeRef::Char => Value::Char('\0'),
            TypeRef::String => Value::String(String::new()),
            TypeRef::Bool => Value::Bool(false),
            TypeRef::Void | TypeRef::Custom(_) => Value::Void,
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
            Value::Instance(inst) => {
                let b = inst.borrow();
                format!("<{} instance>", b.class_name)
            }
            Value::Void => String::new(),
        }
    }
}
