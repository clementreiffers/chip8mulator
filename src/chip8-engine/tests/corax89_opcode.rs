use std::{env, fs, time::Duration};

use chip8_engine::Chip8;

const PROGRAM_START: usize = 0x200;
const EXECUTION_CYCLES: usize = 20_000;

#[test]
fn corax89_opcode_rom_loads_completely_and_executes() {
    let Ok(path) = env::var("CHIP8_CONFORMANCE_ROM") else {
        eprintln!("skipping: run `task test-conformance` to supply the ROM");
        return;
    };
    let rom = fs::read(&path).expect("read downloaded conformance ROM");
    assert_eq!(rom.len(), 478, "unexpected corax89 ROM size");

    let mut chip = Chip8::default();
    chip.load_rom(&rom).expect("ROM fits in CHIP-8 RAM");
    assert_eq!(
        &chip.memory()[PROGRAM_START..PROGRAM_START + rom.len()],
        rom.as_slice(),
        "every ROM byte must be copied into program memory"
    );

    for _ in 0..EXECUTION_CYCLES {
        chip.step()
            .expect("conformance ROM must execute valid CHIP-8 opcodes");
        chip.advance_timers(Duration::from_millis(1));
    }

    assert!(
        chip.framebuffer().iter().any(|pixel| *pixel != 0),
        "conformance ROM should render its result"
    );
    eprintln!(
        "corax89 completed {EXECUTION_CYCLES} cycles; pc={:#05x}",
        chip.program_counter()
    );
}
