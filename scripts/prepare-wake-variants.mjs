#!/usr/bin/env node

import {
  copyFileSync,
  existsSync,
  linkSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, relative, resolve, sep } from "node:path";

const RAW_CLIP = /^clip_(\d{6})\.wav$/u;
const PUBLIC_TRAIN_PREFIX = 600_000;
const PUBLIC_VALIDATION_PREFIX = 700_000;
const OWNER_TRAIN_PREFIX = 800_000;
const OWNER_TEST_PREFIX = 850_000;
const OWNER_NEGATIVE_TRAIN_PREFIX = 900_000;
const OWNER_NEGATIVE_TEST_PREFIX = 950_000;
const SPLITS = [
  "positive_train",
  "positive_test",
  "negative_train",
  "negative_test",
  "background_train",
  "background_test",
];

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
  for (const required of [
    "matrix",
    "base-model-dir",
    "public-corpus-dir",
    "owner-positive-dir",
    "output-dir",
    "config-template",
  ]) {
    if (!options[required]) fail(`--${required} is required`);
  }
  return options;
}

function linkedCopy(source, destination) {
  mkdirSync(dirname(destination), { recursive: true });
  if (existsSync(destination)) return;
  if (process.env.LIVA_WAKE_FORCE_COPY !== "1") {
    try {
      linkSync(source, destination);
      return;
    } catch (error) {
      if (!["EXDEV", "EPERM", "EACCES", "EMLINK", "UNKNOWN"].includes(error.code)) {
        throw error;
      }
    }
  }
  copyFileSync(source, destination);
}

function wavFiles(directory, minimum, label) {
  if (!existsSync(directory) || !statSync(directory).isDirectory()) {
    fail(`${directory}: ${label} directory does not exist`);
  }
  const files = readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.toLowerCase().endsWith(".wav"))
    .map((entry) => join(directory, entry.name))
    .sort((left, right) => left.localeCompare(right, "en"));
  if (files.length < minimum) {
    fail(`${label} requires at least ${minimum} WAV files; found ${files.length}`);
  }
  return files;
}

function splitOwner(files) {
  return {
    train: files.filter((_, index) => index % 5 !== 0),
    test: files.filter((_, index) => index % 5 === 0),
  };
}

function replicate(files, directory, prefix, copies) {
  for (let index = 0; index < copies; index += 1) {
    linkedCopy(
      files[index % files.length],
      join(directory, `clip_${String(prefix + index).padStart(6, "0")}.wav`),
    );
  }
}

function cloneSyntheticBase(baseDir, modelDir) {
  let files = 0;
  for (const split of SPLITS) {
    const sourceDir = join(baseDir, split);
    if (!existsSync(sourceDir)) fail(`${sourceDir}: base split is missing`);
    const destinationDir = join(modelDir, split);
    mkdirSync(destinationDir, { recursive: true });
    for (const entry of readdirSync(sourceDir, { withFileTypes: true })) {
      const match = entry.isFile() ? entry.name.match(RAW_CLIP) : null;
      if (!match || Number(match[1]) >= PUBLIC_TRAIN_PREFIX) continue;
      linkedCopy(join(sourceDir, entry.name), join(destinationDir, entry.name));
      files += 1;
    }
  }
  return files;
}

function publicMetadata(publicRoot) {
  const path = join(publicRoot, "metadata.jsonl");
  if (!existsSync(path)) fail(`${path}: fetch public corpus first`);
  return readFileSync(path, "utf8")
    .split(/\r?\n/u)
    .filter(Boolean)
    .map((line) => JSON.parse(line))
    .sort((left, right) =>
      `${left.dataset_id}:${left.split}:${left.sha256}`.localeCompare(
        `${right.dataset_id}:${right.split}:${right.sha256}`,
        "en",
      ),
    );
}

function safePublicPath(publicRoot, record) {
  const source = resolve(publicRoot, record.path);
  const rel = relative(publicRoot, source);
  if (!rel || rel.startsWith(`..${sep}`) || rel === "..") {
    fail(`Public corpus path escapes root: ${record.path}`);
  }
  if (!existsSync(source)) fail(`${source}: public corpus file is missing`);
  return source;
}

function injectPublic(records, selected, publicRoot, modelDir) {
  const selectedSet = new Set(selected);
  const counts = { train: 0, validation: 0, test: 0 };
  for (const record of records) {
    if (!selectedSet.has(record.dataset_id)) continue;
    if (!(record.split in counts)) fail(`Invalid public split: ${record.split}`);
    counts[record.split] += 1;
    if (record.split === "test") continue;
    const prefix = record.split === "train" ? PUBLIC_TRAIN_PREFIX : PUBLIC_VALIDATION_PREFIX;
    const directory = record.split === "train" ? "negative_train" : "negative_test";
    const index = counts[record.split] - 1;
    linkedCopy(
      safePublicPath(publicRoot, record),
      join(modelDir, directory, `clip_${String(prefix + index).padStart(6, "0")}.wav`),
    );
  }
  return counts;
}

