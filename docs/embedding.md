# Embedding Vanta in Rust

Vanta can run as a standalone command or as an interpreter embedded in a Rust engine/application.

## Native host interface

Implement `vanta::interpreter::NativeHost`:

```rust
use vanta::diagnostic::Diagnostic;
use vanta::interpreter::{NativeHost, Value};

struct Engine;

impl NativeHost for Engine {
    fn call(
        &mut self,
        name: &str,
        arguments: &[Value],
    ) -> Option<Result<Value, Diagnostic>> {
        match name {
            "ScriptContext.IsSprintDown" => Some(Ok(Value::Bool(true))),
            _ => None,
        }
    }
}
```

Returning `None` leaves the name available to normal Vanta function resolution. Returning `Some(Err(...))` reports a native failure to Vanta; use `Diagnostic::failure` when scripts should be allowed to handle it with `ask`.

## Running source with a host

```rust
let output = vanta::run_with_host(source, &mut Engine)?;
```

For file/module loading and streamed output, use `vanta::load` plus `interpreter::interpret_with_host`.

## Member-call dispatch

Given a pack value named `ScriptContext`, Vanta translates:

```vanta
ctx.IsSprintDown()
```

into the native name `ScriptContext.IsSprintDown`. The receiver is inserted as argument zero. Remaining Vanta arguments follow it. This keeps engine APIs natural without baking engine-specific functions into Vanta itself.

Namespaced calls such as `Math.Sqrt(...)` remain ordinary built-ins because `Math` is not a variable binding.

## Passing values

`Value` supports integers, floats, booleans, strings, lists, string-keyed maps, packs, and void. Packs use a type name plus a `BTreeMap<String, Value>`, which keeps display and host behavior deterministic.

Native hosts should validate argument count and value variants. The current interface intentionally leaves native signature registration to the embedder; a future static checker/ABI layer may formalize signatures.

## Gameplay example

`examples/player_controller.vanta` implements normalized WASD movement, walking/sprinting, rigidbody velocity, and UI updates. `examples/embed_player_controller.rs` supplies a mock engine host.

Run it with:

```sh
cargo run --example embed_player_controller
```

The example is a model for Modularity integration: Modularity retains ownership of objects, physics, input, and UI; Vanta receives context values and issues native host calls.

## Threading and safety

The interpreter runs on a dedicated scoped thread with an enlarged stack so the Vanta call-depth diagnostic triggers before native stack exhaustion. `NativeHost` must implement `Send`. Host implementations remain responsible for synchronizing access to engine state and for deciding which operations scripts may perform.
