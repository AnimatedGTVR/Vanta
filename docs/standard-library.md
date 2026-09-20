# Vanta standard library

All APIs below are implemented. Functions described as recoverable may be handled with `ask ... else`.

## Core output

- `emit(value)::void` writes the value and a newline to standard output.
- `emitError(value)::void` writes the value and a newline to standard error.

## `String`

| Function | Result |
| --- | --- |
| `String.Length(text)` | Unicode scalar count as `int` |
| `String.Contains(text, needle)` | Whether `needle` occurs |
| `String.StartsWith(text, prefix)` | Prefix test |
| `String.EndsWith(text, suffix)` | Suffix test |
| `String.ToUpper(text)` | Uppercase string |
| `String.ToLower(text)` | Lowercase string |
| `String.Trim(text)` | Leading/trailing whitespace removed |
| `String.Replace(text, from, to)` | All matches replaced |
| `String.From(value)` | Display representation |
| `String.Compare(left, right)` | `-1`, `0`, or `1` by byte ordering |
| `String.Split(text, separator)` | `list<string>`; empty separator is rejected |
| `String.IndexOf(text, needle)` | Character index or `-1` |
| `String.Substring(text, start, length)` | Character-based substring |
| `String.ToInt(text)` | Parsed `int`; recoverable on invalid input |

## `List`

- `List.Length(list)::int`
- `List.Push(list, value)::list` returns a new list.
- `List.Contains(list, value)::bool`
- `List.Join(list<string>, separator)::string`

Lists are values; `List.Push` does not mutate the original binding.

## `Map`

- `Map.Has(map, key)::bool`
- `Map.Get(map, key)` returns the value or a recoverable missing-key failure.
- `Map.GetOr(map, key, fallback)` returns the value or fallback.
- `Map.Keys(map)::list<string>` returns keys in deterministic sorted order.

## `Config`

`Config.Parse(text)::map<string>` reads line-oriented configuration:

```text
# comment
ID=abora
PRETTY_NAME="Abora Everest"
anix.desktop='cosmic'
```

Blank lines and full-line comments are ignored. Keys may contain ASCII letters, digits after the first character, `_`, `.`, and `-`. Values may be unquoted, single quoted, or double quoted. Double-quoted values accept `\n`, `\r`, `\t`, `\"`, and `\\`.

Malformed lines, invalid keys, duplicate keys, unterminated quotes/escapes, and unknown escapes are recoverable failures containing the source line number.

## `Math`

- `Math.Sqrt(value::float)::float` requires a non-negative float.

## `Path`

| Function | Behavior |
| --- | --- |
| `Path.Join(parts::list<string>)` | Joins using host path rules |
| `Path.Parent(path)` | Parent path; recoverable if absent |
| `Path.FileName(path)` | Final component; recoverable if absent |
| `Path.Extension(path)` | Extension without `.`, or empty string |
| `Path.IsAbsolute(path)` | Absolute-path test |
| `Path.Canonicalize(path)` | Resolves an existing path; recoverable on I/O failure |

## `File`

- `File.Exists(path)::bool`
- `File.ReadText(path)::string` — recoverable I/O and UTF-8 failure.
- `File.WriteText(path, contents)::void`
- `File.AppendText(path, contents)::void` creates the file if needed.
- `File.Copy(from, to)::void`
- `File.Remove(path)::void` treats a missing file as success.

File operations do not elevate privileges and inherit the Vanta process's permissions.

## `Dir`

- `Dir.Exists(path)::bool`
- `Dir.Create(path)::void` creates missing parents.
- `Dir.List(path)::list<string>` returns sorted entry names.

## `Process`

Prefer argument-vector APIs for automation:

- `Process.Exec([program, ...args])::int` inherits the terminal and returns the exit code.
- `Process.Capture([program, ...args])::string` captures stdout and fails recoverably on nonzero exit.
- `Process.Result([program, ...args])::ProcessResult` captures a command without treating nonzero exit as failure. Fields are `success::bool`, `code::int`, `stdout::string`, and `stderr::string`.
- `Process.Exists(program)::bool` searches `PATH`; Unix targets require an executable permission bit.
- `Process.Exit(code)::void` terminates the Vanta program.

`Process.Run(command)` and `Process.Output(command)` invoke the platform shell. Never build their command strings from untrusted input. `Process.Run` returns an exit code; `Process.Output` captures stdout and fails on nonzero exit.

Spawn failures are recoverable. A process terminated by a signal has no normal exit code; APIs that require one report a failure, while `Process.Result` uses `-1`.

## `Env`

- `Env.Args()::list<string>` returns arguments after the program path.
- `Env.Has(name)::bool`
- `Env.Get(name)::string` is recoverable when missing or non-Unicode.

## `System`

- `System.Platform()` returns Rust's target OS name, such as `linux`.
- `System.Arch()` returns the target architecture, such as `x86_64`.
- `System.CurrentDir()` returns the process working directory.
- `System.HomeDir()` uses `HOME` on Unix and `USERPROFILE` on Windows.

## Failure boundary

Filesystem, environment, process-start, conversion, and configuration-input failures are generally recoverable. Type mistakes, invalid control flow, unknown names, arithmetic errors, immutable assignment, and invalid indexing are program diagnostics and are not catchable with `ask`.
