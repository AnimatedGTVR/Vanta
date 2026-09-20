use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
        "String.Compare" => binary_string(name, arguments, |left, right| {
            Ok(Value::Int(match left.cmp(right) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            }))
        }),
        "String.Split" => binary_string(name, arguments, |text, separator| {
            if separator.is_empty() {
                return Err(error("`String.Split` needs a non-empty separator"));
            }
            Ok(Value::List(
                text.split(separator)
                    .map(|part| Value::String(part.to_owned()))
                    .collect(),
            ))
        }),
        "String.IndexOf" => binary_string(name, arguments, |text, needle| {
            Ok(Value::Int(match text.find(needle) {
                Some(byte) => char_count(&text[..byte])?,
                None => -1,
            }))
        }),
        "String.Substring" => match arguments {
            [Value::String(text), Value::Int(start), Value::Int(length)] => {
                let (Ok(start), Ok(length)) = (usize::try_from(*start), usize::try_from(*length))
                else {
                    return Some(Err(error(
                        "`String.Substring` start and length must not be negative",
                    )));
                };
                let count = text.chars().count();
                if start.checked_add(length).is_none_or(|end| end > count) {
                    Err(error(format!(
                        "`String.Substring` range {start}+{length} is outside a string of {count} character(s)"
                    )))
                } else {
                    Ok(Value::String(
                        text.chars().skip(start).take(length).collect(),
                    ))
                }
            }
            _ => Err(type_error(name, "a string and two ints")),
        },
        "String.ToInt" => unary_string(name, arguments, |text| {
            text.trim().parse::<i64>().map(Value::Int).map_err(|_| {
                Diagnostic::failure(format!("`{name}` could not read `{text}` as an int"))
            })
        }),
        "Math.Sqrt" => match arguments {
            [Value::Float(value)] if *value >= 0.0 => Ok(Value::Float(value.sqrt())),
            [Value::Float(_)] => Err(error("`Math.Sqrt` needs a non-negative float")),
            _ => Err(type_error(name, "one float")),
        },
        "List.Length" => match arguments {
            [Value::List(items)] => i64::try_from(items.len())
                .map(Value::Int)
                .map_err(|_| error("list length exceeds the supported integer range")),
            _ => Err(type_error(name, "a list")),
        },
        "List.Push" => match arguments {
            [Value::List(items), item] => {
                let mut items = items.clone();
                items.push(item.clone());
                Ok(Value::List(items))
            }
            _ => Err(type_error(name, "a list and an item")),
        },
        "List.Contains" => match arguments {
            [Value::List(items), item] => Ok(Value::Bool(items.contains(item))),
            _ => Err(type_error(name, "a list and an item")),
        },
        "List.Join" => match arguments {
            [Value::List(items), Value::String(separator)] => items
                .iter()
                .map(|item| match item {
                    Value::String(text) => Ok(text.as_str()),
                    _ => Err(error("`List.Join` needs a list of strings")),
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|parts| Value::String(parts.join(separator))),
            _ => Err(type_error(name, "a list of strings and a string")),
        },
        "Path.Join" => string_list(name, arguments).map(|parts| {
            Value::String(
                parts
                    .into_iter()
                    .collect::<PathBuf>()
                    .to_string_lossy()
                    .into_owned(),
            )
        }),
        "Path.Parent" => unary_string(name, arguments, |path| {
            Path::new(path)
                .parent()
                .map(|parent| Value::String(parent.to_string_lossy().into_owned()))
                .ok_or_else(|| Diagnostic::failure(format!("`{name}`: `{path}` has no parent")))
        }),
        "Path.FileName" => unary_string(name, arguments, |path| {
            Path::new(path)
                .file_name()
                .map(|part| Value::String(part.to_string_lossy().into_owned()))
                .ok_or_else(|| Diagnostic::failure(format!("`{name}`: `{path}` has no file name")))
        }),
        "Path.Extension" => unary_string(name, arguments, |path| {
            Ok(Value::String(
                Path::new(path)
                    .extension()
                    .map(|part| part.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ))
        }),
        "Path.IsAbsolute" => unary_string(name, arguments, |path| {
            Ok(Value::Bool(Path::new(path).is_absolute()))
        }),
        "Path.Canonicalize" => unary_string(name, arguments, |path| {
            fs::canonicalize(path)
                .map(|path| Value::String(path.to_string_lossy().into_owned()))
                .map_err(|cause| io_error(name, path, cause))
        }),
        "Dir.Exists" => unary_string(name, arguments, |path| {
            Ok(Value::Bool(std::path::Path::new(path).is_dir()))
        }),
        "Dir.Create" => unary_string(name, arguments, |path| {
            fs::create_dir_all(path)
                .map(|()| Value::Void)
                .map_err(|cause| io_error(name, path, cause))
        }),
        "Dir.List" => unary_string(name, arguments, |path| {
            let mut names = fs::read_dir(path)
                .and_then(|entries| {
                    entries
                        .map(|entry| {
                            entry.map(|entry| entry.file_name().to_string_lossy().into_owned())
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .map_err(|cause| io_error(name, path, cause))?;
            names.sort();
            Ok(Value::List(names.into_iter().map(Value::String).collect()))
        }),
        "File.Copy" => binary_string(name, arguments, |from, to| {
            fs::copy(from, to)
                .map(|_| Value::Void)
                .map_err(|cause| io_error(name, &format!("{from}` -> `{to}"), cause))
        }),
        "File.Remove" => unary_string(name, arguments, |path| match fs::remove_file(path) {
            Ok(()) => Ok(Value::Void),
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(Value::Void),
            Err(cause) => Err(io_error(name, path, cause)),
        }),
        "Process.Exec" => argv(name, arguments).and_then(|argv| {
            Command::new(&argv[0])
                .args(&argv[1..])
                .status()
                .map_err(|cause| {
                    Diagnostic::failure(format!("`{name}` could not start `{}`: {cause}", argv[0]))
                })
                .and_then(|status| exit_code(name, status))
        }),
        "Process.Capture" => argv(name, arguments).and_then(|argv| {
            let output = Command::new(&argv[0])
                .args(&argv[1..])
                .stdin(Stdio::null())
                .output()
                .map_err(|cause| {
                    Diagnostic::failure(format!("`{name}` could not start `{}`: {cause}", argv[0]))
                })?;
            if !output.status.success() {
                return Err(Diagnostic::failure(format!(
                    "`{}` exited with {}: {}",
                    argv.join(" "),
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            String::from_utf8(output.stdout)
                .map(Value::String)
                .map_err(|_| Diagnostic::failure(format!("`{name}` produced non-UTF-8 output")))
        }),
        "Process.Exists" => unary_string(name, arguments, |program| {
            Ok(Value::Bool(command_exists(program)))
        }),
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
                .map_err(|cause| Diagnostic::failure(format!("`{name}` failed: {cause}")))
                .and_then(|status| exit_code(name, status))
        }),
        "Process.Output" => unary_string(name, arguments, |command| {
            let output = shell_command(command)
                .output()
                .map_err(|cause| Diagnostic::failure(format!("`{name}` failed: {cause}")))?;
            if !output.status.success() {
                return Err(Diagnostic::failure(format!(
                    "`{name}` exited with status {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
            String::from_utf8(output.stdout)
                .map(Value::String)
                .map_err(|_| Diagnostic::failure(format!("`{name}` produced non-UTF-8 output")))
        }),
        "Env.Has" => unary_string(name, arguments, |key| {
            Ok(Value::Bool(std::env::var_os(key).is_some()))
        }),
        "Env.Get" => unary_string(name, arguments, |key| {
            std::env::var(key).map(Value::String).map_err(|cause| {
                Diagnostic::failure(format!("`{name}` could not read `{key}`: {cause}"))
            })
        }),
        "System.Platform" => no_arguments(name, arguments, || {
            Ok(Value::String(std::env::consts::OS.to_owned()))
        }),
        "System.Arch" => no_arguments(name, arguments, || {
            Ok(Value::String(std::env::consts::ARCH.to_owned()))
        }),
        "System.CurrentDir" => no_arguments(name, arguments, || {
            std::env::current_dir()
                .map(|path| Value::String(path.to_string_lossy().into_owned()))
                .map_err(|cause| Diagnostic::failure(format!("`{name}` failed: {cause}")))
        }),
        "System.HomeDir" => no_arguments(name, arguments, || {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|path| Value::String(PathBuf::from(path).to_string_lossy().into_owned()))
                .ok_or_else(|| {
                    Diagnostic::failure(format!("`{name}` could not find the home directory"))
                })
        }),
        _ => return None,
    };
    Some(result)
}

fn no_arguments(
    name: &str,
    arguments: &[Value],
    function: impl FnOnce() -> Result<Value, Diagnostic>,
) -> Result<Value, Diagnostic> {
    arity(name, arguments, 0)?;
    function()
}

fn string_list(name: &str, arguments: &[Value]) -> Result<Vec<String>, Diagnostic> {
    arity(name, arguments, 1)?;
    let Value::List(items) = &arguments[0] else {
        return Err(type_error(name, "one list<string>"));
    };
    items
        .iter()
        .map(|item| match item {
            Value::String(value) => Ok(value.clone()),
            _ => Err(type_error(name, "one list<string>")),
        })
        .collect()
}

fn command_exists(program: &str) -> bool {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return executable(path);
    }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|directory| executable(&directory.join(program)))
    })
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
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
    Diagnostic::failure(format!("`{name}` failed for `{path}`: {cause}"))
}

/// A process's argv from a non-empty `list<string>`.
fn argv(name: &str, arguments: &[Value]) -> Result<Vec<String>, Diagnostic> {
    arity(name, arguments, 1)?;
    let Value::List(items) = &arguments[0] else {
        return Err(type_error(name, "a list<string> of program and arguments"));
    };
    let argv = items
        .iter()
        .map(|item| match item {
            Value::String(text) => Ok(text.clone()),
            _ => Err(type_error(name, "a list<string> of program and arguments")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if argv.is_empty() {
        return Err(error(format!("`{name}` needs at least the program to run")));
    }
    Ok(argv)
}

fn exit_code(name: &str, status: std::process::ExitStatus) -> Result<Value, Diagnostic> {
    status
        .code()
        .map(|code| Value::Int(i64::from(code)))
        .ok_or_else(|| {
            Diagnostic::failure(format!("`{name}`: the process was stopped by a signal"))
        })
}

fn char_count(text: &str) -> Result<i64, Diagnostic> {
    i64::try_from(text.chars().count())
        .map_err(|_| error("string length exceeds the supported integer range"))
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
