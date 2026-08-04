#!/usr/bin/env node

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { basename, join, resolve } from "node:path";

const DEFAULT_SOURCE = "data/wake-enrollment/positive";
const DEFAULT_MODEL_DIR = "tools/wakeword/work/output/wake_liva_en_v2";
const TRAIN_PREFIX = 800_000;
const TEST_PREFIX = 850_000;
const NEGATIVE_TRAIN_PREFIX = 900_000;
const NEGATIVE_TEST_PREFIX = 950_000;
const MAX_COPIES_PER_SPLIT = 50_000;

function fail(message) {
  throw new Error(message);
}

function parsePositiveInteger(raw, option) {
  const value = Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(value) || value <= 0 || String(value) !== raw) {
    fail(`${option} must be a positive integer`);
  }
  return value;
}

function parseArgs(argv) {
  const options = {
    source: DEFAULT_SOURCE,
    negativeSource: null,
    modelDir: DEFAULT_MODEL_DIR,
    minimum: 20,
    negativeMinimum: 20,
    trainCopies: 10_000,
    testCopies: 1_000,
    negativeTrainCopies: 10_000,
    negativeTestCopies: 1_000,
  };

  for (let index = 0; index < argv.length; index += 2) {
    const option = argv[index];
    const value = argv[index + 1];
    if (!option?.startsWith("--") || value === undefined) {
      fail(`Invalid argument near ${option ?? "<end>"}`);
    }
    switch (option) {
      case "--source":
        options.source = value;
        break;
      case "--negative-source":
        options.negativeSource = value;
        break;
      case "--model-dir":
        options.modelDir = value;
        break;
      case "--minimum":
        options.minimum = parsePositiveInteger(value, option);
        break;
      case "--negative-minimum":
        options.negativeMinimum = parsePositiveInteger(value, option);
        break;
      case "--train-copies":
        options.trainCopies = parsePositiveInteger(value, option);
        break;
      case "--test-copies":
        options.testCopies = parsePositiveInteger(value, option);
        break;
      case "--negative-train-copies":
        options.negativeTrainCopies = parsePositiveInteger(value, option);
        break;
      case "--negative-test-copies":
        options.negativeTestCopies = parsePositiveInteger(value, option);
        break;
      default:
        fail(`Unknown option: ${option}`);
    }
  }
  if (options.minimum < 2) fail("--minimum must be at least 2");
  if (options.negativeMinimum < 2)
    fail("--negative-minimum must be at least 2");
  if (options.trainCopies > MAX_COPIES_PER_SPLIT)
    fail("--train-copies exceeds the safe limit");
  if (options.testCopies > MAX_COPIES_PER_SPLIT)
    fail("--test-copies exceeds the safe limit");
  if (options.negativeTrainCopies > MAX_COPIES_PER_SPLIT)
    fail("--negative-train-copies exceeds the safe limit");
  if (options.negativeTestCopies > MAX_COPIES_PER_SPLIT)
    fail("--negative-test-copies exceeds the safe limit");
  return options;
}

function inspectWav(filePath) {
  const wav = readFileSync(filePath);
  if (
    wav.length < 44 ||
    wav.toString("ascii", 0, 4) !== "RIFF" ||
    wav.toString("ascii", 8, 12) !== "WAVE"
  ) {
    fail(`${filePath}: expected a RIFF/WAVE file`);
  }

  let format;
  let dataBytes;
  for (let offset = 12; offset + 8 <= wav.length; ) {
    const chunkId = wav.toString("ascii", offset, offset + 4);
    const chunkSize = wav.readUInt32LE(offset + 4);
    const payloadStart = offset + 8;
    const payloadEnd = payloadStart + chunkSize;
    if (payloadEnd > wav.length)
      fail(`${filePath}: truncated WAV chunk ${chunkId}`);
    if (chunkId === "fmt ") {
      if (chunkSize < 16) fail(`${filePath}: invalid fmt chunk`);
      format = {
        audioFormat: wav.readUInt16LE(payloadStart),
        channels: wav.readUInt16LE(payloadStart + 2),
        sampleRate: wav.readUInt32LE(payloadStart + 4),
        byteRate: wav.readUInt32LE(payloadStart + 8),
        blockAlign: wav.readUInt16LE(payloadStart + 12),
        bitsPerSample: wav.readUInt16LE(payloadStart + 14),
      };
    } else if (chunkId === "data") {
      dataBytes = chunkSize;
    }
    offset = payloadEnd + (chunkSize % 2);
  }

  if (!format || dataBytes === undefined)
    fail(`${filePath}: missing fmt or data chunk`);
  const expected = {
    audioFormat: 1,
    channels: 1,
    sampleRate: 16_000,
    byteRate: 32_000,
    blockAlign: 2,
    bitsPerSample: 16,
  };
  for (const [field, value] of Object.entries(expected)) {
    if (format[field] !== value) {
      fail(`${filePath}: ${field} must be ${value}, got ${format[field]}`);
    }
  }
  const durationSeconds = dataBytes / format.byteRate;
  if (durationSeconds < 0.5 || durationSeconds > 5) {
    fail(`${filePath}: duration must be between 0.5 and 5 seconds`);
  }
}

