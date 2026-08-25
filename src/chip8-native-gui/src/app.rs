use std::time::Duration;

use chip8_engine::{Chip8, Chip8Config, Chip8Error, CompatibilityProfile, DISPLAY_PIXELS};
use winit::keyboard::KeyCode;

const CPU_HZ: u32 = 700;
const MAX_ELAPSED: Duration = Duration::from_millis(100);

pub struct App {
    chip8: Chip8,
    rom: Vec<u8>,
    profile: CompatibilityProfile,
    cpu_remainder: Duration,
    paused: bool,
    frame_dirty: bool,
}

impl App {
    pub fn new(rom: Vec<u8>) -> Result<Self, Chip8Error> {
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
                self.paused = !self.paused;
                true
            }
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
            self.cpu_remainder -= cycle_period;
            let result = self.chip8.step()?;
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

    fn restart(&mut self) -> Result<(), Chip8Error> {
        self.chip8.load_rom(&self.rom)?;
        self.cpu_remainder = Duration::ZERO;
        self.frame_dirty = true;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_prevents_cpu_execution() {
        let mut app = App::new(vec![0x60, 0x01]).expect("valid ROM");
        app.handle_command(KeyCode::Space);
        app.advance(Duration::from_millis(10)).expect("valid ROM");
        assert_eq!(app.chip8.registers()[0], 0);
    }

    #[test]
    fn profile_command_restarts_the_machine() {
        let mut app = App::new(vec![0x60, 0x01]).expect("valid ROM");
        assert!(app.handle_command(KeyCode::F2));
        assert_eq!(app.profile, CompatibilityProfile::Chip48);
        assert_eq!(app.chip8.program_counter(), 0x200);
    }
}
