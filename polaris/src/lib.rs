pub mod ast;
pub mod diagnostic;
pub mod interpreter;
pub mod lexer;
pub mod module;
pub mod parser;
pub mod token;
pub mod typecheck;
pub mod value;

use std::fs;
use std::io::Write;
use std::path::Path;

pub use diagnostic::{Diagnostic, Span};
pub use interpreter::Interpreter;
pub use lexer::Lexer;
pub use module::{ModuleLoader, ModuleProgram};
pub use parser::Parser;
pub use typecheck::TypeChecker;

pub fn run_source(source: &str, input: &str) -> Result<String, String> {
    run_source_with_modules(source, input, Vec::new())
}

pub fn run_source_with_modules(
    source: &str,
    input: &str,
    modules: Vec<(&str, &str)>,
) -> Result<String, String> {
    let program = ModuleLoader::new(modules).load_entry_source(source, None)?;
    let mut checker = TypeChecker::new();
    checker
        .check_module_program(&program)
        .map_err(|d| program.format_diagnostic(&d))?;
    let mut interp = Interpreter::new(input);
    let mut out = Vec::new();
    interp
        .run_module_program(&program, &mut out)
        .map_err(|d| program.format_diagnostic(&d))?;
    String::from_utf8(out).map_err(|e| e.to_string())
}

pub fn run_file(source: &str, out: &mut dyn Write) -> Result<(), String> {
    let program = ModuleLoader::new(Vec::new()).load_entry_source(source, None)?;
    let mut checker = TypeChecker::new();
    checker
        .check_module_program(&program)
        .map_err(|d| program.format_diagnostic(&d))?;
    let mut interp = Interpreter::from_stdin();
    interp
        .run_module_program(&program, out)
        .map_err(|d| program.format_diagnostic(&d))
}

pub fn run_file_path(path: &Path, out: &mut dyn Write) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read source file '{}': {}", path.display(), e))?;
    let program = ModuleLoader::new(Vec::new()).load_entry_source(&source, Some(path))?;
    let mut checker = TypeChecker::new();
    checker
        .check_module_program(&program)
        .map_err(|d| program.format_diagnostic(&d))?;
    let mut interp = Interpreter::from_stdin();
    interp
        .run_module_program(&program, out)
        .map_err(|d| program.format_diagnostic(&d))
}
