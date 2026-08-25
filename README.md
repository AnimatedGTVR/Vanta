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

## Example

```vanta
unit Player;

* PLAYER *

typ Player = pack {
    imm ID: int;
    Name: string;
    Health: int;
};

typ mut player: Player = {
    ID = 1;
    Name = "Player";
    Health = 100;
};

fuct Damage(amount int): void {
    player.Health -= amount;

    if (player.Health <= 0) {
        terminate 1;
    }
}

* ENTRY *

fuct Start(): void {
    Damage(25);
    terminate 0;
}
```

## Current Status

Vanta is currently in early development. Core syntax, packs, functions, mutability, collections, error handling, VPM, and the compiler/toolchain are still being designed and implemented.

**Vanta is not intended to be a beginner-first language.** It assumes some prior programming experience.
