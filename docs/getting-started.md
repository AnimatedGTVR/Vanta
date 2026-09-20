# Getting started with Vanta

## Requirements

The reference implementation is a Rust application. Install a current stable Rust toolchain with Cargo, then clone the repository.

## Install the command

From the repository root:

```sh
cargo install --path .
```

This installs the `vanta` executable into Cargo's binary directory, normally `$HOME/.cargo/bin`. Make sure that directory is on `PATH`.

For development without installation:

```sh
cargo run -- run examples/hello.vanta
```

## CLI

```text
Vanta 0.1.0

Usage: vanta run <file.vanta> [arguments...]
```

The command exits with status `2` for invalid CLI usage, `1` for a Vanta diagnostic, or the status supplied to `Process.Exit`. Otherwise it exits successfully.

## First program

Create `hello.vanta`:

```vanta
module Main;

func Greet(let name::string)::string {
    return "Hello, {name}!";
}

func Start()::void {
    emit(Greet("Vanta"));
}
```

Run it:

```sh
vanta run hello.vanta
```

Expected output:

```text
Hello, Vanta!
```

## Program arguments

Arguments after the source path are returned by `Env.Args()`:

```vanta
module Main;

func Start()::void {
    let arguments = Env.Args();
    let count = List.Length(arguments);
    emit("received {count} argument(s)");
}
```

String interpolation currently accepts variables and pack-field paths, not arbitrary expressions.

Run it with arguments:

```sh
vanta run args.vanta stable edge
```

## Diagnostics

Lexer and parser diagnostics include line and column information. Runtime diagnostics currently use the runtime message without precise expression spans. Recoverable external failures can be handled through `ask ... else`; programming errors stop execution.

```vanta
let text = ask File.ReadText("settings.conf") else {
    emitError("settings unavailable: {error}");
    return;
};
```

## Modules on disk

Use `vanta run` rather than the in-memory Rust `run` helper when a program imports modules. Given `@use Updater.Guard;`, Vanta loads `Updater/Guard.vanta` relative to the entry file's directory.

## Development checks

Before opening a change:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The examples are useful end-to-end checks, including `fizzbuzz.vanta`, `packs.vanta`, `player_controller.vanta`, and the `abora_*.vanta` tools.
