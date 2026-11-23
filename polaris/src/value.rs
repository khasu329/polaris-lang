use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeType {
    Int,
    Uint,
    Float,
    Char,
    String,
    Bool,
    Void,
    Array(Box<RuntimeType>),
    Table(Vec<RuntimeType>),
    Class {
        module_id: String,
        name: String,
        type_args: Vec<RuntimeType>,
    },
}

#[derive(Debug, Clone)]
pub struct ClassInstance {
    pub module_id: String,
    pub class_name: String,
    pub type_args: Vec<RuntimeType>,
    pub fields: HashMap<String, Value>,
    pub dropped: bool,
}

#[derive(Debug, Clone)]
pub enum ViewStorage {
    Contiguous {
        buffer: Rc<RefCell<Vec<Value>>>,
        start: usize,
        end: usize,
    },
    Slots(Vec<(Rc<RefCell<Vec<Value>>>, usize)>),
    Nested(Vec<SliceView>),
}

#[derive(Debug, Clone)]
pub struct SliceView {
    pub storage: ViewStorage,
}

impl SliceView {
    pub fn contiguous(buffer: Rc<RefCell<Vec<Value>>>, start: usize, end: usize) -> Self {
        Self {
            storage: ViewStorage::Contiguous { buffer, start, end },
        }
    }

    pub fn from_slots(slots: Vec<(Rc<RefCell<Vec<Value>>>, usize)>) -> Self {
        Self {
            storage: ViewStorage::Slots(slots),
        }
    }

    pub fn from_nested(views: Vec<SliceView>) -> Self {
        Self {
            storage: ViewStorage::Nested(views),
        }
    }

