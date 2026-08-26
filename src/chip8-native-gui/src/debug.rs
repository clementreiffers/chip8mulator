use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub const TRACE_CAPACITY: usize = 1_000;

#[derive(Debug, Clone)]
pub struct TraceEntry {
    pub pc: u16,
    pub opcode: u16,
    pub mnemonic: String,
    pub analysis_time: Duration,
    pub response_time: Option<Duration>,
    executed_at: Instant,
}

impl TraceEntry {
    pub fn new(pc: u16, opcode: u16, analysis_time: Duration) -> Self {
        Self {
            pc,
            opcode,
            mnemonic: disassemble(opcode),
            analysis_time,
            response_time: None,
            executed_at: Instant::now(),
        }
    }
}

#[derive(Debug, Default)]
pub struct DebugState {
    trace: VecDeque<TraceEntry>,
    breakpoints: Vec<u16>,
}

impl DebugState {
    pub fn clear_trace(&mut self) {
        self.trace.clear();
    }

    pub fn record(&mut self, entry: TraceEntry) {
        if self.trace.len() == TRACE_CAPACITY {
            self.trace.pop_front();
        }
        self.trace.push_back(entry);
    }

    pub fn trace(&self) -> &VecDeque<TraceEntry> {
        &self.trace
    }

    pub fn is_breakpoint(&self, pc: u16) -> bool {
        self.breakpoints.binary_search(&pc).is_ok()
    }

    pub fn toggle_breakpoint(&mut self, pc: u16) {
        match self.breakpoints.binary_search(&pc) {
            Ok(index) => {
                self.breakpoints.remove(index);
            }
            Err(index) => self.breakpoints.insert(index, pc),
        }
    }

    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }

    pub fn mark_presented(&mut self, presented_at: Instant) {
        for entry in self.trace.iter_mut().rev() {
            if entry.response_time.is_some() {
                break;
            }
            entry.response_time = Some(presented_at.duration_since(entry.executed_at));
        }
    }
}

#[must_use]
pub fn disassemble(opcode: u16) -> String {
    let nnn = opcode & 0x0FFF;
    let x = (opcode >> 8) & 0x000F;
    let y = (opcode >> 4) & 0x000F;
    let kk = opcode as u8;
    let n = kk & 0x0F;
    match opcode {
        0x00E0 => "CLS".into(),
        0x00EE => "RET".into(),
        _ => match opcode & 0xF000 {
            0x0000 => format!("SYS {nnn:#05X}"),
            0x1000 => format!("JP {nnn:#05X}"),
            0x2000 => format!("CALL {nnn:#05X}"),
            0x3000 => format!("SE V{x:X}, {kk:#04X}"),
            0x4000 => format!("SNE V{x:X}, {kk:#04X}"),
            0x5000 if n == 0 => format!("SE V{x:X}, V{y:X}"),
            0x6000 => format!("LD V{x:X}, {kk:#04X}"),
            0x7000 => format!("ADD V{x:X}, {kk:#04X}"),
            0x8000 => match n {
                0 => format!("LD V{x:X}, V{y:X}"),
                1 => format!("OR V{x:X}, V{y:X}"),
                2 => format!("AND V{x:X}, V{y:X}"),
                3 => format!("XOR V{x:X}, V{y:X}"),
                4 => format!("ADD V{x:X}, V{y:X}"),
                5 => format!("SUB V{x:X}, V{y:X}"),
                6 => format!("SHR V{x:X}"),
                7 => format!("SUBN V{x:X}, V{y:X}"),
                0xE => format!("SHL V{x:X}"),
                _ => invalid(opcode),
            },
            0x9000 if n == 0 => format!("SNE V{x:X}, V{y:X}"),
            0xA000 => format!("LD I, {nnn:#05X}"),
            0xB000 => format!("JP V0, {nnn:#05X}"),
            0xC000 => format!("RND V{x:X}, {kk:#04X}"),
            0xD000 => format!("DRW V{x:X}, V{y:X}, {n}"),
            0xE000 => match kk {
                0x9E => format!("SKP V{x:X}"),
                0xA1 => format!("SKNP V{x:X}"),
                _ => invalid(opcode),
            },
            0xF000 => match kk {
                0x07 => format!("LD V{x:X}, DT"),
                0x0A => format!("LD V{x:X}, K"),
                0x15 => format!("LD DT, V{x:X}"),
                0x18 => format!("LD ST, V{x:X}"),
                0x1E => format!("ADD I, V{x:X}"),
                0x29 => format!("LD F, V{x:X}"),
                0x33 => format!("LD B, V{x:X}"),
                0x55 => format!("LD [I], V0..V{x:X}"),
                0x65 => format!("LD V0..V{x:X}, [I]"),
                _ => invalid(opcode),
            },
            _ => invalid(opcode),
        },
    }
}

fn invalid(opcode: u16) -> String {
    format!("INVALID {opcode:#06X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disassembles_representative_opcodes() {
        assert_eq!(disassemble(0x60AB), "LD V0, 0xAB");
        assert_eq!(disassemble(0xD125), "DRW V1, V2, 5");
        assert_eq!(disassemble(0x5123), "INVALID 0x5123");
    }

    #[test]
    fn trace_is_bounded_and_breakpoints_are_toggleable() {
        let mut state = DebugState::default();
        state.toggle_breakpoint(0x200);
        assert!(state.is_breakpoint(0x200));
        state.toggle_breakpoint(0x200);
        assert!(!state.is_breakpoint(0x200));
        for pc in 0..=TRACE_CAPACITY {
            state.record(TraceEntry {
                pc: pc as u16,
                opcode: 0,
                mnemonic: String::new(),
                analysis_time: Duration::ZERO,
                response_time: None,
                executed_at: Instant::now(),
            });
        }
        assert_eq!(state.trace().len(), TRACE_CAPACITY);
        assert_eq!(state.trace().front().expect("trace entry").pc, 1);
    }
}
