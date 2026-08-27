export interface WasmCycleResult {
  readonly drew: boolean;
  readonly waiting_for_key: boolean;
  readonly waiting_for_vblank: boolean;
  readonly halted: boolean;
}

export interface WasmChip8 {
  load_rom(rom: Uint8Array): void;
  step_with_result(): WasmCycleResult;
  advance_time_ms(milliseconds: number): number;
  set_key(key: number, pressed: boolean): void;
  framebuffer(): Uint8Array;
  display_width(): number;
  display_height(): number;
  sound_active(): boolean;
  audio_pattern(): Uint8Array;
  audio_pitch(): number;
  program_counter(): number;
  current_opcode(): number | undefined;
}

export interface WasmBindings {
  default(input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<unknown>;
  WasmChip8: { with_profile(profile: number, seed?: number): WasmChip8 };
  WasmCompatibilityProfile: Record<string, number>;
}

const RELEASE_WASM_MODULE =
  "https://github.com/clementreiffers/chip8mulator/releases/latest/download/chip8_engine.js";
const RELEASE_WASM_BINARY =
  "https://github.com/clementreiffers/chip8mulator/releases/latest/download/chip8_engine_bg.wasm";
const LOCAL_WASM_MODULE = "/wasm/chip8_engine.js";
const LOCAL_WASM_BINARY = "/wasm/chip8_engine_bg.wasm";

export const wasmModuleUrl = (local: boolean): string =>
  local ? LOCAL_WASM_MODULE : RELEASE_WASM_MODULE;
export const wasmBinaryUrl = (local: boolean): string =>
  local ? LOCAL_WASM_BINARY : RELEASE_WASM_BINARY;

export interface LoadedWasm {
  bindings: WasmBindings;
  binaryUrl: string;
}

export async function loadWasm(): Promise<LoadedWasm> {
  const local = import.meta.env.DEV;
  const moduleUrl = wasmModuleUrl(local);
  const binaryUrl = wasmBinaryUrl(local);
  const response = await fetch(moduleUrl);
  if (!response.ok) throw new Error(`WASM bindings download failed: ${response.status}`);
  const source = await response.text();
  const blobUrl = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  try {
    return { bindings: await import(/* @vite-ignore */ blobUrl) as WasmBindings, binaryUrl };
  } finally {
    URL.revokeObjectURL(blobUrl);
  }
}
