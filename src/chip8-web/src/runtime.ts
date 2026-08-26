import type { Profile } from "./catalogue";
import { loadWasm, type WasmBindings, type WasmChip8 } from "./wasm-loader";
export const DEFAULT_CPU_HZ = 700, MIN_CPU_HZ = 60, MAX_CPU_HZ = 2000;
export type TraceEntry = { pc: number; opcode: number; mnemonic: string; analysis: number; response?: number };
export type RuntimeState = { frame: Uint8Array; width: number; height: number; halted: boolean; trace: TraceEntry[]; lastKey?: string };

export class Chip8Runtime {
  private constructor(private readonly wasm: WasmBindings) {}
  private chip!: WasmChip8; private rom = new Uint8Array(); private profile: Profile = "chip8";
  private cpuHz = DEFAULT_CPU_HZ; private remainder = 0; private paused = false; private halted = false;
  private debug = false; private breakpoints = new Set<number>(); private skipBreakpoint?: number; private trace: TraceEntry[] = [];
  private lastFrame = 0; private animation?: number; private readonly listeners = new Set<(state: RuntimeState) => void>();
  private audio?: AudioContext; private source?: AudioBufferSourceNode; private gain?: GainNode; private audioSignature = "";

  static async create() { const loaded = await loadWasm(); await loaded.bindings.default(loaded.binaryUrl); return new Chip8Runtime(loaded.bindings); }
  subscribe(listener: (state: RuntimeState) => void) { this.listeners.add(listener); return () => this.listeners.delete(listener); }
  async load(rom: Uint8Array, profile: Profile) { this.rom = rom.slice(); this.profile = profile; this.chip = this.wasm.WasmChip8.with_profile(this.wasm.WasmCompatibilityProfile[profileName(profile)], 0xc8082024); this.chip.load_rom(this.rom); this.resetHost(); this.publish(); }
  setProfile(profile: Profile) { return this.load(this.rom, profile); }
  setCpuHz(value: number) { this.cpuHz = Math.min(MAX_CPU_HZ, Math.max(MIN_CPU_HZ, value)); }
  getCpuHz() { return this.cpuHz; } getProfile() { return this.profile; } isPaused() { return this.paused; } isDebug() { return this.debug; }
  enableDebug() { this.debug = true; this.trace = []; this.publish(); }
  togglePause() { if (this.paused) this.skipBreakpoint = this.chip.program_counter(); this.paused = !this.paused; this.remainder = 0; this.publish(); }
  restart() { return this.load(this.rom, this.profile); }
  toggleBreakpoint(pc: number) { this.breakpoints.has(pc) ? this.breakpoints.delete(pc) : this.breakpoints.add(pc); this.publish(); }
  clearBreakpoints() { this.breakpoints.clear(); this.publish(); }
  step() { if (!this.debug || this.halted) return; this.paused = true; this.execute(); this.publish(); }
  setKey(key: number, pressed: boolean, physical?: string) { this.chip.set_key(key, pressed); if (pressed && physical) { this.lastKey = `${physical} → CHIP-8 ${key.toString(16).toUpperCase()}`; this.publish(); } }
  private lastKey?: string;
  start() { this.lastFrame = performance.now(); const tick = (now: number) => { const elapsed = Math.min(100, now - this.lastFrame); this.lastFrame = now; this.advance(elapsed); this.animation = requestAnimationFrame(tick); }; this.animation = requestAnimationFrame(tick); }
  stop() { if (this.animation) cancelAnimationFrame(this.animation); this.animation = undefined; this.stopAudio(); }
  private advance(ms: number) { if (!this.chip || this.halted || this.paused) return; this.chip.advance_time_ms(ms); this.remainder += ms * this.cpuHz / 1000; while (this.remainder >= 1) { const pc = this.chip.program_counter(); if (this.debug && this.breakpoints.has(pc) && this.skipBreakpoint !== pc) { this.paused = true; this.remainder = 0; break; } this.skipBreakpoint = undefined; this.remainder -= 1; const result = this.execute(); if (result?.waiting_for_key || result?.waiting_for_vblank || result?.halted) break; } this.updateAudio(); this.publish(); }
  private execute() { const pc = this.chip.program_counter(); const opcode = this.chip.current_opcode(); const started = performance.now(); const result = this.chip.step_with_result(); if (this.debug && opcode !== undefined) { this.trace.push({ pc, opcode, mnemonic: disassemble(opcode), analysis: performance.now() - started }); if (this.trace.length > 1000) this.trace.shift(); } this.halted ||= result.halted; return result; }
  private publish() { if (!this.chip) return; const state = { frame: this.chip.framebuffer(), width: this.chip.display_width(), height: this.chip.display_height(), halted: this.halted, trace: [...this.trace], lastKey: this.lastKey }; this.listeners.forEach(listener => listener(state)); }
  private resetHost() { this.remainder = 0; this.paused = false; this.halted = false; this.trace = []; this.skipBreakpoint = undefined; }
  async resumeAudio() { if (!this.audio) { this.audio = new AudioContext(); this.gain = this.audio.createGain(); this.gain.gain.value = 0; this.gain.connect(this.audio.destination); } await this.audio.resume(); }
  private updateAudio() {
    if (!this.audio || !this.gain) return;
    const pattern = this.chip.audio_pattern(); const signature = Array.from(pattern).join(",");
    if (signature !== this.audioSignature) { this.source?.stop(); this.source?.disconnect(); const buffer = this.audio.createBuffer(1, 128, this.audio.sampleRate); const samples = buffer.getChannelData(0); for (let index = 0; index < 128; index += 1) samples[index] = pattern[Math.floor(index / 8)] & (0x80 >> (index % 8)) ? .20 : -.20; this.source = this.audio.createBufferSource(); this.source.buffer = buffer; this.source.loop = true; this.source.connect(this.gain); this.source.start(); this.audioSignature = signature; }
    const frequency = 4000 * 2 ** ((this.chip.audio_pitch() - 64) / 48); this.source!.playbackRate.value = frequency * 128 / this.audio.sampleRate; this.gain.gain.value = this.chip.sound_active() ? 1 : 0;
  }
  private stopAudio() { this.source?.stop(); this.source?.disconnect(); this.gain?.disconnect(); void this.audio?.close(); this.audio = undefined; this.gain = undefined; this.source = undefined; this.audioSignature = ""; }
}

