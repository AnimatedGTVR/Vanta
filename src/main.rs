use std::path::Path;
use std::{env, process};

const USAGE: &str = "Vanta 0.1.0\n\nUsage: vanta run <file.vanta> [arguments...]";

fn main() {
    let arguments: Vec<String> = env::args().collect();
    if arguments.len() < 3 || arguments[1] != "run" {
        eprintln!("{USAGE}");
        process::exit(2);
    }
    match vanta::run_file(Path::new(&arguments[2]), arguments[3..].to_vec()) {
        Ok(code) => process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            process::exit(1);
        }
    }
}