    pub fn len(&self) -> usize {
        match &self.storage {
            ViewStorage::Contiguous { start, end, .. } => end.saturating_sub(*start),
            ViewStorage::Slots(s) => s.len(),
            ViewStorage::Nested(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, idx: usize) -> Option<Value> {
        match &self.storage {
            ViewStorage::Contiguous { buffer, start, end } => {
                if start + idx < *end {
                    buffer.borrow().get(start + idx).cloned()
                } else {
                    None
                }
            }
            ViewStorage::Slots(slots) => slots
                .get(idx)
                .and_then(|(buf, i)| buf.borrow().get(*i).cloned()),
            ViewStorage::Nested(views) => views.get(idx).cloned().map(Value::SliceView),
        }
    }

    pub fn get_slot(&self, idx: usize) -> Option<(Rc<RefCell<Vec<Value>>>, usize)> {
        match &self.storage {
            ViewStorage::Contiguous { buffer, start, end } => {
                if start + idx < *end {
                    let abs = start + idx;
                    if abs < buffer.borrow().len() {
                        Some((Rc::clone(buffer), abs))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            ViewStorage::Slots(slots) => slots.get(idx).map(|(b, i)| (Rc::clone(b), *i)),
            ViewStorage::Nested(_) => None,
        }
    }

    pub fn slice(&self, s: usize, e_opt: Option<usize>) -> SliceView {
        let len = self.len();
        let e_clamped = e_opt.unwrap_or(len).min(len);
        let s_clamped = s.min(e_clamped);
        match &self.storage {
            ViewStorage::Contiguous { buffer, start, .. } => {
                SliceView::contiguous(Rc::clone(buffer), start + s_clamped, start + e_clamped)
            }
            ViewStorage::Slots(slots) => {
                SliceView::from_slots(slots[s_clamped..e_clamped].to_vec())
            }
            ViewStorage::Nested(views) => {
                SliceView::from_nested(views[s_clamped..e_clamped].to_vec())
            }
        }
    }

    pub fn collect_leaf_slots(&self, out: &mut Vec<(Rc<RefCell<Vec<Value>>>, usize)>) {
        match &self.storage {
            ViewStorage::Contiguous { buffer, start, end } => {
                let len = buffer.borrow().len();
                let clamped_end = (*end).min(len);
                let clamped_start = (*start).min(clamped_end);
                for i in clamped_start..clamped_end {
                    out.push((Rc::clone(buffer), i));
                }
            }
            ViewStorage::Slots(slots) => {
                for (b, i) in slots {
                    out.push((Rc::clone(b), *i));
                }
            }
            ViewStorage::Nested(views) => {
                for v in views {
                    v.collect_leaf_slots(out);
                }
            }
        }
    }

    pub fn to_vec(&self) -> Vec<Value> {
        match &self.storage {
            ViewStorage::Contiguous { buffer, start, end } => {
                let borrowed = buffer.borrow();
                let clamped_end = (*end).min(borrowed.len());
                let clamped_start = (*start).min(clamped_end);
                borrowed[clamped_start..clamped_end].to_vec()
            }
            ViewStorage::Slots(slots) => slots
                .iter()
                .filter_map(|(buf, i)| buf.borrow().get(*i).cloned())
                .collect(),
            ViewStorage::Nested(views) => views.iter().cloned().map(Value::SliceView).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum RefTarget {
    Var(Rc<RefCell<Value>>),
    Field {
        instance: Rc<RefCell<ClassInstance>>,
        field: String,
    },
    Index {
        buffer: Rc<RefCell<Vec<Value>>>,
        index: usize,
    },
}

impl RefTarget {
    pub fn read(&self) -> Result<Value, String> {
        match self {
            RefTarget::Var(cell) => Ok(cell.borrow().clone()),
            RefTarget::Field { instance, field } => instance
                .borrow()
                .fields
                .get(field)
                .cloned()
                .ok_or_else(|| format!("Field '{}' not found", field)),
            RefTarget::Index { buffer, index } => buffer
                .borrow()
                .get(*index)
                .cloned()
                .ok_or_else(|| format!("Referenced array index {} out of bounds", index)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Uint(u64),
    Float(f64),
    Char(char),
    String(String),
    Bool(bool),
    Array(Rc<RefCell<Vec<Value>>>),
    SliceView(SliceView),
    Table(Vec<Rc<RefCell<Vec<Value>>>>),
    Instance(Rc<RefCell<ClassInstance>>),
    Ref(RefTarget),
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
            (Value::Array(a), Value::Array(b)) => *a.borrow() == *b.borrow(),
            (Value::SliceView(a), Value::SliceView(b)) => a.to_vec() == b.to_vec(),
            (Value::Array(a), Value::SliceView(b)) => *a.borrow() == b.to_vec(),
            (Value::SliceView(a), Value::Array(b)) => a.to_vec() == *b.borrow(),
            (Value::Table(a), Value::Table(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| *x.borrow() == *y.borrow())
            }
            (Value::Instance(a), Value::Instance(b)) => Rc::ptr_eq(a, b),
            (Value::Void, Value::Void) => true,
            _ => false,
        }
    }
}

impl Value {
    pub fn clone_for_value_binding(&self) -> Self {
        match self {
            Value::Array(rc) => {
                let cloned: Vec<Value> = rc
                    .borrow()
                    .iter()
                    .map(|item| item.clone_for_value_binding())
                    .collect();
                Value::Array(Rc::new(RefCell::new(cloned)))
            }
            Value::Table(rows) => {
                let cloned_rows = rows
                    .iter()
                    .map(|row| {
                        let items: Vec<Value> = row
                            .borrow()
                            .iter()
                            .map(|item| item.clone_for_value_binding())
                            .collect();
                        Rc::new(RefCell::new(items))
                    })
                    .collect();
                Value::Table(cloned_rows)
            }
            Value::Ref(target) => target
                .read()
                .map(|v| v.clone_for_value_binding())
                .unwrap_or(Value::Void),
            other => other.clone(),
        }
    }

    pub fn deref_read(&self) -> Result<Value, String> {
        let mut cur = self.clone();
        while let Value::Ref(target) = cur {
            cur = target.read()?;
        }
        Ok(cur)
    }

    pub fn display_str(&self) -> String {
        match self {
            Value::Int(v) => v.to_string(),
            Value::Uint(v) => v.to_string(),
            Value::Float(v) => v.to_string(),
            Value::Char(v) => v.to_string(),
            Value::String(v) => v.clone(),
            Value::Bool(v) => v.to_string(),
            Value::Array(rc) => rc
                .borrow()
                .iter()
                .map(|item| item.display_str())
                .collect::<Vec<_>>()
                .join(" "),
            Value::SliceView(sv) => sv
                .to_vec()
                .iter()
                .map(|item| item.display_str())
                .collect::<Vec<_>>()
                .join(" "),
            Value::Table(rows) => rows
                .iter()
                .map(|row| {
                    row.borrow()
                        .iter()
                        .map(|item| item.display_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Value::Instance(inst) => {
                let b = inst.borrow();
                format!("<{} instance>", b.class_name)
            }
            Value::Ref(target) => target
                .read()
                .map(|v| v.display_str())
                .unwrap_or_else(|_| "<dangling ref>".to_string()),
            Value::Void => String::new(),
        }
    }
}
