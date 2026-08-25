# Repository Guidelines

## Project Structure & Module Organization

This repository contains two independent Rust crates under `src/`; it is not a
Cargo workspace. `src/chip8-engine` is the platform-independent CHIP-8 library.
Its public API is in `src/lib.rs`; machine state lives in `src/machine.rs`,
opcode execution in `src/instruction.rs`, display support in
`src/peripherals.rs`, and the optional browser binding in `src/wasm.rs`.

`src/chip8-native-gui` is a separate native host crate. Keep host/UI concerns
out of the engine unless a shared public contract genuinely requires them.

## Build, Test, and Development Commands

Run commands with an explicit manifest path from the repository root:

```sh
cargo test --manifest-path src/chip8-engine/Cargo.toml
cargo fmt --manifest-path src/chip8-engine/Cargo.toml --check
cargo clippy --manifest-path src/chip8-engine/Cargo.toml --all-targets --all-features -- -D warnings
cargo build --manifest-path src/chip8-engine/Cargo.toml --target wasm32-unknown-unknown --features wasm --release
```

The first command runs unit and doctests. The next two enforce formatting and
warning-free Rust. Install the WASM target first, if needed, with
`rustup target add wasm32-unknown-unknown`.

## Coding Style & Naming Conventions

Use Rust 2024 and let `rustfmt` define layout; do not hand-format around it.
Use `snake_case` for functions/modules, `PascalCase` for types and enum
variants, and `SCREAMING_SNAKE_CASE` for constants. Keep the engine
deterministic and free of platform I/O. Return `Result` with `Chip8Error` for
recoverable failures; avoid `unwrap` and `unsafe` in production code.

## Testing Guidelines

Place focused `#[cfg(test)]` modules beside the code they exercise. Name tests
by observable behavior, for example `xor_drawing_reports_collision`. Add tests
for normal behavior, boundary errors, and each affected compatibility profile.
Use small deterministic ROM byte sequences in tests; seed random behavior via
`Chip8Config`.

## Commit & Pull Request Guidelines

The history currently has only an initial commit, so no established message
format exists. Use concise imperative subjects such as `Add CHIP-8 timer tests`.
Keep commits scoped to one concern. Pull requests should state the behavioral
change, list validation commands run, link relevant issues, and include a
browser screenshot only when changing a visual host.
