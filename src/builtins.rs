use std::collections::BTreeMap;
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
        "Map.Has" => match arguments {
            [Value::Map(entries), Value::String(key)] => Ok(Value::Bool(entries.contains_key(key))),
            _ => Err(type_error(name, "a map and a string key")),
        },
        "Map.Get" => match arguments {
            [Value::Map(entries), Value::String(key)] => entries
                .get(key)
                .cloned()
                .ok_or_else(|| Diagnostic::failure(format!("`{name}`: key `{key}` was not found"))),
            _ => Err(type_error(name, "a map and a string key")),
        },
        "Map.GetOr" => match arguments {
            [Value::Map(entries), Value::String(key), fallback] => Ok(entries
                .get(key)
                .cloned()
                .unwrap_or_else(|| fallback.clone())),
            _ => Err(type_error(
                name,
                "a map, a string key, and a fallback value",
            )),
        },
        "Map.Keys" => match arguments {
            [Value::Map(entries)] => Ok(Value::List(
                entries.keys().cloned().map(Value::String).collect(),
            )),
            _ => Err(type_error(name, "one map")),
        },
        "Config.Parse" => unary_string(name, arguments, parse_key_value),
        "Json.Parse" => unary_string(name, arguments, parse_json),
        "Json.Stringify" => match arguments {
            [value] => stringify_json(value).map(Value::String),
            _ => Err(type_error(name, "one JSON value")),
        },
        "Json.Check" => match arguments {
            [value, schema] => Ok(Value::Bool(validate_schema(value, schema, "$", None).is_ok())),
            _ => Err(type_error(name, "a JSON value and a JSON schema")),
        },
        "Json.Validate" => match arguments {
            [Value::String(text), Value::String(schema)] => validate_json_text(text, schema),
            _ => Err(type_error(name, "JSON text and schema text")),
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
        "Process.Result" => argv(name, arguments).and_then(|argv| {
            let output = Command::new(&argv[0])
                .args(&argv[1..])
                .stdin(Stdio::null())
                .output()
                .map_err(|cause| {
                    Diagnostic::failure(format!("`{name}` could not start `{}`: {cause}", argv[0]))
                })?;
            Ok(Value::Pack {
                name: "ProcessResult".to_owned(),
                fields: BTreeMap::from([
                    (
                        "code".to_owned(),
                        Value::Int(i64::from(output.status.code().unwrap_or(-1))),
                    ),
                    (
                        "stderr".to_owned(),
                        Value::String(String::from_utf8_lossy(&output.stderr).into_owned()),
                    ),
                    (
                        "stdout".to_owned(),
                        Value::String(String::from_utf8_lossy(&output.stdout).into_owned()),
                    ),
                    ("success".to_owned(), Value::Bool(output.status.success())),
                ]),
            })
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

fn parse_key_value(text: &str) -> Result<Value, Diagnostic> {
    let mut entries = BTreeMap::new();
    for (index, raw_line) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return Err(Diagnostic::failure(format!(
                "`Config.Parse` line {line_number} needs `key=value`"
            )));
        };
        let key = raw_key.trim();
        if !valid_config_key(key) {
            return Err(Diagnostic::failure(format!(
                "`Config.Parse` line {line_number} has invalid key `{key}`"
            )));
        }
        let value = config_value(raw_value.trim(), line_number)?;
        if entries
            .insert(key.to_owned(), Value::String(value))
            .is_some()
        {
            return Err(Diagnostic::failure(format!(
                "`Config.Parse` line {line_number} repeats key `{key}`"
            )));
        }
    }
    Ok(Value::Map(entries))
}

fn parse_json(text: &str) -> Result<Value, Diagnostic> {
    JsonParser::new(text).parse()
}

fn stringify_json(value: &Value) -> Result<String, Diagnostic> {
    Ok(match value {
        Value::Null => "null".to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) if value.is_finite() => value.to_string(),
        Value::Float(_) => return Err(error("JSON cannot represent a non-finite float")),
        Value::String(value) => json_string(value),
        Value::List(items) => format!(
            "[{}]",
            items.iter().map(stringify_json).collect::<Result<Vec<_>, _>>()?.join(",")
        ),
        Value::Map(entries) => format!(
            "{{{}}}",
            entries.iter().map(|(key, value)| Ok(format!("{}:{}", json_string(key), stringify_json(value)?)))
                .collect::<Result<Vec<_>, Diagnostic>>()?.join(",")
        ),
        Value::Pack { .. } | Value::Void => return Err(type_error("Json.Stringify", "a JSON value")),
    })
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""), '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"), '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"), '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"), ch if ch < '\u{20}' => {
                output.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => output.push(ch),
        }
    }
    output.push('"');
    output
}

struct JsonParser<'a> { text: &'a str, index: usize, line: usize, column: usize }