export const KEYMAP: Record<string, number> = { Digit1: 1, Digit2: 2, Digit3: 3, Digit4: 12, KeyQ: 4, KeyW: 5, KeyE: 6, KeyR: 13, KeyA: 7, KeyS: 8, KeyD: 9, KeyF: 14, KeyZ: 10, KeyX: 0, KeyC: 11, KeyV: 15 };
export function disassemble(opcode: number) {
  if (opcode >>> 16 === 0xf000) return `LD I, 0x${(opcode & 0xffff).toString(16).padStart(4, "0").toUpperCase()}`;
  const op = opcode & 0xffff, x = (op >> 8) & 15, y = (op >> 4) & 15, kk = op & 255, n = op & 15, nnn = op & 0xfff, hex = (value: number, digits: number) => `0x${value.toString(16).padStart(digits, "0").toUpperCase()}`;
  if (op === 0x00e0) return "CLS"; if (op === 0x00ee) return "RET"; if (op === 0x00fb) return "SCR"; if (op === 0x00fc) return "SCL"; if (op === 0x00fd) return "EXIT"; if (op === 0x00fe) return "LOW"; if (op === 0x00ff) return "HIGH";
  if ((op & 0xfff0) === 0x00c0) return `SCD ${n}`;
  switch (op & 0xf000) {
    case 0x0000: return `SYS ${hex(nnn, 3)}`; case 0x1000: return `JP ${hex(nnn, 3)}`; case 0x2000: return `CALL ${hex(nnn, 3)}`;
    case 0x3000: return `SE V${x.toString(16).toUpperCase()}, ${hex(kk, 2)}`; case 0x4000: return `SNE V${x.toString(16).toUpperCase()}, ${hex(kk, 2)}`;
    case 0x5000: return n === 0 ? `SE V${x.toString(16).toUpperCase()}, V${y.toString(16).toUpperCase()}` : invalid(op);
    case 0x6000: return `LD V${x.toString(16).toUpperCase()}, ${hex(kk, 2)}`; case 0x7000: return `ADD V${x.toString(16).toUpperCase()}, ${hex(kk, 2)}`;
    case 0x8000: return ["LD", "OR", "AND", "XOR", "ADD", "SUB", "SHR", "SUBN", "", "", "", "", "", "", "SHL"][n] ? (n === 6 || n === 14 ? `${["", "", "", "", "", "", "SHR", "", "", "", "", "", "", "", "SHL"][n]} V${x.toString(16).toUpperCase()}` : `${["LD", "OR", "AND", "XOR", "ADD", "SUB", "", "SUBN"][n]} V${x.toString(16).toUpperCase()}, V${y.toString(16).toUpperCase()}`) : invalid(op);
    case 0x9000: return n === 0 ? `SNE V${x.toString(16).toUpperCase()}, V${y.toString(16).toUpperCase()}` : invalid(op); case 0xa000: return `LD I, ${hex(nnn, 3)}`; case 0xb000: return `JP V0, ${hex(nnn, 3)}`; case 0xc000: return `RND V${x.toString(16).toUpperCase()}, ${hex(kk, 2)}`; case 0xd000: return `DRW V${x.toString(16).toUpperCase()}, V${y.toString(16).toUpperCase()}, ${n}`;
    case 0xe000: return kk === 0x9e ? `SKP V${x.toString(16).toUpperCase()}` : kk === 0xa1 ? `SKNP V${x.toString(16).toUpperCase()}` : invalid(op);
    case 0xf000: { const names: Record<number, string> = { 7: "LD Vx, DT", 0x0a: "LD Vx, K", 0x15: "LD DT, Vx", 0x18: "LD ST, Vx", 0x1e: "ADD I, Vx", 0x29: "LD F, Vx", 0x30: "LD HF, Vx", 0x33: "LD B, Vx", 0x55: "LD [I], V0..Vx", 0x65: "LD V0..Vx, [I]", 0x75: "LD RPL, V0..Vx", 0x85: "LD V0..Vx, RPL" }; return names[kk] ? names[kk].replaceAll("x", x.toString(16).toUpperCase()) : invalid(op); }
    default: return invalid(op);
  }
}
function invalid(opcode: number) { return `INVALID 0x${opcode.toString(16).padStart(4, "0").toUpperCase()}`; }
function profileName(profile: Profile) { return ({ chip8: "OriginalChip8", chip48: "Chip48", modern: "Modern", schip10: "SuperChip10", schip11: "SuperChip11", schipc: "SuperChipCompatibility", "schip-modern": "SuperChipModern", superchip: "SuperChip", xochip: "XoChip" } as const)[profile]; }
