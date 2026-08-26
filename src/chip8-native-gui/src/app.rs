use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use chip8_engine::{Chip8, Chip8Config, Chip8Error, CompatibilityProfile};
use winit::keyboard::KeyCode;

use crate::audio::{self, SharedAudio};
use crate::debug::{DebugState, TraceEntry};

pub const DEFAULT_CPU_HZ: u32 = 700;
pub const MIN_CPU_HZ: u32 = 60;
pub const MAX_CPU_HZ: u32 = 2_000;
const MAX_ELAPSED: Duration = Duration::from_millis(100);

pub struct App {
    chip8: Chip8,
    rom: Vec<u8>,
    profile: CompatibilityProfile,
    cpu_hz: u32,
    cpu_remainder: Duration,
    paused: bool,
    halted: bool,
    frame_dirty: bool,
    debug: Option<DebugState>,
    skip_breakpoint_once: Option<u16>,
    audio: SharedAudio,
}

impl App {
    #[cfg(test)]
    pub fn new(
        rom: Vec<u8>,
        debug_mode: bool,
        profile: CompatibilityProfile,
    ) -> Result<Self, Chip8Error> {
        Self::with_audio_state(rom, debug_mode, profile, audio::shared_state())
    }

    pub fn with_audio_state(
        rom: Vec<u8>,
        debug_mode: bool,
        profile: CompatibilityProfile,
        audio: SharedAudio,
    ) -> Result<Self, Chip8Error> {
        let mut app = Self {
            chip8: Chip8::new(Chip8Config {
                profile,
                seed: 0xC808_2024,
            }),
            rom,
            profile,
            cpu_hz: DEFAULT_CPU_HZ,
            cpu_remainder: Duration::ZERO,
            paused: false,
            halted: false,
            frame_dirty: true,
            debug: debug_mode.then(DebugState::default),
            skip_breakpoint_once: None,
            audio,
        };
        app.restart()?;
        Ok(app)
    }

    pub fn set_key(&mut self, key: u8, pressed: bool) -> Result<(), Chip8Error> {
        self.chip8.set_key(key, pressed)
    }

