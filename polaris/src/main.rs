use std::env;
use std::io;
use std::path::Path;

use polaris::run_file_path;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        let mut stdout = io::stdout();
        if let Err(e) = run_file_path(Path::new(&args[1]), &mut stdout) {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    } else {
        println!("Usage: {} <source_file>", args[0]);
    }
}
