use crate::machine::{Chip8, Chip8Error, CycleResult, RAM_SIZE};

pub(crate) fn step(chip: &mut Chip8) -> Result<CycleResult, Chip8Error> {
    let pc = chip.pc;
    let high = chip
        .read(pc)
        .map_err(|_| Chip8Error::ProgramCounterOutOfBounds { pc })?;
    let low = chip
        .read(
            pc.checked_add(1)
                .ok_or(Chip8Error::ProgramCounterOutOfBounds { pc })?,
        )
        .map_err(|_| Chip8Error::ProgramCounterOutOfBounds { pc })?;
    let opcode = u16::from_be_bytes([high, low]);
    let nnn = opcode & 0x0FFF;
    let x = usize::from((opcode >> 8) & 0x0F);
    let y = usize::from((opcode >> 4) & 0x0F);
    let kk = opcode as u8;
    let n = kk & 0x0F;
    let mut result = CycleResult::default();
    let mut next_pc = pc
        .checked_add(2)
        .ok_or(Chip8Error::ProgramCounterOutOfBounds { pc })?;

    match opcode & 0xF000 {
        0x0000 => match opcode {
            0x00E0 => chip.display.clear(),
            0x00EE => {
                if chip.sp == 0 {
                    return Err(Chip8Error::StackUnderflow);
                }
                chip.sp -= 1;
                next_pc = chip.stack[chip.sp];
            }
            _ => {} // 0NNN is a historical RCA 1802 call, ignored by modern interpreters.
        },
        0x1000 => next_pc = nnn,
        0x2000 => {
            if chip.sp == chip.stack.len() {
                return Err(Chip8Error::StackOverflow);
            }
            chip.stack[chip.sp] = next_pc;
            chip.sp += 1;
            next_pc = nnn;
        }
        0x3000 => {
            if chip.v[x] == kk {
                next_pc = skip(next_pc)?;
            }
        }
        0x4000 => {
            if chip.v[x] != kk {
                next_pc = skip(next_pc)?;
            }
        }
        0x5000 if n == 0 => {
            if chip.v[x] == chip.v[y] {
                next_pc = skip(next_pc)?;
            }
        }
        0x6000 => chip.v[x] = kk,
        0x7000 => chip.v[x] = chip.v[x].wrapping_add(kk),
        0x8000 => execute_8xy(chip, opcode, x, y, pc)?,
        0x9000 if n == 0 => {
            if chip.v[x] != chip.v[y] {
                next_pc = skip(next_pc)?;
            }
        }
        0xA000 => chip.i = nnn,
        0xB000 => {
            next_pc = if chip.config.profile.jump_uses_vx() {
                u16::from(chip.v[x]) + u16::from(kk)
            } else {
                nnn + u16::from(chip.v[0])
            };
        }
        0xC000 => chip.v[x] = chip.next_random() & kk,
        0xD000 => {
            let end = chip
                .i
                .checked_add(u16::from(n))
                .ok_or(Chip8Error::MemoryOutOfBounds { address: chip.i })?;
            if usize::from(end) > RAM_SIZE {
                return Err(Chip8Error::MemoryOutOfBounds { address: end });
            }
            let mut sprite = [0u8; 15];
            for (offset, byte) in sprite.iter_mut().take(usize::from(n)).enumerate() {
                *byte = chip.read(chip.i + offset as u16)?;
            }
            chip.v[0xF] = u8::from(chip.display.draw(
                chip.v[x],
                chip.v[y],
                &sprite[..usize::from(n)],
                chip.config.profile.draw_wraps(),
            ));
            result.drew = true;
        }
        0xE000 => match kk {
            0x9E => {
                if key_pressed(chip, x)? {
                    next_pc = skip(next_pc)?;
                }
            }
            0xA1 => {
                if !key_pressed(chip, x)? {
                    next_pc = skip(next_pc)?;
                }
            }
            _ => return invalid(opcode, pc),
        },
        0xF000 => match kk {
            0x07 => chip.v[x] = chip.delay_timer,
            0x0A => match chip.keys.iter().position(|pressed| *pressed) {
                Some(key) => chip.v[x] = key as u8,
                None => {
                    result.waiting_for_key = true;
                    next_pc = pc;
                }
            },
            0x15 => chip.delay_timer = chip.v[x],
            0x18 => chip.sound_timer = chip.v[x],
            0x1E => chip.i = chip.i.wrapping_add(u16::from(chip.v[x])),
            0x29 => chip.i = crate::machine::FONT_START + 5 * u16::from(chip.v[x] & 0x0F),
            0x33 => {
                let value = chip.v[x];
                chip.write(chip.i, value / 100)?;
                chip.write(
                    chip.i
                        .checked_add(1)
                        .ok_or(Chip8Error::MemoryOutOfBounds { address: chip.i })?,
                    (value / 10) % 10,
                )?;
                chip.write(
                    chip.i
                        .checked_add(2)
                        .ok_or(Chip8Error::MemoryOutOfBounds { address: chip.i })?,
                    value % 10,
                )?;
            }
            0x55 => store_registers(chip, x)?,
            0x65 => load_registers(chip, x)?,
            _ => return invalid(opcode, pc),
        },
        _ => return invalid(opcode, pc),
    }
    chip.pc = next_pc;
    Ok(result)
}

