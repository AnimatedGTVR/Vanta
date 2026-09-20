pub mod ast;
pub mod builtins;
pub mod diagnostic;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod token;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use diagnostic::Diagnostic;
use interpreter::{BUILTIN_NAMESPACES, Execution, Modules, NativeHost, Sink};

/// Runs a single-file program from source and returns what it emitted.
/// `@use` needs a file on disk to resolve against; use [`run_file`] for that.
pub fn run(source: &str) -> Result<String, Diagnostic> {
    let program = parse_source(source)?;
    if let Some(used) = program.uses.first() {
        return Err(Diagnostic::new(
            format!("`@use {used}` needs a program file; run it with `vanta run <file>`"),
            0,
            0,
        ));
    }
    let modules = Modules {
        entry: program.module.clone(),
        programs: HashMap::from([(program.module.clone(), program)]),
    };
    let execution = interpreter::interpret(&modules, Vec::new(), Sink::Buffer(String::new()))?;
    Ok(execution.output)
}

/// Runs a single-file program with engine-provided native functions.
pub fn run_with_host(source: &str, host: &mut dyn NativeHost) -> Result<String, Diagnostic> {
    let program = parse_source(source)?;
    if let Some(used) = program.uses.first() {
        return Err(Diagnostic::new(
            format!("`@use {used}` needs a program file; run it with `vanta run <file>`"),
            0,
            0,
        ));
    }
    let modules = Modules {
        entry: program.module.clone(),
        programs: HashMap::from([(program.module.clone(), program)]),
    };
    let execution =
        interpreter::interpret_with_host(&modules, Vec::new(), Sink::Buffer(String::new()), host)?;
    Ok(execution.output)
}

/// Runs the program in `path` with `args` (what `Env.Args()` returns), streaming `emit`
/// to stdout. Returns the exit status: 0, or the code given to `Process.Exit`.
pub fn run_file(path: &Path, args: Vec<String>) -> Result<i32, Diagnostic> {
    let modules = load(path)?;
    interpreter::interpret(&modules, args, Sink::Stdout)
        .map(|Execution { exit_code, .. }| exit_code)
}

/// Loads `path` and every module it reaches through `@use`. `@use Updater.Guard;`
/// resolves to `Updater/Guard.vanta` beside the entry file, which must declare
/// `module Updater.Guard;`. Its `pub` functions are called as `Guard.Name(...)`.
pub fn load(path: &Path) -> Result<Modules, Diagnostic> {
    let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let entry = parse_file(path)?;
    let mut modules = Modules {
        entry: entry.module.clone(),
        programs: HashMap::new(),
    };
    let mut pending = vec![(entry, path.to_path_buf())];
    while let Some((program, file)) = pending.pop() {
        let mut aliases = HashMap::new();
        for used in &program.uses {
            let alias = used.rsplit('.').next().unwrap_or(used);
            if BUILTIN_NAMESPACES.contains(&alias) {
                return Err(in_file(
                    &file,
                    format!("`@use {used}` would hide the standard library's `{alias}`"),
                ));
            }
            if let Some(other) = aliases.insert(alias.to_owned(), used.clone()) {
                return Err(in_file(
                    &file,
                    format!("`@use {other}` and `@use {used}` are both called `{alias}`"),
                ));
            }
            if modules.programs.contains_key(used)
                || pending.iter().any(|(queued, _)| &queued.module == used)
                || used == &program.module
            {
                continue;
            }
            let module_file = root.join(format!("{}.vanta", used.replace('.', "/")));
            let module = parse_file(&module_file)?;
            if &module.module != used {
                return Err(in_file(
                    &module_file,
                    format!(
                        "expected `module {used};` (imported as `@use {used}`), found `module {};`",
                        module.module
                    ),
                ));
            }
            pending.push((module, module_file));
        }
        if modules.programs.contains_key(&program.module) {
            return Err(in_file(
                &file,
                format!(
                    "module `{}` is declared by more than one file",
                    program.module
                ),
            ));
        }
        modules.programs.insert(program.module.clone(), program);
    }
    Ok(modules)
}