impl<'a> JsonParser<'a> {
    fn new(text: &'a str) -> Self { Self { text, index: 0, line: 1, column: 1 } }
    fn parse(mut self) -> Result<Value, Diagnostic> {
        self.space();
        let value = self.value()?;
        self.space();
        if self.peek().is_some() { return Err(self.fail("unexpected characters after JSON value")); }
        Ok(value)
    }
    fn value(&mut self) -> Result<Value, Diagnostic> {
        self.space();
        match self.peek() {
            Some('n') => { self.literal("null")?; Ok(Value::Null) }
            Some('t') => { self.literal("true")?; Ok(Value::Bool(true)) }
            Some('f') => { self.literal("false")?; Ok(Value::Bool(false)) }
            Some('"') => self.string().map(Value::String),
            Some('[') => self.array(), Some('{') => self.object(),
            Some('-' | '0'..='9') => self.number(),
            Some(_) => Err(self.fail("expected a JSON value")), None => Err(self.fail("unexpected end of input")),
        }
    }
    fn array(&mut self) -> Result<Value, Diagnostic> {
        self.bump(); self.space(); let mut items = Vec::new();
        if self.take(']') { return Ok(Value::List(items)); }
        loop {
            items.push(self.value()?); self.space();
            if self.take(']') { break; }
            if !self.take(',') { return Err(self.fail("expected `,` or `]`")); }
        }
        Ok(Value::List(items))
    }
    fn object(&mut self) -> Result<Value, Diagnostic> {
        self.bump(); self.space(); let mut entries = BTreeMap::new();
        if self.take('}') { return Ok(Value::Map(entries)); }
        loop {
            self.space(); if self.peek() != Some('"') { return Err(self.fail("expected an object key")); }
            let key = self.string()?; self.space();
            if !self.take(':') { return Err(self.fail("expected `:` after object key")); }
            let value = self.value()?;
            if entries.insert(key.clone(), value).is_some() { return Err(self.fail(&format!("duplicate object key `{key}`"))); }
            self.space(); if self.take('}') { break; }
            if !self.take(',') { return Err(self.fail("expected `,` or `}`")); }
        }
        Ok(Value::Map(entries))
    }
    fn string(&mut self) -> Result<String, Diagnostic> {
        self.bump(); let mut output = String::new();
        loop { match self.bump() {
            Some('"') => return Ok(output), Some('\\') => match self.bump() {
                Some('"') => output.push('"'), Some('\\') => output.push('\\'), Some('/') => output.push('/'),
                Some('b') => output.push('\u{08}'), Some('f') => output.push('\u{0c}'), Some('n') => output.push('\n'),
                Some('r') => output.push('\r'), Some('t') => output.push('\t'), Some('u') => output.push(self.unicode()?),
                _ => return Err(self.fail("invalid string escape")),
            }, Some(ch) if ch >= '\u{20}' => output.push(ch), Some(_) => return Err(self.fail("control character in string")),
            None => return Err(self.fail("unterminated string")),
        }}
    }
    fn unicode(&mut self) -> Result<char, Diagnostic> {
        let mut value = 0u32;
        for _ in 0..4 { value = value * 16 + self.bump().and_then(|ch| ch.to_digit(16)).ok_or_else(|| self.fail("invalid unicode escape"))?; }
        char::from_u32(value).ok_or_else(|| self.fail("invalid unicode scalar"))
    }
    fn number(&mut self) -> Result<Value, Diagnostic> {
        let start = self.index; self.take('-');
        if self.take('0') { if self.peek().is_some_and(|c| c.is_ascii_digit()) { return Err(self.fail("leading zero in number")); } }
        else { if !self.peek().is_some_and(|c| c.is_ascii_digit()) { return Err(self.fail("invalid number")); } while self.peek().is_some_and(|c| c.is_ascii_digit()) { self.bump(); } }
        let mut float = false;
        if self.take('.') { float = true; if !self.peek().is_some_and(|c| c.is_ascii_digit()) { return Err(self.fail("expected digit after decimal point")); } while self.peek().is_some_and(|c| c.is_ascii_digit()) { self.bump(); } }
        if matches!(self.peek(), Some('e' | 'E')) { float = true; self.bump(); if matches!(self.peek(), Some('+' | '-')) { self.bump(); } if !self.peek().is_some_and(|c| c.is_ascii_digit()) { return Err(self.fail("expected exponent digits")); } while self.peek().is_some_and(|c| c.is_ascii_digit()) { self.bump(); } }
        let raw = &self.text[start..self.index];
        if float { raw.parse::<f64>().ok().filter(|v| v.is_finite()).map(Value::Float) }
        else { raw.parse::<i64>().ok().map(Value::Int) }.ok_or_else(|| self.fail("number is outside Vanta's supported range"))
    }
    fn literal(&mut self, expected: &str) -> Result<(), Diagnostic> { for ch in expected.chars() { if self.bump() != Some(ch) { return Err(self.fail(&format!("expected `{expected}`"))); } } Ok(()) }
    fn space(&mut self) { while self.peek().is_some_and(char::is_whitespace) { self.bump(); } }
    fn take(&mut self, expected: char) -> bool { if self.peek() == Some(expected) { self.bump(); true } else { false } }
    fn peek(&self) -> Option<char> { self.text[self.index..].chars().next() }
    fn bump(&mut self) -> Option<char> { let ch = self.peek()?; self.index += ch.len_utf8(); if ch == '\n' { self.line += 1; self.column = 1; } else { self.column += 1; } Some(ch) }
    fn fail(&self, message: &str) -> Diagnostic { Diagnostic::failure(format!("`Json.Parse` line {}, column {}: {message}", self.line, self.column)) }
}

fn validate_json_text(text: &str, schema_text: &str) -> Result<Value, Diagnostic> {
    let value = parse_json(text)?;
    let schema = parse_json(schema_text).map_err(|mut diagnostic| {
        diagnostic.message = diagnostic.message.replace("`Json.Parse`", "`Json.Validate` schema");
        diagnostic
    })?;
    validate_schema(&value, &schema, "$", Some(text))?;
    Ok(value)
}

fn validate_schema(value: &Value, schema: &Value, path: &str, source: Option<&str>) -> Result<(), Diagnostic> {
    let Value::Map(rules) = schema else {
        return Err(schema_failure(path, "schema must be an object", source));
    };
    if let Some(Value::String(expected)) = rules.get("type") {
        let valid = match expected.as_str() {
            "null" => matches!(value, Value::Null),
            "boolean" => matches!(value, Value::Bool(_)),
            "integer" => matches!(value, Value::Int(_)),
            "number" => matches!(value, Value::Int(_) | Value::Float(_)),
            "string" => matches!(value, Value::String(_)),
            "array" => matches!(value, Value::List(_)),
            "object" => matches!(value, Value::Map(_)),
            _ => return Err(schema_failure(path, &format!("unknown schema type `{expected}`"), source)),
        };
        if !valid {
            return Err(schema_failure(path, &format!("expected {expected}, found {}", json_kind(value)), source));
        }
    }
    if let Value::Map(entries) = value {
        if let Some(Value::List(required)) = rules.get("required") {
            for item in required {
                let Value::String(key) = item else {
                    return Err(schema_failure(path, "`required` must contain strings", source));
                };
                if !entries.contains_key(key) {
                    return Err(schema_failure(path, &format!("missing required property `{key}`"), source));
                }
            }
        }
        if let Some(Value::Map(properties)) = rules.get("properties") {
            for (key, child_schema) in properties {
                if let Some(child) = entries.get(key) {
                    validate_schema(child, child_schema, &format!("{path}.{key}"), source)?;
                }
            }
        }
    }
    if let (Value::List(items), Some(item_schema)) = (value, rules.get("items")) {
        for (index, item) in items.iter().enumerate() {
            validate_schema(item, item_schema, &format!("{path}[{index}]"), source)?;
        }
    }
    Ok(())
}

fn schema_failure(path: &str, message: &str, source: Option<&str>) -> Diagnostic {
    let line = source.map(|text| line_for_path(text, path)).unwrap_or(0);
    Diagnostic::failure(format!("`Json.Validate` schema error at {path}, line {line}: {message}"))
}

fn line_for_path(text: &str, path: &str) -> usize {
    let key = path.rsplit('.').next().unwrap_or("");
    let key = key.split('[').next().unwrap_or(key);
    if !key.is_empty() && key != "$" {
        let needle = format!("\"{key}\"");
        if let Some((index, _)) = text.lines().enumerate().find(|(_, line)| line.contains(&needle)) {
            return index + 1;
        }
    }
    1
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null", Value::Bool(_) => "boolean", Value::Int(_) => "integer",
        Value::Float(_) => "number", Value::String(_) => "string", Value::List(_) => "array",
        Value::Map(_) => "object", Value::Pack { .. } => "pack", Value::Void => "void",
    }
}

fn valid_config_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-'))
}

fn config_value(value: &str, line: usize) -> Result<String, Diagnostic> {
    if let Some(quoted) = value.strip_prefix('"') {
        let Some(inner) = quoted.strip_suffix('"') else {
            return Err(Diagnostic::failure(format!(
                "`Config.Parse` line {line} has an unterminated double quote"
            )));
        };
        let mut output = String::new();
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                output.push(ch);
                continue;
            }
            let escaped = chars.next().ok_or_else(|| {
                Diagnostic::failure(format!(
                    "`Config.Parse` line {line} has an unterminated escape"
                ))
            })?;
            output.push(match escaped {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                other => {
                    return Err(Diagnostic::failure(format!(
                        "`Config.Parse` line {line} has unknown escape `\\{other}`"
                    )));
                }
            });
        }
        return Ok(output);
    }
    if let Some(quoted) = value.strip_prefix('\'') {
        return quoted.strip_suffix('\'').map(str::to_owned).ok_or_else(|| {
            Diagnostic::failure(format!(
                "`Config.Parse` line {line} has an unterminated single quote"
            ))
        });
    }
    Ok(value.to_owned())
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
