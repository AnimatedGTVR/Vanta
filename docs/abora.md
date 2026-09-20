# Vanta for Abora and ANIX

Vanta is being prepared for Abora utilities and ANIX configuration work. It is suitable today for experiments, probes, validation helpers, and non-critical tooling. Abora installation, update, recovery, and privileged package operations should not depend exclusively on the early Vanta runtime yet; retain established Rust tooling and recovery paths until the language, static checker, packaging, and ABI stabilize.

## Available building blocks

- Shell-free argv execution through `Process.Exec`, `Process.Capture`, and `Process.Result`.
- Structured exit status, stdout, and stderr.
- Platform, architecture, home directory, and working-directory inspection.
- Safe host-native path construction and canonicalization.
- Executable discovery without invoking a shell.
- Files, directories, environment variables, arguments, modules, and explicit failure handling.
- Typed `map<string>` configuration data.
- `key=value` parsing for `/etc/os-release` and simple ANIX-style settings.

## Reading Linux release information

```vanta
module Abora.Release;

func Start()::void {
    let text = ask File.ReadText("/etc/os-release") else {
        emitError(error);
        Process.Exit(1);
    };
    let release::map<string> = ask Config.Parse(text) else {
        emitError(error);
        Process.Exit(1);
    };

    emit(Map.GetOr(release, "PRETTY_NAME", "Unknown Linux"));
}
```

The runnable version is `examples/abora_release.vanta`.

## Running commands safely

Use argv APIs when any value is variable or external:

```vanta
let result = Process.Result(["nix", "--version"]);
if !result.success {
    emitError(result.stderr);
    Process.Exit(result.code);
}
emit(String.Trim(result.stdout));
```

Avoid `Process.Run` and `Process.Output` for untrusted text because they invoke a shell. Vanta does not automatically elevate privileges. Abora should keep privileged work behind narrowly scoped, audited native tools rather than running whole scripts as root.

## Configuration model

`Config.Parse` is deliberately for line-oriented configuration and release metadata. It is not a Nix parser and must not rewrite Nix source text by substitution. For `/etc/nixos/abora/abora-local.nix`, `anix.nix`, atomic plans, and generated configuration, prefer structured intermediate data and dedicated generators.

ANIX v2 adapters (ANIX Native, MKO, and ModuCPP) should continue resolving into one validated Plan representation before diff/apply. Native JSON support and schema validation are the next relevant Vanta milestones.

## Sensitive data

Password hashes, signing keys, tokens, and generated secrets must not be emitted, interpolated into logs, or stored with default world-readable permissions. Vanta does not yet expose permission-setting or secret-memory APIs, so tools handling sensitive Abora configuration should remain in hardened Rust components for now.

## Recommended early uses

- `abora doctor`-style read-only probes.
- Release/channel inspection.
- Validating simple adapter output.
- Formatting human-readable plan diffs.
- Non-privileged migration helpers.
- Engine or GUI scripting through a restricted `NativeHost`.

## Not production-ready yet

Before Vanta owns critical Abora workflows, it still needs static type checking, structured JSON and schemas, timeouts/cancellation, atomic file replacement, permission APIs, file locking/single-operation protection, cryptographic verification, stable packaging, a hardened privilege boundary, and stronger diagnostics with source spans.
