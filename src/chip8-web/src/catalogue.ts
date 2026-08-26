export type Profile = "chip8" | "chip48" | "modern" | "schip10" | "schip11" | "schipc" | "schip-modern" | "superchip" | "xochip";
export type Palette = [string, string, string, string];
export interface Game { name: string; source: string; url: string; profile: Profile; palette?: Palette; }

const SOURCES: Array<{ name: string; repository: string; revision: string; prefix: string; catalogue?: string }> = [
  { name: "CHIP-8 Archive — John Earnest", repository: "JohnEarnest/chip8Archive", revision: "master", prefix: "roms/", catalogue: "programs.json" },
  { name: "dmatlack/chip8", repository: "dmatlack/chip8", revision: "master", prefix: "roms/" },
];
type Catalogue = Record<string, { platform?: string; options?: { backgroundColor?: string; fillColor?: string; fillColor2?: string; blendColor?: string } }>;
type Tree = { tree: Array<{ type: string; path: string }> };

function title(path: string) { return path.split("/").pop()!.replace(/\.ch8$/i, "").replace(/[._-]+/g, " "); }
function profileFor(path: string, entry?: Catalogue[string]): Profile {
  const platform = entry?.platform?.toLowerCase();
  if (platform === "xochip") return "xochip";
  if (platform === "schip" || path.toLowerCase().includes("hires")) return "superchip";
  if (platform === "chip48") return "chip48";
  if (platform === "modern") return "modern";
  return "chip8";
}
function paletteFor(entry?: Catalogue[string]): Palette | undefined {
  const options = entry?.options;
  if (!options?.backgroundColor || !options.fillColor || !options.fillColor2 || !options.blendColor) return undefined;
  return [options.backgroundColor, options.fillColor, options.fillColor2, options.blendColor];
}

export async function fetchGames(): Promise<{ games: Game[]; errors: string[] }> {
  const results = await Promise.allSettled(SOURCES.map(async source => {
    const treeUrl = `https://api.github.com/repos/${source.repository}/git/trees/${source.revision}?recursive=1`;
    const [treeResponse, catalogueResponse] = await Promise.all([
      fetch(treeUrl), source.catalogue ? fetch(`https://raw.githubusercontent.com/${source.repository}/${source.revision}/${source.catalogue}`) : Promise.resolve(undefined),
    ]);
    if (!treeResponse.ok) throw new Error(`${source.name}: ${treeResponse.status}`);
    const tree = await treeResponse.json() as Tree;
    const catalogue = catalogueResponse?.ok ? await catalogueResponse.json() as Catalogue : {};
    return tree.tree.filter(entry => entry.type === "blob" && entry.path.startsWith(source.prefix) && entry.path.toLowerCase().endsWith(".ch8")).map(entry => {
      const name = title(entry.path); const catalogued = catalogue[name]; const profile = profileFor(entry.path, catalogued);
      return { name, source: source.name, profile, palette: profile === "xochip" ? paletteFor(catalogued) : undefined, url: `https://raw.githubusercontent.com/${source.repository}/${source.revision}/${entry.path}` } satisfies Game;
    });
  }));
  const games = results.flatMap(result => result.status === "fulfilled" ? result.value : []);
  const errors = results.flatMap(result => result.status === "rejected" ? [String(result.reason)] : []);
  return { games: games.sort((a, b) => a.name.localeCompare(b.name) || a.source.localeCompare(b.source)), errors };
}
export const PROFILE_LABEL: Record<Profile, string> = { chip8: "CHIP-8", chip48: "CHIP-48", modern: "Modern", schip10: "Super-CHIP 1.0", schip11: "Super-CHIP 1.1", schipc: "SCHIPC", "schip-modern": "Super-CHIP moderne", superchip: "Super-CHIP", xochip: "XO-CHIP" };