fn parse_source(source: &str) -> Result<ast::Program, Diagnostic> {
    parser::parse(lexer::lex(source)?)
}

fn parse_file(path: &Path) -> Result<ast::Program, Diagnostic> {
    let source = fs::read_to_string(path).map_err(|cause| {
        Diagnostic::new(
            format!("could not read `{}`: {cause}", path.display()),
            0,
            0,
        )
    })?;
    parse_source(&source).map_err(|diagnostic| Diagnostic {
        message: format!("{}: {}", path.display(), diagnostic.message),
        ..diagnostic
    })
}

fn in_file(path: &Path, message: String) -> Diagnostic {
    Diagnostic::new(format!("{}: {message}", path.display()), 0, 0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use super::{run, run_with_host};
    use crate::diagnostic::Diagnostic;
    use crate::interpreter::{NativeHost, Value};

    struct SprintHost;

    impl NativeHost for SprintHost {
        fn call(&mut self, name: &str, arguments: &[Value]) -> Option<Result<Value, Diagnostic>> {
            match name {
                "ScriptContext.IsSprintDown" if arguments.len() == 1 => Some(Ok(Value::Bool(true))),
                "ScriptContext.GetMoveInputWASD" if arguments.len() == 3 => Some(Ok(Value::Pack {
                    name: "Vec3".into(),
                    fields: BTreeMap::from([
                        ("x".into(), Value::Float(3.0)),
                        ("y".into(), Value::Float(0.0)),
                        ("z".into(), Value::Float(4.0)),
                    ]),
                })),
                _ => None,
            }
        }
    }

    #[test]
    fn runs_functions_arithmetic_and_interpolation() {
        let source = r#"
            module Main;
            func Add(let a::int, let b::int)::int { return a + b; }
            func Start()::void {
                let answer::int = Add(20, 22);
                emit("Answer: {answer}");
            }
        "#;
        assert_eq!(run(source).unwrap(), "Answer: 42\n");
    }

    #[test]
    fn supports_mutability_and_conditionals() {
        let source = r#"
            module Main;
            func Start()::void {
                mut score = 2;
                score = score * 5;
                if score >= 10 { emit("win"); } else { emit("lose"); }
            }
        "#;
        assert_eq!(run(source).unwrap(), "win\n");
    }

    #[test]
    fn rejects_assignment_to_let() {
        let source = "module Main; func Start()::void { let x = 1; x = 2; }";
        assert!(run(source).unwrap_err().message.contains("immutable"));
    }

    #[test]
    fn rejects_out_of_range_integer_literals() {
        let source = "module Main; func Start()::void { emit(9223372036854775808); }";
        assert!(run(source).unwrap_err().message.contains("out of range"));
    }

    #[test]
    fn reports_integer_overflow() {
        let source = "module Main; func Start()::void { emit(9223372036854775807 + 1); }";
        assert!(run(source).unwrap_err().message.contains("overflow"));
    }

    #[test]
    fn supports_float_math_comparisons_and_types() {
        let source = r#"
            module Main;
            func Length(let x::float, let y::float)::float {
                return Math.Sqrt(x * x + y * y);
            }
            func Start()::void {
                let length::float = Length(3.0, 4.0);
                emit(length);
                emit(length > 4.999);
                emit(-1.5 + 2.0);
                emit(5.5 % 2.0);
            }
        "#;
        assert_eq!(run(source).unwrap(), "5\ntrue\n0.5\n1.5\n");
    }

    #[test]
    fn provides_immutable_module_configuration() {
        let source = r#"
            module Main;
            let walkSpeed::float = 4.0;
            let runSpeed::float = walkSpeed + 3.0;

            func Speed(let sprinting::bool)::float {
                if sprinting { return runSpeed; }
                return walkSpeed;
            }

            func Start()::void {
                emit(Speed(false));
                emit(Speed(true));
                emit("walk={walkSpeed}");
            }
        "#;
        assert_eq!(run(source).unwrap(), "4\n7\nwalk=4\n");
    }

    #[test]
    fn dispatches_pack_member_calls_to_a_native_host() {
        let source = r#"
            module Main;
            pack Vec3 { x::float; y::float; z::float; }
            pack ScriptContext { object::bool; }
            func Start()::void {
                let ctx = ScriptContext { object = true };
                let move = ctx.GetMoveInputWASD(0.0, 0.0);
                emit(move.x);
                emit(ctx.IsSprintDown());
            }
        "#;
        assert_eq!(run_with_host(source, &mut SprintHost).unwrap(), "3\ntrue\n");
    }

    #[test]
    fn rejects_mutable_or_effectful_module_bindings() {
        let mutable = "module Main; mut speed = 4.0; func Start()::void { }";
        assert!(run(mutable).unwrap_err().message.contains("must use `let`"));

        let call = "module Main; let speed = Math.Sqrt(16.0); func Start()::void { }";
        assert!(
            run(call)
                .unwrap_err()
                .message
                .contains("cannot call functions")
        );
    }

    #[test]
    fn rejects_assignment_to_module_bindings() {
        let source = r#"
            module Main;
            let speed = 4.0;
            func Start()::void { speed = 7.0; }
        "#;
        assert!(run(source).unwrap_err().message.contains("immutable"));
    }

    #[test]
    fn keeps_float_literals_distinct_from_ranges() {
        let source = r#"
            module Main;
            func Start()::void {
                emit(1.25);
                for value in 1..2 { emit(value); }
            }
        "#;
        assert_eq!(run(source).unwrap(), "1.25\n1\n2\n");
    }

    #[test]
    fn rejects_invalid_float_operations() {
        let division = "module Main; func Start()::void { emit(1.0 / 0.0); }";
        assert!(
            run(division)
                .unwrap_err()
                .message
                .contains("division by zero")
        );

        let square_root = "module Main; func Start()::void { emit(Math.Sqrt(-1.0)); }";
        assert!(
            run(square_root)
                .unwrap_err()
                .message
                .contains("non-negative")
        );
    }

    #[test]
    fn limits_recursive_calls() {
        let source = r#"
            module Main;
            func Loop()::void { Loop(); }
            func Start()::void { Loop(); }
        "#;
        assert!(
            run(source)
                .unwrap_err()
                .message
                .contains("maximum call depth")
        );
    }

    #[test]
    fn supports_all_comment_forms() {
        let source = r#"
            # ordinary line comment
            #! documentation-style line comment
            #| block comments
               may span lines |#
            module Main;
            func Start()::void { emit("comments work"); } # trailing comment
        "#;
        assert_eq!(run(source).unwrap(), "comments work\n");
    }

    #[test]
    fn provides_string_apis() {
        let source = r#"
            module Main;
            func Start()::void {
                let text = String.Trim("  Hello, Vanta!  ");
                emit(String.Length(text));
                emit(String.Contains(text, "Vanta"));
                emit(String.Replace(text, "Vanta", "World"));
                emit(String.ToUpper("safe"));
            }
        "#;
        assert_eq!(run(source).unwrap(), "13\ntrue\nHello, World!\nSAFE\n");
    }

    #[test]
    fn provides_environment_apis() {
        let source = r#"
            module Main;
            func Start()::void {
                emit(Env.Has("PATH"));
                emit(String.Length(Env.Get("PATH")) > 0);
            }
        "#;
        assert_eq!(run(source).unwrap(), "true\ntrue\n");
    }

    #[test]
    fn provides_file_apis() {
        let path = std::env::temp_dir().join(format!("vanta-test-{}.txt", std::process::id()));
        let escaped_path = path.to_string_lossy().replace('\\', "\\\\");
        let source = format!(
            r#"module Main;
                func Start()::void {{
                    File.WriteText("{escaped_path}", "Vanta");
                    File.AppendText("{escaped_path}", " works");
                    emit(File.Exists("{escaped_path}"));
                    emit(File.ReadText("{escaped_path}"));
                }}"#
        );
        assert_eq!(run(&source).unwrap(), "true\nVanta works\n");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn iterates_ranges_and_lists_with_block_scopes() {
        let source = r#"
            module Main;
            func Start()::void {
                mut total = 0;
                for i in 0..4 {
                    let doubled = i * 2;
                    total = total + doubled;
                }
                emit(total);
                for name in ["edge", "stable"] {
                    emit("channel {name}");
                }
            }
        "#;
        assert_eq!(run(source).unwrap(), "20\nchannel edge\nchannel stable\n");
    }

    #[test]
    fn supports_inclusive_reverse_and_stepped_ranges() {
        let source = r#"
            module Main;
            func Start()::void {
                for value in 1..5 by 2 { emit(value); }
                for value in 3..1 { emit(value); }
            }
        "#;
        assert_eq!(run(source).unwrap(), "1\n3\n5\n3\n2\n1\n");
    }

    #[test]
    fn supports_while_break_and_skip() {
        let source = r#"
            module Main;
            func Start()::void {
                mut value = 0;
                while value < 6 {
                    value = value + 1;
                    if value == 2 { skip; }
                    if value == 5 { break; }
                    emit(value);
                }
            }
        "#;
        assert_eq!(run(source).unwrap(), "1\n3\n4\n");
    }

    #[test]
    fn supports_infinite_loops_with_break_and_skip() {
        let source = r#"
            module Main;
            func Start()::void {
                mut value = 0;
                loop {
                    value = value + 1;
                    if value == 2 { skip; }
                    emit(value);
                    if value == 4 { break; }
                }
            }
        "#;
        assert_eq!(run(source).unwrap(), "1\n3\n4\n");
    }

    #[test]
    fn returns_from_inside_an_infinite_loop() {
        let source = r#"
            module Main;
            func Find()::int {
                loop { return 42; }
            }
            func Start()::void { emit(Find()); }
        "#;
        assert_eq!(run(source).unwrap(), "42\n");
    }

    #[test]
    fn runs_fizzbuzz_with_modulo_and_else_if() {
        let source = r#"
            module Main;
            func Start()::void {
                for number in 1..15 {
                    if number % 15 == 0 {
                        emit("FizzBuzz");
                    } else if number % 3 == 0 {
                        emit("Fizz");
                    } else if number % 5 == 0 {
                        emit("Buzz");
                    } else {
                        emit(number);
                    }
                }
            }
        "#;
        assert_eq!(
            run(source).unwrap(),
            "1\n2\nFizz\n4\nBuzz\nFizz\n7\n8\nFizz\nBuzz\n11\nFizz\n13\n14\nFizzBuzz\n"
        );
    }

    #[test]
    fn rejects_invalid_loop_control_and_steps() {
        let outside = "module Main; func Start()::void { break; }";
        assert!(run(outside).unwrap_err().message.contains("inside a loop"));

        let zero_step = "module Main; func Start()::void { for i in 1..3 by 0 { emit(i); } }";
        assert!(
            run(zero_step)
                .unwrap_err()
                .message
                .contains("greater than zero")
        );
    }

    #[test]
    fn reports_remainder_by_zero() {
        let source = "module Main; func Start()::void { emit(10 % 0); }";
        assert!(
            run(source)
                .unwrap_err()
                .message
                .contains("remainder by zero")
        );
    }

    #[test]
    fn returns_from_inside_a_loop() {
        let source = r#"
            module Main;
            func FirstLong(let words::list<string>)::string {
                for word in words {
                    if String.Length(word) > 3 { return word; }
                }
                return "";
            }
            func Start()::void { emit(FirstLong(["v4", "edge", "stable"])); }
        "#;
        assert_eq!(run(source).unwrap(), "edge\n");
    }

    #[test]
    fn supports_lists_indexing_and_list_apis() {
        let source = r#"
            module Main;
            func Start()::void {
                mut refs::list<string> = [];
                refs = List.Push(refs, "v3.14");
                refs = List.Push(refs, "v4.0");
                emit(List.Length(refs));
                emit(refs[1]);
                emit(List.Contains(refs, "v3.14"));
                emit(List.Join(refs, ", "));
                emit(refs);
            }
        "#;
        assert_eq!(
            run(source).unwrap(),
            "2\nv4.0\ntrue\nv3.14, v4.0\n[v3.14, v4.0]\n"
        );
    }

    #[test]
    fn rejects_out_of_range_indexes_as_program_errors() {
        let source = r#"
            module Main;
            func Start()::void {
                let value = ask ["a"][3] else { emit("caught"); return; };
            }
        "#;
        assert!(run(source).unwrap_err().message.contains("out of range"));
    }

    #[test]
    fn checks_list_element_types() {
        let source = "module Main; func Start()::void { let xs::list<int> = [1, \"two\"]; }";
        assert!(run(source).unwrap_err().message.contains("declared type"));
    }

    #[test]
    fn supports_whitespace_escapes() {
        let source = r#"module Main; func Start()::void { emit(String.Length("a\r\n\t\0b")); emit(String.Replace("x\ry", "\r", "-")); }"#;
        assert_eq!(run(source).unwrap(), "6\nx-y\n");
    }

    #[test]
    fn rejects_unknown_escapes() {
        let source = r#"module Main; func Start()::void { emit("C:\Users"); }"#;
        let error = run(source).unwrap_err();
        assert!(
            error.message.contains("unknown escape `\\U`"),
            "{}",
            error.message
        );
    }

    #[test]
    fn compares_strings_by_bytes() {
        let source = r#"
            module Main;
            func Start()::void {
                emit(String.Compare("edge", "main"));
                emit(String.Compare("v4", "v4"));
                emit(String.Compare("b", "B"));
            }
        "#;
        assert_eq!(run(source).unwrap(), "-1\n0\n1\n");
    }

    #[test]
    fn provides_version_parsing_string_apis() {
        let source = r#"
            module Main;
            func Start()::void {
                let parts = String.Split("4.10.2", ".");
                emit(String.ToInt(parts[1]) + 1);
                emit(String.IndexOf("v4.1-DEMO2", "-"));
                emit(String.IndexOf("v4.1", "-"));
                emit(String.Substring("v4.1-DEMO2", 1, 3));
            }
        "#;
        assert_eq!(run(source).unwrap(), "11\n4\n-1\n4.1\n");
    }

    #[test]
    fn short_circuits_logical_operators() {
        let source = r#"
            module Main;
            func Boom()::bool { return 1 / 0 == 0; }
            func Start()::void {
                emit(false && Boom());
                emit(true || Boom());
                emit(true && 2 > 1);
            }
        "#;
        assert_eq!(run(source).unwrap(), "false\ntrue\ntrue\n");
    }

    #[test]
    fn ask_handles_recoverable_failures() {
        let source = r#"
            module Main;
            func Read(let path::string)::string {
                let text = ask File.ReadText(path) else {
                    emit("fallback because: {error}");
                    return "default";
                };
                return text;
            }
            func Start()::void {
                emit(Read("/definitely/not/here.vanta"));
                ask String.ToInt("nope") else { emit("not a number"); };
                emit("still running");
            }
        "#;
        let output = run(source).unwrap();
        assert!(output.starts_with(
            "fallback because: `File.ReadText` failed for `/definitely/not/here.vanta`"
        ));
        assert!(output.ends_with("default\nnot a number\nstill running\n"));
    }

    #[test]
    fn ask_binding_else_must_leave_the_function() {
        let source = r#"
            module Main;
            func Start()::void {
                let n = ask String.ToInt("x") else { emit("oops"); };
                emit(n);
            }
        "#;
        assert!(run(source).unwrap_err().message.contains("must `return`"));
    }

    #[test]
    fn ask_does_not_hide_program_errors() {
        let source = r#"
            module Main;
            func Start()::void {
                ask Missing() else { emit("should not run"); };
            }
        "#;
        assert!(
            run(source)
                .unwrap_err()
                .message
                .contains("unknown function")
        );
    }

    #[test]
    fn unhandled_failures_still_stop_the_program() {
        let source = "module Main; func Start()::void { emit(String.ToInt(\"x\")); }";
        let error = run(source).unwrap_err();
        assert!(error.recoverable && error.message.contains("as an int"));
    }

    #[test]
    fn run_rejects_use_without_a_file() {
        let source = "module Main; @use Updater.Guard; func Start()::void { }";
        assert!(
            run(source)
                .unwrap_err()
                .message
                .contains("needs a program file")
        );
    }

    #[test]
    fn provides_directory_and_copy_apis() {
        let dir = std::env::temp_dir().join(format!("vanta-dir-test-{}", std::process::id()));
        let escaped = dir.to_string_lossy().replace('\\', "\\\\");
        let source = format!(
            r#"module Main;
                func Start()::void {{
                    Dir.Create("{escaped}/nested");
                    File.WriteText("{escaped}/nested/b.txt", "b");
                    File.Copy("{escaped}/nested/b.txt", "{escaped}/nested/a.txt");
                    emit(Dir.List("{escaped}/nested"));
                    File.Remove("{escaped}/nested/b.txt");
                    File.Remove("{escaped}/nested/b.txt");
                    emit(Dir.Exists("{escaped}/nested"));
                    emit(File.Exists("{escaped}/nested/b.txt"));
                }}"#
        );
        assert_eq!(run(&source).unwrap(), "[a.txt, b.txt]\ntrue\nfalse\n");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn provides_abora_system_and_path_apis() {
        let executable = std::env::current_exe().unwrap();
        let escaped = executable.to_string_lossy().replace('\\', "\\\\");
        let source = format!(
            r#"module Main;
                func Start()::void {{
                    let joined = Path.Join(["var", "lib", "abora", "state.json"]);
                    emit(Path.FileName(joined));
                    emit(Path.Extension(joined));
                    emit(Path.IsAbsolute(joined));
                    emit(String.Length(System.Platform()) > 0);
                    emit(String.Length(System.Arch()) > 0);
                    emit(Path.IsAbsolute(System.CurrentDir()));
                    emit(Path.IsAbsolute(System.HomeDir()));
                    emit(Process.Exists("{escaped}"));
                }}"#
        );
        assert_eq!(
            run(&source).unwrap(),
            "state.json\njson\nfalse\ntrue\ntrue\ntrue\ntrue\ntrue\n"
        );
    }

    #[test]
    fn path_failures_work_with_ask() {
        let source = r#"
            module Main;
            func Start()::void {
                ask Path.Canonicalize("/definitely/not/a/vanta/path") else {
                    emit("missing");
                };
            }
        "#;
        assert_eq!(run(source).unwrap(), "missing\n");
    }

    #[test]
    fn parses_typed_key_value_configuration_maps() {
        let source = r##"
            module Main;
            func ReleaseName(let values::map<string>)::string {
                return Map.GetOr(values, "PRETTY_NAME", "Unknown Linux");
            }
            func Start()::void {
                let values::map<string> = Config.Parse(
                    "# release metadata\nID=abora\nPRETTY_NAME=\"Abora Everest\"\nanix.desktop='cosmic'"
                );
                emit(ReleaseName(values));
                emit(Map.Get(values, "ID"));
                emit(Map.Has(values, "anix.desktop"));
                emit(Map.GetOr(values, "CHANNEL", "stable"));
                emit(Map.Keys(values));
            }
        "##;
        assert_eq!(
            run(source).unwrap(),
            "Abora Everest\nabora\ntrue\nstable\n[ID, PRETTY_NAME, anix.desktop]\n"
        );
    }

    #[test]
    fn config_errors_are_recoverable_and_report_lines() {
        let source = r#"
            module Main;
            func Start()::void {
                ask Config.Parse("GOOD=yes\nbroken line") else {
                    emit(String.Contains(error, "line 2"));
                };
                ask Config.Parse("DUP=one\nDUP=two") else {
                    emit(String.Contains(error, "repeats key"));
                };
            }
        "#;
        assert_eq!(run(source).unwrap(), "true\ntrue\n");
    }

    #[cfg(unix)]
    #[test]
    fn runs_processes_by_argv_without_a_shell() {
        let source = r#"
            module Main;
            func Start()::void {
                emit(Process.Capture(["printf", "%s", "a; echo injected"]));
                emit(Process.Exec(["sh", "-c", "exit 4"]));
                ask Process.Capture(["false"]) else { emit("capture failed"); };
                ask Process.Exec(["/no/such/program"]) else { emit("could not start"); };
            }
        "#;
        assert_eq!(
            run(source).unwrap(),
            "a; echo injected\n4\ncapture failed\ncould not start\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn returns_structured_process_results_for_nonzero_exits() {
        let source = r#"
            module Main;
            func Start()::void {
                let result = Process.Result([
                    "sh", "-c", "printf output; printf problem >&2; exit 7"
                ]);
                emit(result.success);
                emit(result.code);
                emit(result.stdout);
                emit(result.stderr);
            }
        "#;
        assert_eq!(run(source).unwrap(), "false\n7\noutput\nproblem\n");
    }

    #[cfg(unix)]
    #[test]
    fn structured_process_start_failures_work_with_ask() {
        let source = r#"
            module Main;
            func Start()::void {
                ask Process.Result(["/definitely/not/a/program"]) else {
                    emit("could not start");
                };
            }
        "#;
        assert_eq!(run(source).unwrap(), "could not start\n");
    }

    #[cfg(unix)]
    #[test]
    fn provides_process_apis() {
        let source = r#"
            module Main;
            func Start()::void {
                emit(Process.Output("printf Vanta"));
                emit(Process.Run("exit 7"));
            }
        "#;
        assert_eq!(run(source).unwrap(), "Vanta\n7\n");
    }

    #[test]
    fn constructs_typed_packs_and_reads_nested_fields() {
        let source = r#"
            module Main;
            pack Position { x::int; y::int; }
            pack Player { name::string; position::Position; }

            func Describe(let player::Player)::string {
                return "{player.name} is at {player.position.x},{player.position.y}";
            }

            func Start()::void {
                let player::Player = Player {
                    name = "Nova",
                    position = Position { x = 4, y = 9 },
                };
                emit(Describe(player));
                emit(player);
            }
        "#;
        assert_eq!(
            run(source).unwrap(),
            "Nova is at 4,9\nPlayer { name = Nova, position = Position { x = 4, y = 9 } }\n"
        );
    }

    #[test]
    fn validates_pack_construction() {
        let missing = r#"
            module Main;
            pack Player { name::string; score::int; }
            func Start()::void { let player = Player { name = "Nova" }; }
        "#;
        assert!(
            run(missing)
                .unwrap_err()
                .message
                .contains("missing field `score`")
        );

        let unknown = r#"
            module Main;
            pack Player { name::string; }
            func Start()::void { let player = Player { name = "Nova", rank = 1 }; }
        "#;
        assert!(
            run(unknown)
                .unwrap_err()
                .message
                .contains("unknown field `rank`")
        );

        let wrong_type = r#"
            module Main;
            pack Player { score::int; }
            func Start()::void { let player = Player { score = "high" }; }
        "#;
        assert!(
            run(wrong_type)
                .unwrap_err()
                .message
                .contains("does not match declared type")
        );
    }

    #[test]
    fn rejects_unknown_pack_fields_on_access() {
        let source = r#"
            module Main;
            pack Player { name::string; }
            func Start()::void {
                let player = Player { name = "Nova" };
                emit(player.score);
            }
        "#;
        assert!(
            run(source)
                .unwrap_err()
                .message
                .contains("has no field `score`")
        );
    }

    #[test]
    fn rejects_duplicate_pack_declarations() {
        let source = r#"
            module Main;
            pack Player { name::string; }
            pack Player { score::int; }
            func Start()::void { }
        "#;
        assert!(
            run(source)
                .unwrap_err()
                .message
                .contains("duplicate pack `Player`")
        );
    }
}
