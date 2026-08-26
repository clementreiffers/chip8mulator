# chip8-engine

Portable, deterministic CHIP-8 core with no windowing, audio, or filesystem
dependencies. The host schedules CPU cycles, presents the 64×32 monochrome
framebuffer, and produces audio while `sound_active()` is true.

```rust
use std::time::Duration;
use chip8_engine::{Chip8, Chip8Config};

# let rom_bytes = [];
let mut chip = Chip8::new(Chip8Config::default());
chip.load_rom(&rom_bytes)?;
chip.step()?;
chip.advance_timers(Duration::from_millis(16));
let pixels = chip.framebuffer();
# Ok::<(), chip8_engine::Chip8Error>(())
```

`Chip8Config` accepts a deterministic PRNG seed and one of the compatibility
profiles `OriginalChip8` (the default), `Chip48`, `Modern`, or modern
`SuperChip`. Super-CHIP programs can switch the framebuffer between 64×32 and
128×64; query `display_dimensions()` whenever presenting `framebuffer()`.

## WebAssembly

Build the optional façade with:

```sh
cargo build --target wasm32-unknown-unknown --features wasm --release
```

After processing it with `wasm-bindgen`, a browser host can use it as follows:

```js
import init, { WasmChip8 } from "./pkg/chip8_engine.js";

await init();
const chip8 = new WasmChip8(1234);
chip8.load_rom(romUint8Array);
chip8.run_cycles(12);
chip8.advance_time_ms(deltaMs);
canvasPresent(chip8.framebuffer());
if (chip8.sound_active()) hostAudioOn();
```

Use `display_width()` and `display_height()` to size the canvas before
presenting the framebuffer.

The JavaScript host maps keyboard input through `set_key(key, pressed)`, where
`key` is in the CHIP-8 range `0x0..=0xF`.

## Conformance ROM

Run `task test-conformance` from the repository root to dynamically download
corax89's public `test_opcode.ch8` ROM, verify its SHA-256, copy every byte
into CHIP-8 program memory, and execute 20,000 cycles. The downloaded artifact
lives under `.cache/` and is not committed.
