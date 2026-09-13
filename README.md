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
| `String` | `Length`, `Contains`, `StartsWith`, `EndsWith`, `ToUpper`, `ToLower`, `Trim`, `Replace`, `From` |
| `File` | `Exists`, `ReadText`, `WriteText`, `AppendText` |
| `Process` | `Run`, `Output` |
| `Env` | `Has`, `Get` |

`Process.Run` returns an exit code. `Process.Output` captures successful UTF-8 standard output and reports unsuccessful commands as diagnostics. Both APIs execute through the platform shell, so programs must not place untrusted text into commands.

## Comments

```vanta
# A line comment
#! A documentation-style line comment

#| A block comment
   spanning multiple lines. |#
```

## Current Status

Vanta is in early development. The Rust reference implementation currently supports modules, functions, typed parameters and returns, immutable and mutable bindings, primitive values, expressions, qualified function calls, conditionals, comments, string interpolation, output, file access, environment access, and process execution.

The next milestones are static type checking, source-span diagnostics, `pack` and `pick`, explicit error handling, and native code generation.

**Vanta is not intended to be a beginner-first language.** It assumes some prior programming experience.
