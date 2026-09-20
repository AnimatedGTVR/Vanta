# Vanta documentation

This manual documents the currently implemented Rust reference interpreter. Vanta is still early software, so the manual separates working syntax from planned language design.

## Start here

1. Install the command with `cargo install --path .`.
2. Run `vanta run examples/hello.vanta`.
3. Read [Getting started](getting-started.md).
4. Continue with the [language reference](language-reference.md).

## Guides

- [Language reference](language-reference.md) — modules, bindings, types, functions, control flow, packs, collections, expressions, failures, and comments.
- [Getting started](getting-started.md) — installation, CLI behavior, first program, arguments, and diagnostics.
- [Standard library](standard-library.md) — every implemented built-in namespace and its failure behavior.
- [Embedding Vanta in Rust](embedding.md) — `NativeHost`, member-call dispatch, values, and the player-controller example.
- [Abora and ANIX](abora.md) — safe process execution, host inspection, configuration parsing, and current production boundaries.

## Runnable examples

| Example | Purpose |
| --- | --- |
| `hello.vanta` | Functions, arithmetic, and interpolation |
| `fizzbuzz.vanta` | Conditions, modulo, and ranges |
| `packs.vanta` | Typed records and nested field access |
| `system.vanta` | Files, processes, environment, and strings |
| `player_controller.vanta` | Engine-native member calls and float math |
| `abora_probe.vanta` | Platform, paths, and executable discovery |
| `abora_command.vanta` | Structured process results |
| `abora_release.vanta` | Typed config maps and `/etc/os-release` |

## Current versus planned

The interpreter currently executes `.vanta` source directly. Static type checking before execution, native code generation, `pick` sum types, pack methods and privacy, `init`, contracts, effects, generics, ownership/reference rules, VPM packages, and a stable native ABI remain planned. Do not depend on proposed syntax for those features yet.
