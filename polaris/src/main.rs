use std::env;
use std::fs;
use std::io;

use polaris::run_file;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        let source = fs::read_to_string(&args[1]).expect("Failed to read source file");
        let mut stdout = io::stdout();
        if let Err(e) = run_file(&source, &mut stdout) {
            eprintln!("Runtime error: {}", e);
            std::process::exit(1);
        }
    } else {
        println!("Usage: {} <source_file>", args[0]);
    }
}