function createConfig(template, variant, outputDir) {
  const replacements = [
    [/^model_name:.*$/mu, `model_name: ${variant.model_name}`],
    [/^output_dir:.*$/mu, `output_dir: ${outputDir.replaceAll("\\", "/")}`],
    [/^  model_size:.*$/mu, `  model_size: ${variant.model_size}`],
    [/^steps:.*$/mu, `steps: ${variant.steps}`],
    [
      /^  adversarial_negative:.*$/mu,
      `  adversarial_negative: ${variant.adversarial_negative_batch}`,
    ],
  ];
  let config = template;
  for (const [pattern, replacement] of replacements) {
    if (!pattern.test(config)) fail(`Config template is missing ${pattern}`);
    config = config.replace(pattern, replacement);
  }
  return config;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const matrix = JSON.parse(readFileSync(resolve(options.matrix), "utf8"));
  if (matrix.schema_version !== 1) fail("Unsupported variant matrix schema");
  const baseDir = resolve(options["base-model-dir"]);
  const publicRoot = resolve(options["public-corpus-dir"]);
  const ownerPositiveDir = resolve(options["owner-positive-dir"]);
  const ownerNegativeDir = options["owner-negative-dir"]
    ? resolve(options["owner-negative-dir"])
    : null;
  const outputDir = resolve(options["output-dir"]);
  const configTemplate = readFileSync(resolve(options["config-template"]), "utf8");
  const ownerMinimum = Number(matrix.owner_minimum ?? 20);
  const ownerTrainCopies = Number(matrix.owner_train_copies ?? 10_000);
  const ownerTestCopies = Number(matrix.owner_test_copies ?? 1_000);
  const ownerPositive = splitOwner(
    wavFiles(ownerPositiveDir, ownerMinimum, "Owner positive"),
  );
  const ownerNegative =
    ownerNegativeDir && existsSync(ownerNegativeDir)
      ? splitOwner(wavFiles(ownerNegativeDir, ownerMinimum, "Owner negative"))
      : null;
  const records = publicMetadata(publicRoot);
  const summaries = [];

  mkdirSync(join(outputDir, "configs"), { recursive: true });
  for (const variant of matrix.variants) {
    const modelDir = join(outputDir, variant.model_name);
    mkdirSync(modelDir, { recursive: true });
    const baseFiles = cloneSyntheticBase(baseDir, modelDir);
    replicate(
      ownerPositive.train,
      join(modelDir, "positive_train"),
      OWNER_TRAIN_PREFIX,
      ownerTrainCopies,
    );
    replicate(
      ownerPositive.test,
      join(modelDir, "positive_test"),
      OWNER_TEST_PREFIX,
      ownerTestCopies,
    );
    if (ownerNegative) {
      replicate(
        ownerNegative.train,
        join(modelDir, "negative_train"),
        OWNER_NEGATIVE_TRAIN_PREFIX,
        ownerTrainCopies,
      );
      replicate(
        ownerNegative.test,
        join(modelDir, "negative_test"),
        OWNER_NEGATIVE_TEST_PREFIX,
        ownerTestCopies,
      );
    }
    const publicCounts = injectPublic(
      records,
      variant.datasets,
      publicRoot,
      modelDir,
    );
    const configPath = join(outputDir, "configs", `${variant.id}.yaml`);
    writeFileSync(
      configPath,
      createConfig(configTemplate, variant, outputDir),
      "utf8",
    );
    const manifest = {
      schema_version: 1,
      variant_id: variant.id,
      model_name: variant.model_name,
      model_size: variant.model_size,
      datasets: variant.datasets,
      base_files: baseFiles,
      owner_positive_train_copies: ownerTrainCopies,
      owner_positive_test_copies: ownerTestCopies,
      owner_hard_negatives_present: Boolean(ownerNegative),
      public_train_records: publicCounts.train,
      public_validation_records: publicCounts.validation,
      public_holdout_records: publicCounts.test,
      training_data_eligible: true,
      production_eligible: false,
      config_path: configPath,
    };
    writeFileSync(
      join(modelDir, "variant-manifest.json"),
      `${JSON.stringify(manifest, null, 2)}\n`,
      "utf8",
    );
    summaries.push(manifest);
  }
  process.stdout.write(`${JSON.stringify({ variants: summaries })}\n`);
}

try {
  main();
} catch (error) {
  process.stderr.write(`wake variants: FAIL - ${error.message}\n`);
  process.exitCode = 1;
}
