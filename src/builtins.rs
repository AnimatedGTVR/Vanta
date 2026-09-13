use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::Command;

use crate::diagnostic::Diagnostic;
use crate::interpreter::Value;

/// Calls a standard-library function, or returns `None` for user-defined names.
pub fn call(name: &str, arguments: &[Value]) -> Option<Result<Value, Diagnostic>> {
    let result = match name {
        "String.Length" => unary_string(name, arguments, |value| {
            i64::try_from(value.chars().count())
                .map(Value::Int)
                .map_err(|_| error("string length exceeds the supported integer range"))
        }),
        "String.Contains" => binary_string(name, arguments, |text, needle| {
            Ok(Value::Bool(text.contains(needle)))
        }),
        "String.StartsWith" => binary_string(name, arguments, |text, prefix| {
            Ok(Value::Bool(text.starts_with(prefix)))
        }),
        "String.EndsWith" => binary_string(name, arguments, |text, suffix| {
            Ok(Value::Bool(text.ends_with(suffix)))
        }),
        "String.ToUpper" => unary_string(name, arguments, |value| {
            Ok(Value::String(value.to_uppercase()))
        }),
        "String.ToLower" => unary_string(name, arguments, |value| {
            Ok(Value::String(value.to_lowercase()))
        }),
        "String.Trim" => unary_string(name, arguments, |value| {
            Ok(Value::String(value.trim().to_owned()))
        }),
        "String.Replace" => {
            if let Err(diagnostic) = arity(name, arguments, 3) {
                Err(diagnostic)
            } else {
                match (&arguments[0], &arguments[1], &arguments[2]) {
                    (Value::String(text), Value::String(from), Value::String(to)) => {
                        Ok(Value::String(text.replace(from, to)))
                    }
                    _ => Err(type_error(name, "three strings")),
                }
            }
        }
        "String.From" => {
            if let Err(diagnostic) = arity(name, arguments, 1) {
                Err(diagnostic)
            } else {
                Ok(Value::String(arguments[0].to_string()))
            }
        }
        "File.Exists" => unary_string(name, arguments, |path| {
            Ok(Value::Bool(std::path::Path::new(path).exists()))
        }),
        "File.ReadText" => unary_string(name, arguments, |path| {
            fs::read_to_string(path)
                .map(Value::String)
                .map_err(|cause| io_error(name, path, cause))
        }),
        "File.WriteText" => binary_string(name, arguments, |path, contents| {
            fs::write(path, contents)
                .map(|()| Value::Void)
                .map_err(|cause| io_error(name, path, cause))
        }),
        "File.AppendText" => binary_string(name, arguments, |path, contents| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut file| file.write_all(contents.as_bytes()))
                .map(|()| Value::Void)
                .map_err(|cause| io_error(name, path, cause))
        }),
        "Process.Run" => unary_string(name, arguments, |command| {
            shell_command(command)
                .status()
                .map_err(|cause| error(format!("`{name}` failed: {cause}")))
                .and_then(|status| {
                    status
                        .code()
                        .map(i64::from)
                        .map(Value::Int)
                        .ok_or_else(|| error(format!("`{name}` ended without an exit code")))
                })
        }),
        "Process.Output" => unary_string(name, arguments, |command| {
            let output = shell_command(command)
                .output()
                .map_err(|cause| error(format!("`{name}` failed: {cause}")))?;
            if !output.status.success() {
                return Err(error(format!(
                    "`{name}` exited with status {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            String::from_utf8(output.stdout)
                .map(Value::String)
                .map_err(|_| error(format!("`{name}` produced non-UTF-8 output")))
        }),
        "Env.Has" => unary_string(name, arguments, |key| {
            Ok(Value::Bool(std::env::var_os(key).is_some()))
        }),
        "Env.Get" => unary_string(name, arguments, |key| {
            std::env::var(key)
                .map(Value::String)
                .map_err(|cause| error(format!("`{name}` could not read `{key}`: {cause}")))
        }),
        _ => return None,
    };
    Some(result)
}

fn unary_string(
    name: &str,
    arguments: &[Value],
    function: impl FnOnce(&str) -> Result<Value, Diagnostic>,
) -> Result<Value, Diagnostic> {
    arity(name, arguments, 1)?;
    match &arguments[0] {
        Value::String(value) => function(value),
        _ => Err(type_error(name, "a string")),
    }
}

fn binary_string(
    name: &str,
    arguments: &[Value],
    function: impl FnOnce(&str, &str) -> Result<Value, Diagnostic>,
) -> Result<Value, Diagnostic> {
    arity(name, arguments, 2)?;
    match (&arguments[0], &arguments[1]) {
        (Value::String(left), Value::String(right)) => function(left, right),
        _ => Err(type_error(name, "two strings")),
    }
}

fn arity(name: &str, arguments: &[Value], expected: usize) -> Result<(), Diagnostic> {
    if arguments.len() == expected {
        Ok(())
    } else {
        Err(error(format!(
            "`{name}` expects {expected} argument(s), received {}",
            arguments.len()
        )))
    }
}

fn type_error(name: &str, expected: &str) -> Diagnostic {
    error(format!("`{name}` expects {expected}"))
}

fn io_error(name: &str, path: &str, cause: std::io::Error) -> Diagnostic {
    error(format!("`{name}` failed for `{path}`: {cause}"))
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    let mut process = Command::new("sh");
    process.arg("-c").arg(command);
    process
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut process = Command::new("cmd");
    process.arg("/C").arg(command);
    process
}

fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message, 0, 0)
}
