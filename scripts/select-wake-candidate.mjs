#!/usr/bin/env node

import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

function fail(message) {
  throw new Error(message);
}

function parseArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || value === undefined) {
      fail(`Invalid argument near ${key ?? "<end>"}`);
    }
    options[key.slice(2)] = value;
  }
  for (const required of ["reports-dir", "matrix", "output"]) {
    if (!options[required]) fail(`--${required} is required`);
  }
  return options;
}

function candidateScore(left, right) {
  return (
    Number(right.metrics.accepted) - Number(left.metrics.accepted) ||
    right.metrics.recall - left.metrics.recall ||
    left.metrics.false_positives_per_hour - right.metrics.false_positives_per_hour ||
    right.metrics.negative_hours - left.metrics.negative_hours ||
    left.variant_id.localeCompare(right.variant_id, "en")
  );
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const reportsDir = resolve(options["reports-dir"]);
  const matrix = JSON.parse(readFileSync(resolve(options.matrix), "utf8"));
  const variantMap = new Map(matrix.variants.map((variant) => [variant.id, variant]));
  const gates = matrix.promotion_gates ?? {
    minimum_owner_recall: 0.9,
    maximum_fpph: 1,
    minimum_ambient_negative_hours: 1,
  };
  const candidates = [];

  for (const name of readdirSync(reportsDir).filter(
    (entry) => entry.endsWith(".json") && !entry.endsWith(".variant.json"),
  )) {
    const variantId = name.slice(0, -".json".length);
    const variant = variantMap.get(variantId);
    if (!variant) continue;
    const report = JSON.parse(readFileSync(join(reportsDir, name), "utf8"));
    const manifestPath = join(reportsDir, `${variantId}.variant.json`);
    if (!existsSync(manifestPath)) fail(`${manifestPath}: variant manifest is missing`);
    const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
    const metrics = report.metrics;
    for (const field of [
      "recall",
      "false_positives_per_hour",
      "negative_hours",
      "accepted",
    ]) {
      if (metrics?.[field] === undefined) fail(`${name}: metrics.${field} is missing`);
    }
    candidates.push({
      variant_id: variantId,
      model_name: variant.model_name,
      model_path: report.model_path,
      model_sha256: report.model_sha256,
      metrics,
      owner_hard_negatives_present: Boolean(manifest.owner_hard_negatives_present),
      public_holdout_records: Number(manifest.public_holdout_records ?? 0),
    });
  }
  if (candidates.length === 0) fail("No benchmark reports matched the variant matrix");
  candidates.sort(candidateScore);

  const blockers = [];
  const productionCandidates = candidates.filter((candidate) => {
    const passesMetrics =
      candidate.metrics.accepted === true &&
      candidate.metrics.recall >= gates.minimum_owner_recall &&
      candidate.metrics.false_positives_per_hour <= gates.maximum_fpph &&
      candidate.metrics.negative_hours >= gates.minimum_ambient_negative_hours;
    return passesMetrics && candidate.owner_hard_negatives_present;
  });
  if (!candidates.some((candidate) => candidate.owner_hard_negatives_present)) {
    blockers.push("owner hard-negative corpus is missing");
  }
  if (
    !candidates.some(
      (candidate) =>
        candidate.metrics.negative_hours >= gates.minimum_ambient_negative_hours,
    )
  ) {
    blockers.push("ambient negative holdout is shorter than the release gate");
  }
  if (
    !candidates.some(
      (candidate) =>
        candidate.metrics.recall >= gates.minimum_owner_recall &&
        candidate.metrics.false_positives_per_hour <= gates.maximum_fpph,
    )
  ) {
    blockers.push("no candidate meets the recall/FPPH gate");
  }

  const selection = {
    schema_version: 1,
    gates,
    experimental_winner: candidates[0],
    production_winner: productionCandidates[0] ?? null,
    blockers,
    candidates,
  };
  writeFileSync(
    resolve(options.output),
    `${JSON.stringify(selection, null, 2)}\n`,
    "utf8",
  );
  process.stdout.write(`${JSON.stringify(selection)}\n`);
  process.exitCode = selection.production_winner ? 0 : 2;
}

try {
  main();
} catch (error) {
  process.stderr.write(`wake candidate selection: FAIL - ${error.message}\n`);
  process.exitCode = 1;
}
