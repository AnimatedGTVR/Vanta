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

## Current Status

Vanta is in early development. The Rust reference implementation currently supports modules, functions, typed parameters and returns, immutable and mutable bindings, primitive values, expressions, function calls, conditionals, string interpolation, and `emit`.

The next milestones are static type checking, source-span diagnostics, `pack` and `pick`, explicit error handling, and native code generation.

**Vanta is not intended to be a beginner-first language.** It assumes some prior programming experience.
