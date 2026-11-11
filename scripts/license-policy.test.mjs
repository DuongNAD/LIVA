import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// LIVA is dual-licensed under MIT or Apache-2.0. The licence is stated in four places
// (the licence files, the Cargo manifests, README.md and CONTRIBUTING.md), and these
// tests keep them saying the same thing.
const CRATE_MANIFESTS = [
  "crates/liva-core-types/Cargo.toml",
  "crates/liva-storage/Cargo.toml",
  "crates/liva-llm/Cargo.toml",
  "crates/liva-tools/Cargo.toml",
  "crates/liva-cua/Cargo.toml",
  "liva-native-core/Cargo.toml",
  "liva-desktop/src-tauri/Cargo.toml",
];

async function readRepoFile(relativePath) {
  return readFile(path.join(repoRoot, relativePath), "utf8");
}

async function exists(relativePath) {
  try {
    await access(path.join(repoRoot, relativePath));
    return true;
  } catch {
    return false;
  }
}

test("the licence files are the MIT and Apache-2.0 texts", async () => {
  const [mit, apache] = await Promise.all([
    readRepoFile("LICENSE-MIT"),
    readRepoFile("LICENSE-APACHE"),
  ]);

  assert.match(mit, /^MIT License/);
  assert.match(mit, /Permission is hereby granted, free of charge/);
  assert.match(apache, /Apache License\s+Version 2\.0, January 2004/);
  assert.equal(
    await exists("LICENSE"),
    false,
    "the old Personal & Internal Use LICENSE file must not come back beside the open licences",
  );
});

test("every crate manifest declares MIT OR Apache-2.0", async () => {
  for (const manifest of CRATE_MANIFESTS) {
    const text = await readRepoFile(manifest);
    assert.match(text, /^license\s*=\s*"MIT OR Apache-2\.0"\s*$/m, manifest);
  }
});

test("README states the open licence and nothing that contradicts it", async () => {
  const readme = await readRepoFile("README.md");

  assert.match(readme, /\(LICENSE-MIT\)/);
  assert.match(readme, /\(LICENSE-APACHE\)/);
  assert.match(readme, /Third-party models and assets/i);
  assert.doesNotMatch(readme, /Personal & Internal Use License/i);
  assert.doesNotMatch(readme, /redistribution[^.\n]*(strictly )?prohibited/i);
});

test("CONTRIBUTING accepts forks and pull requests under the open licence", async () => {
  const policy = await readRepoFile("CONTRIBUTING.md");

  assert.match(policy, /feedback, issue reports and pull requests are welcome/i);
  assert.match(policy, /\(LICENSE-MIT\)/);
  assert.match(policy, /\(LICENSE-APACHE\)/);
  assert.match(policy, /any contribution you intentionally submit[\s\S]*dual-licensed as above/i);
  assert.doesNotMatch(policy, /not accepted/i);
  assert.doesNotMatch(policy, /Personal & Internal Use License/i);
});
