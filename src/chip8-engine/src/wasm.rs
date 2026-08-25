//! WebAssembly bindings. Enable with `--features wasm`.

use std::time::Duration;

use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;

use crate::{Chip8, Chip8Config};

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Browser-oriented façade over the platform-independent [`Chip8`] core.
#[wasm_bindgen]
pub struct WasmChip8 {
    core: Chip8,
}

#[wasm_bindgen]
impl WasmChip8 {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: Option<u32>) -> Self {
        let mut config = Chip8Config::default();
        if let Some(seed) = seed {
            config.seed = seed;
        }
        Self {
            core: Chip8::new(config),
        }
    }

    pub fn load_rom(&mut self, rom: Uint8Array) -> Result<(), JsValue> {
        self.core.load_rom(&rom.to_vec()).map_err(js_error)
    }
    pub fn step(&mut self) -> Result<bool, JsValue> {
        self.core.step().map(|result| result.drew).map_err(js_error)
    }
    pub fn run_cycles(&mut self, cycles: u32) -> Result<bool, JsValue> {
        let mut drew = false;
        for _ in 0..cycles {
            drew |= self.core.step().map_err(js_error)?.drew;
        }
        Ok(drew)
    }
    pub fn advance_time_ms(&mut self, milliseconds: f64) -> u32 {
        if !milliseconds.is_finite() || milliseconds <= 0.0 {
            return 0;
        }
        let Ok(elapsed) = Duration::try_from_secs_f64(milliseconds / 1000.0) else {
            return 0;
        };
        self.core.advance_timers(elapsed)
    }
    pub fn set_key(&mut self, key: u8, pressed: bool) -> Result<(), JsValue> {
        self.core.set_key(key, pressed).map_err(js_error)
    }
    pub fn framebuffer(&self) -> Vec<u8> {
        self.core.framebuffer().to_vec()
    }
    pub fn sound_active(&self) -> bool {
        self.core.sound_active()
    }
}
