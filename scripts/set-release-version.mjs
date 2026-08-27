import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const [version] = process.argv.slice(2);
const VERSION_PATTERN = /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)$/;
const repository_root = process.env.RELEASE_ROOT ?? ".";

if (!VERSION_PATTERN.test(version)) {
  throw new Error(`Expected a stable semantic version, received: ${version}`);
}

const cargo_manifests = [
  "src/chip8-engine/Cargo.toml",
  "src/chip8-native-gui/Cargo.toml",
];

for (const manifest of cargo_manifests) {
  const path = join(repository_root, manifest);
  const source = await readFile(path, "utf8");
  const updated = source.replace(
    /^(\[package\][\s\S]*?^version = ")[^"]+(".*)$/m,
    `$1${version}$2`,
  );

  if (updated === source) {
    throw new Error(`Could not update the package version in ${manifest}`);
  }

  await writeFile(path, updated);
}

for (const file of ["src/chip8-web/package.json", "src/chip8-web/package-lock.json"]) {
  const path = join(repository_root, file);
  const package_file = JSON.parse(await readFile(path, "utf8"));
  package_file.version = version;
  await writeFile(path, `${JSON.stringify(package_file, null, 2)}\n`);
}
