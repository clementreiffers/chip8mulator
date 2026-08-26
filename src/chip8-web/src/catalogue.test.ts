import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchGames } from "./catalogue";

describe("fetchGames", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("lists binary ROMs and excludes Octo source files", async () => {
    const response = (body: unknown) => ({ ok: true, json: async () => body });
    vi.stubGlobal("fetch", vi.fn((url: string) => {
      if (url.includes("programs.json")) return Promise.resolve(response({ "Colour Demo": { platform: "xochip", options: { backgroundColor: "#000000", fillColor: "#111111", fillColor2: "#222222", blendColor: "#333333" } } }));
      if (url.includes("chip8Archive")) return Promise.resolve(response({ tree: [{ type: "blob", path: "roms/Colour Demo.ch8" }, { type: "blob", path: "examples/source.8o" }] }));
      return Promise.resolve(response({ tree: [{ type: "blob", path: "roms/Basic.ch8" }] }));
    }));
    const { games } = await fetchGames();
    expect(games.map(game => game.name)).toEqual(["Basic", "Colour Demo"]);
    expect(games.find(game => game.name === "Colour Demo")).toMatchObject({ profile: "xochip", palette: ["#000000", "#111111", "#222222", "#333333"] });
  });
});
