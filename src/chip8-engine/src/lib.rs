//! A small, deterministic and platform-independent CHIP-8 emulator.
//!
//! The host drives CPU cycles and calls [`Chip8::advance_timers`] with elapsed
//! wall-clock time. This keeps the CHIP-8 60 Hz timers independent from CPU
//! speed and makes the core suitable for native and WebAssembly hosts.

mod instruction;
mod machine;
mod peripherals;

#[cfg(feature = "wasm")]
pub mod wasm;

pub use machine::{Chip8, Chip8Config, Chip8Error, CompatibilityProfile, CycleResult};
pub use peripherals::{DISPLAY_HEIGHT, DISPLAY_PIXELS, DISPLAY_WIDTH};
