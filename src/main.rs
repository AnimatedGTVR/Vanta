use std::{env, fs, process};

fn main() {
    let arguments: Vec<String> = env::args().collect();
    if arguments.len() != 3 || arguments[1] != "run" {
        eprintln!("Vanta 0.1.0\n\nUsage: vanta run <file.vanta>");
        process::exit(2);
    }
    let path = &arguments[2];
    let source = fs::read_to_string(path).unwrap_or_else(|error| {
        eprintln!("Error[V0000]: could not read `{path}`: {error}");
        process::exit(1);
    });
    match vanta::run(&source) {
        Ok(output) => print!("{output}"),
        Err(error) => {
            eprintln!("{error}");
            process::exit(1);
        }
    }
}