fn execute_8xy(
    chip: &mut Chip8,
    opcode: u16,
    x: usize,
    y: usize,
    pc: u16,
) -> Result<(), Chip8Error> {
    match opcode & 0x000F {
        0x0 => chip.v[x] = chip.v[y],
        0x1 => {
            chip.v[x] |= chip.v[y];
            clear_vf_for_logic(chip);
        }
        0x2 => {
            chip.v[x] &= chip.v[y];
            clear_vf_for_logic(chip);
        }
        0x3 => {
            chip.v[x] ^= chip.v[y];
            clear_vf_for_logic(chip);
        }
        0x4 => {
            let (value, carry) = chip.v[x].overflowing_add(chip.v[y]);
            chip.v[x] = value;
            chip.v[0xF] = u8::from(carry);
        }
        0x5 => {
            let (value, borrow) = chip.v[x].overflowing_sub(chip.v[y]);
            chip.v[x] = value;
            chip.v[0xF] = u8::from(!borrow);
        }
        0x6 => {
            let source = if chip.config.profile.shift_uses_vy() {
                chip.v[y]
            } else {
                chip.v[x]
            };
            chip.v[x] = source >> 1;
            chip.v[0xF] = source & 1;
        }
        0x7 => {
            let (value, borrow) = chip.v[y].overflowing_sub(chip.v[x]);
            chip.v[x] = value;
            chip.v[0xF] = u8::from(!borrow);
        }
        0xE => {
            let source = if chip.config.profile.shift_uses_vy() {
                chip.v[y]
            } else {
                chip.v[x]
            };
            chip.v[x] = source << 1;
            chip.v[0xF] = source >> 7;
        }
        _ => return invalid(opcode, pc),
    }
    Ok(())
}

fn clear_vf_for_logic(chip: &mut Chip8) {
    if chip.config.profile.logic_clears_vf() {
        chip.v[0xF] = 0;
    }
}
fn key_pressed(chip: &Chip8, x: usize) -> Result<bool, Chip8Error> {
    chip.keys
        .get(usize::from(chip.v[x]))
        .copied()
        .ok_or(Chip8Error::InvalidKey { key: chip.v[x] })
}
fn skip(pc: u16) -> Result<u16, Chip8Error> {
    pc.checked_add(2)
        .ok_or(Chip8Error::ProgramCounterOutOfBounds { pc })
}
fn invalid<T>(opcode: u16, pc: u16) -> Result<T, Chip8Error> {
    Err(Chip8Error::InvalidOpcode { opcode, pc })
}

