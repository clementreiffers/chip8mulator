use std::{fmt, time::Duration};

use crate::{
    instruction,
    peripherals::{DISPLAY_PIXELS, Framebuffer},
};

pub(crate) const RAM_SIZE: usize = 4096;
pub(crate) const PROGRAM_START: u16 = 0x200;
pub(crate) const FONT_START: u16 = 0x50;
const STACK_SIZE: usize = 16;
const TIMER_PERIOD: Duration = Duration::from_nanos(16_666_667);

const FONT: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompatibilityProfile {
    #[default]
    OriginalChip8,
    Chip48,
    Modern,
}

impl CompatibilityProfile {
    pub(crate) const fn shift_uses_vy(self) -> bool {
        matches!(self, Self::OriginalChip8)
    }
    pub(crate) const fn increment_i_after_store_load(self) -> bool {
        matches!(self, Self::OriginalChip8)
    }
    pub(crate) const fn jump_uses_vx(self) -> bool {
        matches!(self, Self::Chip48)
    }
    pub(crate) const fn draw_wraps(self) -> bool {
        !matches!(self, Self::Chip48)
    }
    pub(crate) const fn logic_clears_vf(self) -> bool {
        matches!(self, Self::OriginalChip8)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chip8Config {
    pub profile: CompatibilityProfile,
    pub seed: u32,
}

impl Default for Chip8Config {
    fn default() -> Self {
        Self {
            profile: CompatibilityProfile::OriginalChip8,
            seed: 0xC808_2024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CycleResult {
    pub drew: bool,
    pub waiting_for_key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chip8Error {
    RomTooLarge { size: usize, maximum: usize },
    InvalidOpcode { opcode: u16, pc: u16 },
    StackOverflow,
    StackUnderflow,
    MemoryOutOfBounds { address: u16 },
    ProgramCounterOutOfBounds { pc: u16 },
    InvalidKey { key: u8 },
}

impl fmt::Display for Chip8Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RomTooLarge { size, maximum } => {
                write!(f, "ROM is {size} bytes; maximum is {maximum}")
            }
            Self::InvalidOpcode { opcode, pc } => {
                write!(f, "invalid opcode {opcode:#06x} at {pc:#05x}")
            }
            Self::StackOverflow => f.write_str("CHIP-8 stack overflow"),
            Self::StackUnderflow => f.write_str("CHIP-8 stack underflow"),
            Self::MemoryOutOfBounds { address } => {
                write!(f, "memory access out of bounds at {address:#05x}")
            }
            Self::ProgramCounterOutOfBounds { pc } => {
                write!(f, "program counter out of bounds at {pc:#05x}")
            }
            Self::InvalidKey { key } => write!(f, "invalid CHIP-8 key {key}"),
        }
    }
}

impl std::error::Error for Chip8Error {}

/// CHIP-8 machine state. It performs no I/O and is deterministic for a given seed.
#[derive(Debug, Clone)]
pub struct Chip8 {
    pub(crate) memory: [u8; RAM_SIZE],
    pub(crate) v: [u8; 16],
    pub(crate) i: u16,
    pub(crate) pc: u16,
    pub(crate) stack: [u16; STACK_SIZE],
    pub(crate) sp: usize,
    pub(crate) delay_timer: u8,
    pub(crate) sound_timer: u8,
    pub(crate) keys: [bool; 16],
    pub(crate) display: Framebuffer,
    pub(crate) config: Chip8Config,
    rng_state: u32,
    timer_remainder: Duration,
}

impl Chip8 {
    #[must_use]
    pub fn new(config: Chip8Config) -> Self {
        let mut memory = [0; RAM_SIZE];
        let font_end = usize::from(FONT_START) + FONT.len();
        memory[usize::from(FONT_START)..font_end].copy_from_slice(&FONT);
        Self {
            memory,
            v: [0; 16],
            i: 0,
            pc: PROGRAM_START,
            stack: [0; STACK_SIZE],
            sp: 0,
            delay_timer: 0,
            sound_timer: 0,
            keys: [false; 16],
            display: Framebuffer::default(),
            config,
            rng_state: config.seed,
            timer_remainder: Duration::ZERO,
        }
    }

    pub fn load_rom(&mut self, rom: &[u8]) -> Result<(), Chip8Error> {
        let maximum = RAM_SIZE - usize::from(PROGRAM_START);
        if rom.len() > maximum {
            return Err(Chip8Error::RomTooLarge {
                size: rom.len(),
                maximum,
            });
        }
        self.memory[usize::from(PROGRAM_START)..].fill(0);
        self.memory[usize::from(PROGRAM_START)..usize::from(PROGRAM_START) + rom.len()]
            .copy_from_slice(rom);
        self.v = [0; 16];
        self.i = 0;
        self.pc = PROGRAM_START;
        self.stack = [0; STACK_SIZE];
        self.sp = 0;
        self.delay_timer = 0;
        self.sound_timer = 0;
        self.keys = [false; 16];
        self.display.clear();
        self.rng_state = self.config.seed;
        self.timer_remainder = Duration::ZERO;
        Ok(())
    }

    pub fn step(&mut self) -> Result<CycleResult, Chip8Error> {
        instruction::step(self)
    }

    /// Advance CHIP-8 delay and sound timers at exactly 60 Hz.
    pub fn advance_timers(&mut self, elapsed: Duration) -> u32 {
        self.timer_remainder = self.timer_remainder.saturating_add(elapsed);
        let elapsed_nanos = self.timer_remainder.as_nanos();
        let ticks = elapsed_nanos / TIMER_PERIOD.as_nanos();
        let remainder = elapsed_nanos % TIMER_PERIOD.as_nanos();
        self.timer_remainder = Duration::from_nanos(remainder as u64);
        let returned_ticks = ticks.min(u128::from(u32::MAX)) as u32;
        let decrement = ticks.min(u128::from(u8::MAX)) as u8;
        self.delay_timer = self.delay_timer.saturating_sub(decrement);
        self.sound_timer = self.sound_timer.saturating_sub(decrement);
        returned_ticks
    }

    pub fn set_key(&mut self, key: u8, pressed: bool) -> Result<(), Chip8Error> {
        let Some(slot) = self.keys.get_mut(usize::from(key)) else {
            return Err(Chip8Error::InvalidKey { key });
        };
        *slot = pressed;
        Ok(())
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u8; DISPLAY_PIXELS] {
        self.display.pixels()
    }
    /// Complete RAM snapshot for diagnostics and host-side inspection.
    #[must_use]
    pub fn memory(&self) -> &[u8] {
        &self.memory
    }
    #[must_use]
    pub fn sound_active(&self) -> bool {
        self.sound_timer != 0
    }
    #[must_use]
    pub fn registers(&self) -> &[u8; 16] {
        &self.v
    }
    #[must_use]
    pub fn index_register(&self) -> u16 {
        self.i
    }
    #[must_use]
    pub fn program_counter(&self) -> u16 {
        self.pc
    }
    #[must_use]
    pub fn delay_timer(&self) -> u8 {
        self.delay_timer
    }
    #[must_use]
    pub fn sound_timer(&self) -> u8 {
        self.sound_timer
    }

    pub(crate) fn read(&self, address: u16) -> Result<u8, Chip8Error> {
        self.memory
            .get(usize::from(address))
            .copied()
            .ok_or(Chip8Error::MemoryOutOfBounds { address })
    }
    pub(crate) fn write(&mut self, address: u16, value: u8) -> Result<(), Chip8Error> {
        let Some(slot) = self.memory.get_mut(usize::from(address)) else {
            return Err(Chip8Error::MemoryOutOfBounds { address });
        };
        *slot = value;
        Ok(())
    }
    pub(crate) fn next_random(&mut self) -> u8 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.rng_state >> 24) as u8
    }
}

impl Default for Chip8 {
    fn default() -> Self {
        Self::new(Chip8Config::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn font_and_rom_are_loaded_at_standard_addresses() {
        let mut chip = Chip8::default();
        chip.load_rom(&[0x60, 0xAB]).expect("valid rom");
        assert_eq!(chip.memory[0x50], 0xF0);
        assert_eq!(chip.memory[0x200], 0x60);
    }
    #[test]
    fn oversized_rom_is_rejected() {
        let mut chip = Chip8::default();
        let rom = vec![0; RAM_SIZE - usize::from(PROGRAM_START) + 1];
        assert!(matches!(
            chip.load_rom(&rom),
            Err(Chip8Error::RomTooLarge { .. })
        ));
    }
    #[test]
    fn timers_tick_at_60_hz() {
        let mut chip = Chip8 {
            delay_timer: 2,
            sound_timer: 1,
            ..Chip8::default()
        };
        assert_eq!(chip.advance_timers(TIMER_PERIOD), 1);
        assert_eq!(chip.delay_timer, 1);
        assert!(!chip.sound_active());
    }
    #[test]
    fn invalid_key_is_rejected() {
        assert!(matches!(
            Chip8::default().set_key(16, true),
            Err(Chip8Error::InvalidKey { key: 16 })
        ));
    }
}
