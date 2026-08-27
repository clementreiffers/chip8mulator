import { appendFile } from "node:fs/promises";
import semantic_release from "semantic-release";

const result = await semantic_release({ ci: true });
const output = result
  ? `published=true\nversion=${result.nextRelease.version}\ntag=v${result.nextRelease.version}\n`
  : "published=false\n";

await appendFile(process.env.GITHUB_OUTPUT, output);