fn store_registers(chip: &mut Chip8, x: usize) -> Result<(), Chip8Error> {
    for offset in 0..=x {
        chip.write(
            chip.i
                .checked_add(offset as u16)
                .ok_or(Chip8Error::MemoryOutOfBounds { address: chip.i })?,
            chip.v[offset],
        )?;
    }
    if chip.config.profile.increment_i_after_store_load() {
        chip.i = chip.i.wrapping_add(x as u16 + 1);
    }
    Ok(())
}
fn load_registers(chip: &mut Chip8, x: usize) -> Result<(), Chip8Error> {
    for offset in 0..=x {
        let address = chip
            .i
            .checked_add(offset as u16)
            .ok_or(Chip8Error::MemoryOutOfBounds { address: chip.i })?;
        chip.v[offset] = chip.read(address)?;
    }
    if chip.config.profile.increment_i_after_store_load() {
        chip.i = chip.i.wrapping_add(x as u16 + 1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::{Chip8Config, CompatibilityProfile};

    fn chip(opcodes: &[u16]) -> Chip8 {
        let mut c = Chip8::default();
        let bytes: Vec<_> = opcodes.iter().flat_map(|op| op.to_be_bytes()).collect();
        c.load_rom(&bytes).expect("valid ROM");
        c
    }
    fn run(c: &mut Chip8, count: usize) {
        for _ in 0..count {
            c.step().expect("valid instruction");
        }
    }

    #[test]
    fn arithmetic_and_flags_follow_chip8_rules() {
        let mut c = chip(&[0x60FE, 0x6102, 0x8014, 0x8015, 0x8017]);
        run(&mut c, 3);
        assert_eq!((c.v[0], c.v[0xF]), (0, 1));
        c.v[1] = 1;
        run(&mut c, 1);
        assert_eq!((c.v[0], c.v[0xF]), (255, 0));
        run(&mut c, 1);
        assert_eq!((c.v[0], c.v[0xF]), (2, 0));
    }
    #[test]
    fn call_return_and_skip_work() {
        let mut c = chip(&[0x2206, 0x6001, 0x120A, 0x6102, 0x00EE, 0x3000, 0x6203]);
        run(&mut c, 7);
        assert_eq!((c.v[0], c.v[1], c.v[2]), (1, 2, 3));
    }
    #[test]
    fn bcd_font_and_memory_transfer_work() {
        let mut c = chip(&[
            0x60E7, 0xA300, 0xF033, 0xF029, 0xA310, 0xF055, 0x6000, 0xA310, 0xF065,
        ]);
        run(&mut c, 3);
        assert_eq!(&c.memory[0x300..0x303], &[2, 3, 1]);
        run(&mut c, 1);
        assert_eq!(c.i, 0x50 + 5 * 7);
        run(&mut c, 5);
        assert_eq!(c.v[0], 0xE7);
    }
    #[test]
    fn wait_for_key_and_key_skips_work() {
        let mut c = chip(&[0xF00A, 0xE09E, 0x6101]);
        assert!(c.step().expect("wait").waiting_for_key);
        assert_eq!(c.pc, 0x200);
        c.set_key(5, true).expect("valid key");
        c.step().expect("key");
        assert_eq!(c.v[0], 5);
        c.step().expect("skip");
        assert_eq!(c.pc, 0x206);
    }
    #[test]
    fn random_is_seeded() {
        let config = Chip8Config {
            seed: 42,
            ..Chip8Config::default()
        };
        let mut a = Chip8::new(config);
        let mut b = Chip8::new(config);
        a.load_rom(&[0xC0, 0xFF]).expect("rom");
        b.load_rom(&[0xC0, 0xFF]).expect("rom");
        a.step().expect("random");
        b.step().expect("random");
        assert_eq!(a.v[0], b.v[0]);
    }
    #[test]
    fn profiles_centralize_quirks() {
        let mut original = Chip8::default();
        original
            .load_rom(&[0x60, 1, 0x61, 4, 0x80, 0x16, 0xA3, 0x00, 0xF1, 0x55])
            .expect("rom");
        run(&mut original, 5);
        assert_eq!((original.v[0], original.i), (2, 0x302));
        let config = Chip8Config {
            profile: CompatibilityProfile::Chip48,
            ..Chip8Config::default()
        };
        let mut chip48 = Chip8::new(config);
        chip48
            .load_rom(&[0x60, 1, 0x61, 4, 0x80, 0x16, 0xA3, 0x00, 0xF1, 0x55])
            .expect("rom");
        run(&mut chip48, 5);
        assert_eq!((chip48.v[0], chip48.i), (0, 0x300));
    }
    #[test]
    fn profiles_cover_logic_jump_and_drawing_quirks() {
        let mut original = chip(&[0x60FF, 0x6101, 0x6F09, 0x8011]);
        run(&mut original, 4);
        assert_eq!(original.v[0xF], 0);

        let config = Chip8Config {
            profile: CompatibilityProfile::Modern,
            ..Chip8Config::default()
        };
        let mut modern = Chip8::new(config);
        modern
            .load_rom(&[0x60, 0xFF, 0x61, 1, 0x6F, 9, 0x80, 0x11])
            .expect("rom");
        run(&mut modern, 4);
        assert_eq!(modern.v[0xF], 9);

        let config = Chip8Config {
            profile: CompatibilityProfile::Chip48,
            ..Chip8Config::default()
        };
        let mut chip48 = Chip8::new(config);
        chip48.load_rom(&[0x61, 2, 0xB1, 0x23]).expect("rom");
        run(&mut chip48, 2);
        assert_eq!(chip48.pc, 0x25);

        let mut wrapped = Chip8::default();
        wrapped
            .load_rom(&[0xA2, 0x10, 0x60, 63, 0x61, 31, 0xD0, 0x11])
            .expect("rom");
        wrapped.memory[0x210] = 0xC0;
        run(&mut wrapped, 4);
        assert_eq!(wrapped.display.pixels()[31 * 64 + 63], 1);
        assert_eq!(wrapped.display.pixels()[31 * 64], 1);
    }
    #[test]
    fn invalid_opcode_is_explicit() {
        let mut c = chip(&[0x5123]);
        assert!(matches!(
            c.step(),
            Err(Chip8Error::InvalidOpcode { opcode: 0x5123, .. })
        ));
    }
}
