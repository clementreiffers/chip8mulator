import { describe, expect, it } from "vitest";
import { wasmBinaryUrl, wasmModuleUrl } from "./wasm-loader";

describe("wasmModuleUrl", () => {
  it("uses the local binding while Vite is serving the application", () => {
    expect(wasmModuleUrl(true)).toBe("/wasm/chip8_engine.js");
  });

  it("uses the stable latest-release binding for production", () => {
    expect(wasmModuleUrl(false)).toContain("releases/latest/download/chip8_engine.js");
    expect(wasmBinaryUrl(false)).toContain("releases/latest/download/chip8_engine_bg.wasm");
  });
});
