# Vanta language reference

This reference covers syntax accepted by the current Rust implementation.

## Program structure

Every file begins with one module declaration:

```vanta
module Main;
```

The entry module must contain `func Start()::void`. The interpreter calls it automatically.

Imports follow the module declaration and precede every other declaration:

```vanta
module Main;
@use Updater.Guard;
```

`@use Updater.Guard;` resolves `Updater/Guard.vanta` relative to the entry program. That file must declare `module Updater.Guard;`. The final module segment is its call alias, such as `Guard.Allows()`.

## Bindings and mutability

`let` creates an immutable binding. `mut` creates a mutable binding:

```vanta
let channel = "stable";
let attempts::int = 3;
mut completed = 0;
completed = completed + 1;
```

Assignments cannot change a binding's runtime type. Rebinding the same name inside an active function scope is rejected.

Immutable module bindings are evaluated once, in source order, before `Start`:

```vanta
module PlayerController;

let walkSpeed::float = 4.0;
let runSpeed::float = walkSpeed + 3.0;
```

Module initializers may use literals, collections, packs, operators, indexing, and earlier module bindings. They cannot call functions or perform I/O. Mutable module bindings are rejected.

## Built-in types

| Type | Example | Notes |
| --- | --- | --- |
| `int` | `42` | Signed 64-bit integer with checked arithmetic |
| `float` | `4.0` | Finite 64-bit floating point |
| `bool` | `true` | `true` or `false` |
| `string` | `"Vanta"` | UTF-8 text |
| `void` | function-only | No returned value |
| `list<T>` | `[1, 2, 3]` | Ordered homogeneous collection when typed |
| `map<T>` | returned by `Config.Parse` | String-keyed homogeneous map when typed |

Type annotations use `::`:

```vanta
let edition::string = "COSMIC";
let ports::list<int> = [80, 443];
let release::map<string> = Config.Parse(text);
```

Lists and maps validate every element/value when assigned to a declared type or passed through a typed parameter.

## Strings

Supported escapes are `\n`, `\t`, `\r`, `\0`, `\"`, and `\\`. Unknown escapes are errors.

Interpolation reads bindings and nested pack fields:

```vanta
emit("channel={channel}");
emit("player={player.name} x={player.position.x}");
```

Interpolation is variable/field based; it does not evaluate arbitrary expressions.

## Functions

Functions declare parameters and an explicit return type:

```vanta
func Add(let left::int, let right::int)::int {
    return left + right;
}

func Announce(let message::string)::void {
    emit(message);
}
```

Parameters begin with `let` or `mut`. A `mut` parameter makes its local binding assignable; values are currently passed by value. Functions are private by default. `pub func` makes a function callable from importing modules.

The interpreter limits nested Vanta calls to 256 and reports excessive recursion as a diagnostic.

## Operators

Arithmetic operators are `+`, `-`, `*`, `/`, and `%`. Integer arithmetic is checked for overflow. Float arithmetic rejects non-finite results. Division and remainder by zero are errors.

Comparisons are `==`, `!=`, `<`, `<=`, `>`, and `>=`. Logical operators are `!`, `&&`, and `||`; `&&` and `||` short-circuit.

String `+` concatenates two strings. Other unsupported type/operator combinations are runtime diagnostics.

## Conditions

```vanta
if score >= 10 {
    emit("ready");
} else if score > 0 {
    emit("warming up");
} else {
    emit("idle");
}
```

Conditions must evaluate to `bool`. `return;` exits a `void` function early.

## Loops

Ranges include both endpoints, reverse automatically, and accept a positive step magnitude:

```vanta
for number in 1..100 { emit(number); }
for number in 10..1 { emit(number); }
for number in 1..10 by 2 { emit(number); }
```

Lists are iterable:

```vanta
for name in ["stable", "edge"] {
    emit(name);
}
```

Conditional and unconditional loops use `while` and `loop`:

```vanta
while pending > 0 {
    pending = pending - 1;
}

loop {
    if Finished() { break; }
    skip;
}
```

`break` exits the nearest loop. `skip` begins its next iteration. Both are rejected outside loops.

## Pack types

`pack` declares a product type containing every listed field. Pack names must begin with an uppercase letter:

```vanta
pack Position {
    x::float;
    y::float;
}

pack Player {
    name::string;
    position::Position;
}
```

Construction names every field:

```vanta
let player::Player = Player {
    name = "Nova",
    position = Position { x = 4.0, y = 9.0 },
};
```

Missing, unknown, duplicate, and wrongly typed fields are rejected. Fields are read with `.`, may be nested, and are read-only in the current milestone. Pack equality compares the type name and every field.

## Lists, indexing, and maps

List indexes are zero-based integers:

```vanta
let first = channels[0];
```

Negative and out-of-range indexes are program errors and cannot be caught with `ask`.

Maps currently come from standard-library operations such as `Config.Parse`. Access them through `Map.Get`, `Map.GetOr`, `Map.Has`, and `Map.Keys`; map literal syntax is not implemented.

## Recoverable failures and `ask`

External operations and conversions may return recoverable failures. Without handling, the failure stops the program. `ask ... else` handles it explicitly:

```vanta
let text = ask File.ReadText(path) else {
    emitError("could not read {path}: {error}");
    return "";
};
```

Inside `else`, `error` is the failure message. When `ask` initializes a binding, the `else` block must leave the function with `return` or terminate with `Process.Exit`. Statement form may continue:

```vanta
ask File.Remove(path) else {
    emitError(error);
};
```

`ask` does not catch program errors such as unknown functions, invalid operators, immutable assignment, or invalid indexing.

## Comments

```vanta
# Ordinary line comment
#! Documentation-style line comment

#| Block comment
   across multiple lines. |#
```

Comments are discarded by the lexer. Documentation comments do not yet generate API documentation.

## Visibility and names

Functions are private unless declared `pub`. Imported public functions are called through their module alias. Built-in namespaces (`String`, `List`, `Map`, `Config`, `Math`, `Path`, `File`, `Dir`, `Process`, `Env`, and `System`) are reserved and cannot be hidden by imports.

## Not implemented yet

The design direction includes `pick` sum types, pack methods, private fields, verified `init` constructors, generics, requirements/implementations, contracts, effects, ownership/reference rules, constrained numeric/range types, package management, static whole-program checking, a stable ABI, and native code generation. Their final syntax is not part of this reference yet.