function removePriorInjection(directory, pattern) {
  if (!statSync(directory).isDirectory())
    fail(`${directory}: expected a directory`);
  for (const name of readdirSync(directory)) {
    if (pattern.test(name)) unlinkSync(join(directory, name));
  }
}

function replicate(recordings, directory, prefix, copies) {
  for (let index = 0; index < copies; index += 1) {
    const outputName = `clip_${String(prefix + index).padStart(6, "0")}.wav`;
    copyFileSync(
      recordings[index % recordings.length],
      join(directory, outputName),
    );
  }
}

function recordingsAt(source, minimum, label) {
  if (!existsSync(source) || !statSync(source).isDirectory()) {
    fail(`${source}: ${label} enrollment directory does not exist`);
  }
  const recordings = readdirSync(source, { withFileTypes: true })
    .filter(
      (entry) => entry.isFile() && entry.name.toLowerCase().endsWith(".wav"),
    )
    .map((entry) => join(source, entry.name))
    .sort((left, right) => left.localeCompare(right, "en"));
  if (recordings.length < minimum) {
    fail(
      `${label} enrollment requires at least ${minimum} valid WAV recordings; found ${recordings.length}`,
    );
  }
  recordings.forEach(inspectWav);
  return recordings;
}

function splitRecordings(recordings) {
  const test = recordings.filter((_, index) => index % 5 === 0);
  const train = recordings.filter((_, index) => index % 5 !== 0);
  if (train.length === 0 || test.length === 0) {
    fail("Enrollment split must contain both train and test recordings");
  }
  return { train, test };
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const source = resolve(options.source);
  const modelDir = resolve(options.modelDir);
  if (basename(modelDir) !== "wake_liva_en_v2") {
    fail("--model-dir must end in wake_liva_en_v2");
  }
  const recordings = recordingsAt(source, options.minimum, "Positive");
  const { train: trainRecordings, test: testRecordings } =
    splitRecordings(recordings);

  const trainDir = join(modelDir, "positive_train");
  const testDir = join(modelDir, "positive_test");
  mkdirSync(trainDir, { recursive: true });
  mkdirSync(testDir, { recursive: true });
  removePriorInjection(trainDir, /^clip_8[0-4]\d{4}(?:_r\d+)?\.wav$/u);
  removePriorInjection(testDir, /^clip_8[5-9]\d{4}(?:_r\d+)?\.wav$/u);
  replicate(trainRecordings, trainDir, TRAIN_PREFIX, options.trainCopies);
  replicate(testRecordings, testDir, TEST_PREFIX, options.testCopies);

  let negativeManifest = {};
  if (options.negativeSource) {
    const negativeSource = resolve(options.negativeSource);
    const negativeRecordings = recordingsAt(
      negativeSource,
      options.negativeMinimum,
      "Negative",
    );
    const { train: negativeTrain, test: negativeTest } =
      splitRecordings(negativeRecordings);
    const negativeTrainDir = join(modelDir, "negative_train");
    const negativeTestDir = join(modelDir, "negative_test");
    mkdirSync(negativeTrainDir, { recursive: true });
    mkdirSync(negativeTestDir, { recursive: true });
    removePriorInjection(
      negativeTrainDir,
      /^clip_9[0-4]\d{4}(?:_r\d+)?\.wav$/u,
    );
    removePriorInjection(
      negativeTestDir,
      /^clip_9[5-9]\d{4}(?:_r\d+)?\.wav$/u,
    );
    replicate(
      negativeTrain,
      negativeTrainDir,
      NEGATIVE_TRAIN_PREFIX,
      options.negativeTrainCopies,
    );
    replicate(
      negativeTest,
      negativeTestDir,
      NEGATIVE_TEST_PREFIX,
      options.negativeTestCopies,
    );
    negativeManifest = {
      negative_source_recordings: negativeRecordings.length,
      negative_train_recordings: negativeTrain.length,
      negative_test_recordings: negativeTest.length,
      negative_train_copies: options.negativeTrainCopies,
      negative_test_copies: options.negativeTestCopies,
    };
  }

  const manifest = {
    schema_version: 2,
    source_recordings: recordings.length,
    train_recordings: trainRecordings.length,
    test_recordings: testRecordings.length,
    train_copies: options.trainCopies,
    test_copies: options.testCopies,
    ...negativeManifest,
    split_rule:
      "sorted recordings where index % 5 == 0 are test; all others are train",
  };
  writeFileSync(
    join(modelDir, "enrollment-manifest.json"),
    `${JSON.stringify(manifest, null, 2)}\n`,
  );
  process.stdout.write(`${JSON.stringify(manifest)}\n`);
}

try {
  main();
} catch (error) {
  process.stderr.write(`wake enrollment: FAIL - ${error.message}\n`);
  process.exitCode = 1;
}
