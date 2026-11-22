pub mod ast;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod value;

use std::io::Write;

pub use interpreter::Interpreter;
pub use lexer::Lexer;
pub use parser::Parser;

pub fn run_source(source: &str, input: &str) -> Result<String, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program()?;
    let mut interp = Interpreter::new(input);
    let mut out = Vec::new();
    interp.run(&stmts, &mut out)?;
    String::from_utf8(out).map_err(|e| e.to_string())
}

pub fn run_file(source: &str, out: &mut dyn Write) -> Result<(), String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program()?;
    let mut interp = Interpreter::from_stdin();
    interp.run(&stmts, out)
}
