//! WebAssembly bindings. Enable with `--features wasm`.

use std::time::Duration;

use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;

use crate::{Chip8, Chip8Config, CompatibilityProfile, CycleResult};

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Browser-oriented façade over the platform-independent [`Chip8`] core.
#[wasm_bindgen]
pub struct WasmChip8 {
    core: Chip8,
    profile: CompatibilityProfile,
}

/// Compatibility choices exposed to JavaScript without stringly-typed values.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmCompatibilityProfile {
    OriginalChip8,
    Chip48,
    Modern,
    SuperChip10,
    SuperChip11,
    SuperChipCompatibility,
    SuperChipModern,
    SuperChip,
    XoChip,
}

impl From<WasmCompatibilityProfile> for CompatibilityProfile {
    fn from(value: WasmCompatibilityProfile) -> Self {
        match value {
            WasmCompatibilityProfile::OriginalChip8 => Self::OriginalChip8,
            WasmCompatibilityProfile::Chip48 => Self::Chip48,
            WasmCompatibilityProfile::Modern => Self::Modern,
            WasmCompatibilityProfile::SuperChip10 => Self::SuperChip10,
            WasmCompatibilityProfile::SuperChip11 => Self::SuperChip11,
            WasmCompatibilityProfile::SuperChipCompatibility => Self::SuperChipCompatibility,
            WasmCompatibilityProfile::SuperChipModern => Self::SuperChipModern,
            WasmCompatibilityProfile::SuperChip => Self::SuperChip,
            WasmCompatibilityProfile::XoChip => Self::XoChip,
        }
    }
}

/// Detailed outcome of one emulation cycle for debugger hosts.
#[wasm_bindgen]
pub struct WasmCycleResult {
    drew: bool,
    waiting_for_key: bool,
    waiting_for_vblank: bool,
    halted: bool,
}

impl From<CycleResult> for WasmCycleResult {
    fn from(value: CycleResult) -> Self {
        Self {
            drew: value.drew,
            waiting_for_key: value.waiting_for_key,
            waiting_for_vblank: value.waiting_for_vblank,
            halted: value.halted,
        }
    }
}

#[wasm_bindgen]
impl WasmCycleResult {
    #[wasm_bindgen(getter)]
    pub fn drew(&self) -> bool {
        self.drew
    }
    #[wasm_bindgen(getter)]
    pub fn waiting_for_key(&self) -> bool {
        self.waiting_for_key
    }
    #[wasm_bindgen(getter)]
    pub fn waiting_for_vblank(&self) -> bool {
        self.waiting_for_vblank
    }
    #[wasm_bindgen(getter)]
    pub fn halted(&self) -> bool {
        self.halted
    }
}

#[wasm_bindgen]
impl WasmChip8 {
    #[wasm_bindgen(constructor)]
    pub fn new(seed: Option<u32>) -> Self {
        Self::with_profile(WasmCompatibilityProfile::OriginalChip8, seed)
    }

    #[wasm_bindgen]
    pub fn with_profile(profile: WasmCompatibilityProfile, seed: Option<u32>) -> Self {
        let profile = CompatibilityProfile::from(profile);
        let mut config = Chip8Config {
            profile,
            ..Chip8Config::default()
        };
        if let Some(seed) = seed {
            config.seed = seed;
        }
        Self {
            core: Chip8::new(config),
            profile,
        }
    }

    pub fn load_rom(&mut self, rom: Uint8Array) -> Result<(), JsValue> {
        self.core.load_rom(&rom.to_vec()).map_err(js_error)
    }
    pub fn step(&mut self) -> Result<bool, JsValue> {
        self.core.step().map(|result| result.drew).map_err(js_error)
    }
    pub fn step_with_result(&mut self) -> Result<WasmCycleResult, JsValue> {
        self.core.step().map(Into::into).map_err(js_error)
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
    pub fn display_width(&self) -> usize {
        self.core.display_dimensions().0
    }
    pub fn display_height(&self) -> usize {
        self.core.display_dimensions().1
    }
    pub fn sound_active(&self) -> bool {
        self.core.sound_active()
    }
    pub fn audio_pattern(&self) -> Vec<u8> {
        self.core.audio_pattern().to_vec()
    }
    pub fn audio_pitch(&self) -> u8 {
        self.core.audio_pitch()
    }
    pub fn program_counter(&self) -> u16 {
        self.core.program_counter()
    }
    pub fn current_opcode(&self) -> Option<u32> {
        let pc = usize::from(self.core.program_counter());
        let memory = self.core.memory();
        let high = *memory.get(pc)?;
        let low = *memory.get(pc.checked_add(1)?)?;
        let opcode = u16::from_be_bytes([high, low]);
        if opcode == 0xF000 && self.profile == CompatibilityProfile::XoChip {
            let extra = u16::from_be_bytes([
                *memory.get(pc.checked_add(2)?)?,
                *memory.get(pc.checked_add(3)?)?,
            ]);
            Some((u32::from(opcode) << 16) | u32::from(extra))
        } else {
            Some(u32::from(opcode))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wasm_profiles_map_to_core_profiles() {
        assert_eq!(
            CompatibilityProfile::from(WasmCompatibilityProfile::XoChip),
            CompatibilityProfile::XoChip
        );
        assert_eq!(
            CompatibilityProfile::from(WasmCompatibilityProfile::SuperChip11),
            CompatibilityProfile::SuperChip11
        );
    }
}
