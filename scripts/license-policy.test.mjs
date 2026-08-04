import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

async function readRepoFile(relativePath) {
  return readFile(path.join(repoRoot, relativePath), "utf8");
}

test("README does not invite redistribution forbidden by LICENSE", async () => {
  const [license, readme] = await Promise.all([
    readRepoFile("LICENSE"),
    readRepoFile("README.md"),
  ]);

  assert.match(license, /STRICTLY PROHIBITED[\s\S]*Redistributing/i);
  assert.doesNotMatch(readme, /code contributions\s*\(Pull Requests\)/i);
  assert.doesNotMatch(readme, /standard open-source workflow/i);
  assert.doesNotMatch(readme, /^\s*1\.\s+\*\*Fork\*\*/im);
});

test("CONTRIBUTING policy matches the current redistribution restriction", async () => {
  const policy = await readRepoFile("CONTRIBUTING.md");

  assert.match(policy, /feedback and issue reports are welcome/i);
  assert.match(policy, /public forks? or pull requests? are not accepted/i);
  assert.match(policy, /Personal & Internal Use License/i);
});
