use std::time::{Duration, Instant};

use chip8_engine::{Chip8, Chip8Config, Chip8Error, CompatibilityProfile, DISPLAY_PIXELS};
use winit::keyboard::KeyCode;

use crate::debug::{DebugState, TraceEntry};

const CPU_HZ: u32 = 700;
const MAX_ELAPSED: Duration = Duration::from_millis(100);

pub struct App {
    chip8: Chip8,
    rom: Vec<u8>,
    profile: CompatibilityProfile,
    cpu_remainder: Duration,
    paused: bool,
    frame_dirty: bool,
    debug: Option<DebugState>,
    skip_breakpoint_once: Option<u16>,
}

impl App {
    pub fn new(rom: Vec<u8>, debug_mode: bool) -> Result<Self, Chip8Error> {
        let profile = CompatibilityProfile::OriginalChip8;
        let mut app = Self {
            chip8: Chip8::new(Chip8Config {
                profile,
                seed: 0xC808_2024,
            }),
            rom,
            profile,
            cpu_remainder: Duration::ZERO,
            paused: false,
            frame_dirty: true,
            debug: debug_mode.then(DebugState::default),
            skip_breakpoint_once: None,
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
            _ => false,
        }
    }

    pub fn advance(&mut self, elapsed: Duration) -> Result<(), Chip8Error> {
        if self.paused {
            return Ok(());
        }

        let elapsed = elapsed.min(MAX_ELAPSED);
        self.chip8.advance_timers(elapsed);
        self.cpu_remainder = self.cpu_remainder.saturating_add(elapsed);
        let cycle_period = Duration::from_nanos(1_000_000_000 / u64::from(CPU_HZ));

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
            if result.waiting_for_key {
                break;
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn framebuffer(&self) -> &[u8; DISPLAY_PIXELS] {
        self.chip8.framebuffer()
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

    pub fn is_paused(&self) -> bool {
        self.paused
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
        self.skip_breakpoint_once = None;
        if let Some(debug) = &mut self.debug {
            debug.clear_trace();
        }
        Ok(())
    }

    fn set_profile(&mut self, profile: CompatibilityProfile) -> Result<(), Chip8Error> {
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
        result
    }

    fn opcode_at(&self, pc: u16) -> Option<u16> {
        let memory = self.chip8.memory();
        let high = *memory.get(usize::from(pc))?;
        let low = *memory.get(usize::from(pc.checked_add(1)?))?;
        Some(u16::from_be_bytes([high, low]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_prevents_cpu_execution() {
        let mut app = App::new(vec![0x60, 0x01], false).expect("valid ROM");
        app.handle_command(KeyCode::Space);
        app.advance(Duration::from_millis(10)).expect("valid ROM");
        assert_eq!(app.chip8.registers()[0], 0);
    }

    #[test]
    fn profile_command_restarts_the_machine() {
        let mut app = App::new(vec![0x60, 0x01], false).expect("valid ROM");
        assert!(app.handle_command(KeyCode::F2));
        assert_eq!(app.profile, CompatibilityProfile::Chip48);
        assert_eq!(app.chip8.program_counter(), 0x200);
    }

    #[test]
    fn breakpoint_stops_before_the_instruction_and_step_executes_it() {
        let mut app = App::new(vec![0x60, 0x01], true).expect("valid ROM");
        app.debug_mut()
            .expect("debug enabled")
            .toggle_breakpoint(0x200);
        app.advance(Duration::from_millis(10)).expect("valid ROM");
        assert!(app.is_paused());
        assert_eq!(app.chip8.program_counter(), 0x200);
        app.step_once().expect("step succeeds");
        assert_eq!(app.chip8.registers()[0], 1);
    }
}
