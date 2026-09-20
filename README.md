![Alt text](Assets/Vanta.png)
# Vanta

**Vanta** is a modern general-purpose and systems programming language designed to scale from everyday software to advanced systems work.

It takes inspiration from languages like Rust, C++, Go, and C#, while keeping its own syntax and structure.

## Goals

Vanta is designed around:

* explicit, readable syntax
* strong mutability rules
* high-level and systems-level development
* multiple ways to solve a problem when more control is needed
* a moderate learning curve with a high skill ceiling
* native-performance-focused development

## First executable milestone

```vanta
module Main;

func Add(let a::int, let b::int)::int {
    return a + b;
}

func Start()::void {
    let answer = Add(20, 22);
    emit("Answer: {answer}");
}
```

Run it with:

```sh
cargo run -- run examples/hello.vanta
```

## Standard library

Vanta's first system APIs use qualified names:

```vanta
module Main;

func Start()::void {
    let message = String.Trim("  Hello from Vanta  ");
    File.WriteText("message.txt", message);

    if File.Exists("message.txt") {
        emit(File.ReadText("message.txt"));
    }

    let kernel = Process.Output("uname -s");
    emit("Kernel: {kernel}");
}
```

Available APIs:

| Namespace | Functions |
| --- | --- |
| `String` | `Length`, `Contains`, `StartsWith`, `EndsWith`, `ToUpper`, `ToLower`, `Trim`, `Replace`, `From`, `Split`, `IndexOf`, `Substring`, `ToInt`, `Compare` |
| `List` | `Length`, `Push`, `Contains`, `Join` |
| `File` | `Exists`, `ReadText`, `WriteText`, `AppendText`, `Copy`, `Remove` |
| `Dir` | `Exists`, `Create`, `List` |
| `Process` | `Exec`, `Capture`, `Exit`, `Run`, `Output` |
| `Env` | `Args`, `Has`, `Get` |

`Process.Exec(["git", "fetch", ref])` runs a program with its arguments directly, no shell involved, sharing the terminal, and returns its exit code. `Process.Capture([...])` does the same but returns standard output, failing if the program exits unsuccessfully. Prefer these: `Process.Run` and `Process.Output` take a single command string and execute it through the platform shell, so programs must never put untrusted text into them.

`emitError(value)` writes a line to standard error, like `emit` does to standard output. `Env.Args()` returns the arguments after the program file (`vanta run tool.vanta channel set edge`), and `Process.Exit(code)` ends the program with that exit status.

## Lists and loops

```vanta
module Main;

func Start()::void {
    mut tags::list<string> = ["v3.14"];
    tags = List.Push(tags, "v4.0");

    for tag in tags {
        emit("tag {tag}");
    }
    let count = List.Length(tags);
    for attempt in 1..4 {
        emit("attempt {attempt} for {count} tags");
    }
    emit(tags[0]);
}
```

`for name in start..end` includes both bounds. Descending ranges such as `10..1` reverse automatically, and `1..10 by 2` uses a custom positive step. Every block has its own scope, so a `let` inside a loop body is fresh on each pass. `&&` and `||` only evaluate their right side when it matters.

## Advanced control flow

Vanta supports `while`, `break`, and `skip` (`skip` begins the next loop iteration):

```vanta
mut value = 0;
while value < 10 {
    value = value + 1;
    if value % 2 == 0 { skip; }
    if value > 7 { break; }
    emit(value);
}
```

`else if` chains are supported, and `%` reports remainder-by-zero and integer-overflow errors as diagnostics.

## FizzBuzz

```vanta
module Main;

func Start()::void {
    for number in 1..100 {
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
```

Run it with `cargo run -- run examples/fizzbuzz.vanta`.

## Pack types

`pack` defines a product type: every value contains every declared field. Pack names begin with an uppercase letter, construction names each field, and `.` reads fields (including nested fields).

```vanta
pack Position {
    x::int;
    y::int;
}

pack Player {
    name::string;
    position::Position;
}

let player::Player = Player {
    name = "Nova",
    position = Position { x = 4, y = 9 },
};

emit("{player.name}: {player.position.x}");
```

Construction rejects missing, unknown, duplicate, or wrongly typed fields. Pack values participate in equality and can be passed to and returned from typed functions. Fields are read-only in this milestone; pack methods, private fields, `init`, and field mutation come later.

Run the complete example with `cargo run -- run examples/packs.vanta`.

## Handling failures

Standard-library calls that touch the outside world (files, processes, environment, number parsing) can fail. A failure stops the program unless it is acknowledged with `ask ... else`:

```vanta
func ReadVersion(let path::string)::string {
    let text = ask File.ReadText(path) else {
        emit("could not read the version: {error}");
        return "unknown";
    };
    return String.Trim(text);
}
```

Inside the `else` block, `error` holds the failure message. When `ask` binds a value, the `else` block must `return` or call `Process.Exit`; as a statement (`ask File.Remove(path) else { ... };`) it may continue. Program errors such as calling an unknown function or indexing past the end of a list are never caught.

## Modules

```vanta
module Main;
@use Updater.Guard;

func Start()::void {
    if !Guard.Allows("4.0", "v3.14") {
        Process.Exit(1);
    }
}
```

`@use Updater.Guard;` loads `Updater/Guard.vanta` next to the program file, which must declare `module Updater.Guard;`. Functions are private by default; the imported module's `pub func` functions are called through its last name segment (`Guard.Allows`).

## Strings

String literals support the escapes `\n`, `\t`, `\r`, `\0`, `\"` and `\\`, and interpolate variables with `{name}`. Any other escape is an error, so a typo such as `"\R"` is reported instead of silently becoming a letter.

## Comments

```vanta
# A line comment
#! A documentation-style line comment

#| A block comment
   spanning multiple lines. |#
```

## Current Status

Vanta is in early development. The Rust reference implementation currently supports modules and `@use` imports, `pub` and private functions, typed parameters and returns, immutable and mutable bindings, primitive values, `list<T>`, typed `pack` values and field access, expressions with short-circuit `&&`/`||`, qualified function calls, `if`/`else if`, inclusive and stepped ranges, `for`/`while`, `break`/`skip`, `ask ... else` failure handling, comments, string interpolation, streaming output, program arguments and exit status, file and directory access, environment access, and process execution.

The next milestones are static type checking, source-span diagnostics, pack methods and privacy, `pick`, explicit error handling, and native code generation.

**Vanta is not intended to be a beginner-first language.** It assumes some prior programming experience.
