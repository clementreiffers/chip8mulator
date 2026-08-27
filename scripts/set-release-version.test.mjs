import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import test from "node:test";

const exec_file = promisify(execFile);
const script = new URL("./set-release-version.mjs", import.meta.url).pathname;

test("synchronizes every distributable manifest to the release version", async () => {
  const root = await mkdtemp(join(tmpdir(), "chip8-release-version-"));
  await Promise.all([
    mkdir(join(root, "src/chip8-engine"), { recursive: true }),
    mkdir(join(root, "src/chip8-native-gui"), { recursive: true }),
    mkdir(join(root, "src/chip8-web"), { recursive: true }),
  ]);
  await Promise.all([
    writeFile(join(root, "src/chip8-engine/Cargo.toml"), '[package]\nversion = "0.1.0"\n'),
    writeFile(join(root, "src/chip8-native-gui/Cargo.toml"), '[package]\nversion = "0.1.0"\n'),
    writeFile(join(root, "src/chip8-web/package.json"), '{"version":"0.1.0"}\n'),
    writeFile(join(root, "src/chip8-web/package-lock.json"), '{"version":"0.1.0"}\n'),
  ]);

  await exec_file(process.execPath, [script, "1.2.3"], {
    env: { ...process.env, RELEASE_ROOT: root },
  });

  assert.match(await readFile(join(root, "src/chip8-engine/Cargo.toml"), "utf8"), /version = "1.2.3"/);
  assert.match(await readFile(join(root, "src/chip8-native-gui/Cargo.toml"), "utf8"), /version = "1.2.3"/);
  assert.equal(JSON.parse(await readFile(join(root, "src/chip8-web/package.json"), "utf8")).version, "1.2.3");
  assert.equal(JSON.parse(await readFile(join(root, "src/chip8-web/package-lock.json"), "utf8")).version, "1.2.3");
});

test("rejects prerelease versions", async () => {
  await assert.rejects(exec_file(process.execPath, [script, "1.2.3-rc.1"]));
});
