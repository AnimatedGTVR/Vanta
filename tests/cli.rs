//! End-to-end tests of the `vanta` binary: arguments, exit status, modules, output.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn vanta() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vanta"))
}

/// A fresh directory with `files` written into it.
fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vanta-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for (path, contents) in files {
        let file = dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }
    dir
}

#[test]
fn passes_arguments_and_exit_status() {
    let dir = project(
        "args",
        &[(
            "main.vanta",
            r#"module Main;
            func Start()::void {
                let args = Env.Args();
                emit(List.Join(args, "|"));
                if List.Length(args) > 0 && args[0] == "fail" {
                    Process.Exit(3);
                }
                emit("unreachable only when failing");
            }"#,
        )],
    );
    let ok = vanta()
        .arg("run")
        .arg(dir.join("main.vanta"))
        .args(["channel", "set edge"])
        .output()
        .unwrap();
    assert_eq!(ok.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&ok.stdout),
        "channel|set edge\nunreachable only when failing\n"
    );

    let failed = vanta()
        .arg("run")
        .arg(dir.join("main.vanta"))
        .arg("fail")
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&failed.stdout), "fail\n");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn imports_public_functions_from_modules() {
    let dir = project(
        "modules",
        &[
            (
                "main.vanta",
                r#"module Main;
                @use Updater.Guard;
                func Start()::void {
                    emit(Guard.Allows("4.0", "v4.1"));
                }"#,
            ),
            (
                "Updater/Guard.vanta",
                r#"module Updater.Guard;
                pub func Allows(let current::string, let selected::string)::bool {
                    return Strip(selected) != current;
                }
                func Strip(let tag::string)::string {
                    return String.Replace(tag, "v", "");
                }"#,
            ),
        ],
    );
    let run = vanta()
        .arg("run")
        .arg(dir.join("main.vanta"))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&run.stderr), "");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "true\n");
    assert_eq!(run.status.code(), Some(0));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn keeps_module_functions_private_by_default() {
    let dir = project(
        "private",
        &[
            (
                "main.vanta",
                "module Main; @use Tools; func Start()::void { Tools.Helper(); }",
            ),
            (
                "Tools.vanta",
                "module Tools; func Helper()::void { emit(\"hidden\"); }",
            ),
        ],
    );
    let run = vanta()
        .arg("run")
        .arg(dir.join("main.vanta"))
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&run.stderr).contains("private to module `Tools`"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reports_modules_that_declare_the_wrong_name() {
    let dir = project(
        "wrong-name",
        &[
            (
                "main.vanta",
                "module Main; @use Tools; func Start()::void { }",
            ),
            ("Tools.vanta", "module Other; func Start()::void { }"),
        ],
    );
    let run = vanta()
        .arg("run")
        .arg(dir.join("main.vanta"))
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&run.stderr).contains("expected `module Tools;`"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn streams_output_before_the_program_ends() {
    let dir = project(
        "stream",
        &[(
            "main.vanta",
            r#"module Main;
            func Start()::void {
                emit("before");
                Process.Exec(["sh", "-c", "echo child"]);
                emit("after");
                Process.Exit(0);
            }"#,
        )],
    );
    let run = vanta()
        .arg("run")
        .arg(dir.join("main.vanta"))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "before\nchild\nafter\n"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn writes_errors_to_stderr() {
    let dir = project(
        "stderr",
        &[(
            "main.vanta",
            "module Main; func Start()::void { emit(\"out\"); emitError(\"usage: tool <ref>\"); Process.Exit(2); }",
        )],
    );
    let run = vanta()
        .arg("run")
        .arg(dir.join("main.vanta"))
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "out\n");
    assert_eq!(String::from_utf8_lossy(&run.stderr), "usage: tool <ref>\n");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prints_usage_without_a_program() {
    let run = vanta().output().unwrap();
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("vanta run <file.vanta> [arguments...]"));
}