    /// Returns true when a host-level command was consumed.
    pub fn handle_command(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Escape => true,
            KeyCode::Space => {
                self.toggle_pause();
                true
            }
            KeyCode::F10 if self.debug.is_some() => self.step_once().is_ok(),
            KeyCode::F5 => self.restart().is_ok(),
            KeyCode::F1 => self
                .set_profile(CompatibilityProfile::OriginalChip8)
                .is_ok(),
            KeyCode::F2 => self.set_profile(CompatibilityProfile::Chip48).is_ok(),
            KeyCode::F3 => self.set_profile(CompatibilityProfile::Modern).is_ok(),
            KeyCode::F4 => self.set_profile(CompatibilityProfile::SuperChip).is_ok(),
            KeyCode::F6 => self.set_profile(CompatibilityProfile::XoChip).is_ok(),
            _ => false,
        }
    }

    pub fn advance(&mut self, elapsed: Duration) -> Result<bool, Chip8Error> {
        if self.halted {
            return Ok(true);
        }
        if self.paused {
            return Ok(false);
        }

        let elapsed = elapsed.min(MAX_ELAPSED);
        self.chip8.advance_timers(elapsed);
        self.update_audio();
        self.cpu_remainder = self.cpu_remainder.saturating_add(elapsed);
        let cycle_period = Duration::from_nanos(1_000_000_000 / u64::from(self.cpu_hz));

        while self.cpu_remainder >= cycle_period {
            let pc = self.chip8.program_counter();
            if self.should_break_at(pc) {
                self.paused = true;
                self.cpu_remainder = Duration::ZERO;
                break;
            }
            self.cpu_remainder -= cycle_period;
            let result = self.execute_one()?;
            self.frame_dirty |= result.drew;
            if result.halted {
                return Ok(true);
            }
            if result.waiting_for_key || result.waiting_for_vblank {
                break;
            }
        }
        Ok(false)
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u8] {
        self.chip8.framebuffer()
    }

    #[must_use]
    pub fn display_dimensions(&self) -> (usize, usize) {
        self.chip8.display_dimensions()
    }

    #[must_use]
    pub fn cpu_hz(&self) -> u32 {
        self.cpu_hz
    }

    pub fn set_cpu_hz(&mut self, cpu_hz: u32) {
        self.cpu_hz = cpu_hz.clamp(MIN_CPU_HZ, MAX_CPU_HZ);
    }

    #[must_use]
    pub fn memory_size(&self) -> usize {
        self.chip8.memory().len()
    }

    pub fn audio_state(&self) -> SharedAudio {
        Arc::clone(&self.audio)
    }

    pub fn take_frame_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.frame_dirty, false)
    }

    pub fn debug(&self) -> Option<&DebugState> {
        self.debug.as_ref()
    }

    pub fn debug_mut(&mut self) -> Option<&mut DebugState> {
        self.debug.as_mut()
    }

    #[must_use]
    pub fn is_debug_enabled(&self) -> bool {
        self.debug.is_some()
    }

    /// Enables tracing and breakpoints without changing the emulated machine state.
    pub fn enable_debug(&mut self) {
        self.debug.get_or_insert_with(DebugState::default);
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    pub fn toggle_pause(&mut self) {
        if self.paused {
            self.skip_breakpoint_once = Some(self.chip8.program_counter());
        }
        self.paused = !self.paused;
        self.cpu_remainder = Duration::ZERO;
    }

    pub fn step_once(&mut self) -> Result<(), Chip8Error> {
        if self.debug.is_none() {
            return Ok(());
        }
        self.paused = true;
        self.cpu_remainder = Duration::ZERO;
        self.execute_one().map(|_| ())
    }

    pub fn mark_frame_presented(&mut self) {
        if let Some(debug) = &mut self.debug {
            debug.mark_presented(Instant::now());
        }
    }

    fn restart(&mut self) -> Result<(), Chip8Error> {
        self.chip8.load_rom(&self.rom)?;
        self.cpu_remainder = Duration::ZERO;
        self.frame_dirty = true;
        self.halted = false;
        self.skip_breakpoint_once = None;
        self.update_audio();
        if let Some(debug) = &mut self.debug {
            debug.clear_trace();
        }
        Ok(())
    }

    pub fn set_profile(&mut self, profile: CompatibilityProfile) -> Result<(), Chip8Error> {
        self.profile = profile;
        self.chip8 = Chip8::new(Chip8Config {
            profile,
            seed: 0xC808_2024,
        });
        self.restart()
    }

    fn should_break_at(&mut self, pc: u16) -> bool {
        if self.skip_breakpoint_once == Some(pc) {
            self.skip_breakpoint_once = None;
            return false;
        }
        self.debug
            .as_ref()
            .is_some_and(|debug| debug.is_breakpoint(pc))
    }

    fn execute_one(&mut self) -> Result<chip8_engine::CycleResult, Chip8Error> {
        let pc = self.chip8.program_counter();
        let opcode = self.opcode_at(pc);
        let started = Instant::now();
        let result = self.chip8.step();
        let analysis_time = started.elapsed();
        if let (Some(debug), Some(opcode)) = (&mut self.debug, opcode) {
            debug.record(TraceEntry::new(pc, opcode, analysis_time));
        }
        if let Ok(cycle) = &result {
            self.halted |= cycle.halted;
            self.update_audio();
        }
        result
    }

    fn update_audio(&self) {
        self.audio.update(audio::AudioSnapshot {
            pattern: *self.chip8.audio_pattern(),
            pitch: self.chip8.audio_pitch(),
            active: self.chip8.sound_active(),
        });
    }

    fn opcode_at(&self, pc: u16) -> Option<u32> {
        let memory = self.chip8.memory();
        let high = *memory.get(usize::from(pc))?;
        let low = *memory.get(usize::from(pc.checked_add(1)?))?;
        let opcode = u16::from_be_bytes([high, low]);
        if opcode == 0xF000 && self.profile == CompatibilityProfile::XoChip {
            let extra = u16::from_be_bytes([
                *memory.get(usize::from(pc.checked_add(2)?))?,
                *memory.get(usize::from(pc.checked_add(3)?))?,
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
    fn pause_prevents_cpu_execution() {
        let mut app = App::new(vec![0x60, 0x01], false, CompatibilityProfile::OriginalChip8)
            .expect("valid ROM");
        app.handle_command(KeyCode::Space);
        app.advance(Duration::from_millis(10)).expect("valid ROM");
        assert_eq!(app.chip8.registers()[0], 0);
    }

    #[test]
    fn profile_command_restarts_the_machine() {
        let mut app = App::new(vec![0x60, 0x01], false, CompatibilityProfile::OriginalChip8)
            .expect("valid ROM");
        assert!(app.handle_command(KeyCode::F2));
        assert_eq!(app.profile, CompatibilityProfile::Chip48);
        assert_eq!(app.chip8.program_counter(), 0x200);

        assert!(app.handle_command(KeyCode::F4));
        assert_eq!(app.profile, CompatibilityProfile::SuperChip);
        assert!(app.handle_command(KeyCode::F6));
        assert_eq!(app.profile, CompatibilityProfile::XoChip);
    }

    #[test]
    fn cpu_frequency_is_configurable_within_safe_bounds() {
        let mut app = App::new(vec![0x60, 0x01], false, CompatibilityProfile::OriginalChip8)
            .expect("valid ROM");
        assert_eq!(app.cpu_hz(), DEFAULT_CPU_HZ);

        app.set_cpu_hz(1_200);
        assert_eq!(app.cpu_hz(), 1_200);
        app.advance(Duration::from_millis(1)).expect("valid ROM");
        assert_eq!(app.chip8.registers()[0], 1);

        app.set_cpu_hz(1);
        assert_eq!(app.cpu_hz(), MIN_CPU_HZ);
        app.set_cpu_hz(u32::MAX);
        assert_eq!(app.cpu_hz(), MAX_CPU_HZ);
    }

    #[test]
    fn memory_size_matches_the_allocated_ram() {
        let app = App::new(vec![], false, CompatibilityProfile::OriginalChip8).expect("valid ROM");
        assert_eq!(app.memory_size(), 65_536);
    }

    #[test]
    fn breakpoint_stops_before_the_instruction_and_step_executes_it() {
        let mut app = App::new(vec![0x60, 0x01], true, CompatibilityProfile::OriginalChip8)
            .expect("valid ROM");
        app.debug_mut()
            .expect("debug enabled")
            .toggle_breakpoint(0x200);
        app.advance(Duration::from_millis(10)).expect("valid ROM");
        assert!(app.is_paused());
        assert_eq!(app.chip8.program_counter(), 0x200);
        app.step_once().expect("step succeeds");
        assert_eq!(app.chip8.registers()[0], 1);
    }

    #[test]
    fn enabling_debug_preserves_machine_state_and_starts_a_new_trace() {
        let mut app = App::new(vec![0x60, 0x01], false, CompatibilityProfile::OriginalChip8)
            .expect("valid ROM");
        let pc = app.chip8.program_counter();
        let registers = *app.chip8.registers();

        app.enable_debug();

        assert!(app.is_debug_enabled());
        assert_eq!(app.chip8.program_counter(), pc);
        assert_eq!(app.chip8.registers(), &registers);
        assert!(app.debug().expect("debug enabled").trace().is_empty());

        app.step_once().expect("step succeeds");
        assert_eq!(app.debug().expect("debug enabled").trace().len(), 1);
    }

    #[test]
    fn enabling_debug_twice_keeps_the_existing_trace() {
        let mut app = App::new(vec![0x60, 0x01], false, CompatibilityProfile::OriginalChip8)
            .expect("valid ROM");
        app.enable_debug();
        app.step_once().expect("step succeeds");

        app.enable_debug();

        assert_eq!(app.debug().expect("debug enabled").trace().len(), 1);
    }

    #[test]
    fn superchip_exit_halts_the_application() {
        let mut app =
            App::new(vec![0x00, 0xFD], false, CompatibilityProfile::SuperChip).expect("valid ROM");
        assert!(app.advance(Duration::from_millis(10)).expect("valid ROM"));
        assert!(app.is_halted());
    }

    #[test]
    fn replacing_a_rom_reuses_and_resets_the_audio_state() {
        let audio = audio::shared_state();
        let mut rom = vec![0; 0xE10];
        rom[..12].copy_from_slice(&[
            0xF0, 0x00, 0x10, 0x00, // I = 0x1000
            0xF0, 0x02, // load the XO-CHIP audio pattern
            0x60, 0x0A, // V0 = ten timer ticks
            0xF0, 0x18, // start the sound timer
            0x12, 0x0A, // loop
        ]);
        rom[0xE00..].copy_from_slice(&[
            0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        let mut sounding =
            App::with_audio_state(rom, false, CompatibilityProfile::XoChip, Arc::clone(&audio))
                .expect("valid XO-CHIP ROM");
        sounding
            .advance(Duration::from_millis(20))
            .expect("runs ROM");

        let snapshot = audio.snapshot();
        assert!(snapshot.active);
        assert_eq!(
            snapshot.pattern[..8],
            [0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA]
        );

        let _replacement = App::with_audio_state(
            vec![],
            false,
            CompatibilityProfile::OriginalChip8,
            audio.clone(),
        )
        .expect("valid replacement ROM");

        assert!(!audio.snapshot().active);
    }
}
