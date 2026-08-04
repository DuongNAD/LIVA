import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

function makePcm16MonoWav(marker) {
  const sampleRate = 16_000;
  const samples = sampleRate;
  const dataSize = samples * 2;
  const wav = Buffer.alloc(44 + dataSize);
  wav.write("RIFF", 0);
  wav.writeUInt32LE(36 + dataSize, 4);
  wav.write("WAVE", 8);
  wav.write("fmt ", 12);
  wav.writeUInt32LE(16, 16);
  wav.writeUInt16LE(1, 20);
  wav.writeUInt16LE(1, 22);
  wav.writeUInt32LE(sampleRate, 24);
  wav.writeUInt32LE(sampleRate * 2, 28);
  wav.writeUInt16LE(2, 32);
  wav.writeUInt16LE(16, 34);
  wav.write("data", 36);
  wav.writeUInt32LE(dataSize, 40);
  wav.fill(marker, 44);
  return wav;
}

test("cấu hình train Hey Liva đạt production guardrails", () => {
  const result = spawnSync(
    process.execPath,
    ["scripts/wake-training-check.mjs"],
    { cwd: process.cwd(), encoding: "utf8" },
  );

  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(result.stdout, /wake training config: PASS/u);
});

test("classifier chỉ có đúng một target phrase là Hey Liva", () => {
  const config = readFileSync("tools/wakeword/hey_liva_prod.yaml", "utf8");
  const targetBlock = config.match(
    /target_phrases:\s*\n(?<items>(?:\s+-[^\n]+\n)+)/u,
  );

  assert.ok(targetBlock);
  const phrases = [
    ...targetBlock.groups.items.matchAll(/^\s+-\s*"([^"]+)"\s*$/gmu),
  ].map((match) => match[1]);
  assert.deepEqual(phrases, ["hey liva"]);
});

test("corpus giọng và output huấn luyện không thể bị commit nhầm", () => {
  const gitignore = readFileSync(".gitignore", "utf8");

  assert.match(gitignore, /^data\/wake-enrollment\/$/mu);
  assert.match(gitignore, /^tools\/wakeword\/work\/$/mu);
});

test("PowerShell ép Python UTF-8 để CLI không vỡ trên Windows cp1252", () => {
  const script = readFileSync("scripts/train-wakeword.ps1", "utf8");

  assert.match(script, /\$env:PYTHONUTF8\s*=\s*"1"/u);
});

test("wake matrix resume skips variants with complete export and evaluation artifacts", () => {
  const script = readFileSync("scripts/train-wakeword-matrix.ps1", "utf8");

  assert.match(script, /ValidateSet\([^)]*"Resume"/su);
  assert.match(script, /function Test-VariantCompleted/u);
  assert.match(script, /function Resume-Experiment/u);
  assert.match(script, /\$modelPath\s*=\s*[^\n]+\.onnx/u);
  assert.match(script, /\$evalPath\s*=\s*[^\n]+_eval\.json/u);
  assert.match(
    script,
    /Test-Path -LiteralPath \$modelPath\) -and \(Test-Path -LiteralPath \$evalPath/u,
  );
  assert.doesNotMatch(script, /Get-Process\s+-Id/u);
});

test("wake benchmark passes its explicit argument list through Invoke-Checked", () => {
  const script = readFileSync("scripts/train-wakeword-matrix.ps1", "utf8");
  const benchmarkBlock = script.match(
    /function Benchmark-Variants \{(?<body>[\s\S]*?)\n\}/u,
  );

  assert.ok(benchmarkBlock);
  assert.match(benchmarkBlock.groups.body, /\$benchmarkArgs\s*=\s*@\(/u);
  assert.match(
    benchmarkBlock.groups.body,
    /& \$benchmarkPath @benchmarkArgs/u,
  );
  assert.doesNotMatch(benchmarkBlock.groups.body, /\$args\b/u);
});

test("toolchain train pin PyTorch CUDA và từ chối chạy chậm trên CPU", () => {
  const toolchain = JSON.parse(
    readFileSync("tools/wakeword/toolchain.json", "utf8"),
  );
  const script = readFileSync("scripts/train-wakeword.ps1", "utf8");

  assert.equal(toolchain.torch.version, "2.11.0");
  assert.equal(
    toolchain.torch.index_url,
    "https://download.pytorch.org/whl/cu128",
  );
  assert.match(script, /torch\.cuda\.is_available\(\)/u);
  assert.match(script, /torchaudio==\$torchVersion/u);
  assert.match(script, /--index-url\s+\$torchIndexUrl/u);
});

test("toolchain có pha personalization từ enrollment giọng thật", () => {
  const script = readFileSync("scripts/train-wakeword.ps1", "utf8");

  assert.match(script, /"Personalize"/u);
  assert.match(script, /prepare-wake-enrollment\.mjs/u);
  assert.match(script, /Prepare-Enrollment/u);
  assert.match(script, /NegativeEnrollmentDir/u);
  assert.match(script, /--negative-source/u);
});

test("personalization tach recording truoc khi nhan ban, khong ro ri train/test", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "liva-wake-enrollment-"));
  const sourceDir = join(tempRoot, "source");
  const modelDir = join(tempRoot, "wake_liva_en_v2");
  mkdirSync(sourceDir, { recursive: true });

  try {
    for (let index = 0; index < 5; index += 1) {
      writeFileSync(
        join(
          sourceDir,
          `hey_liva_positive_${String(index + 1).padStart(2, "0")}.wav`,
        ),
        makePcm16MonoWav(20 + index),
      );
    }

    const result = spawnSync(
      process.execPath,
      [
        "scripts/prepare-wake-enrollment.mjs",
        "--source",
        sourceDir,
        "--model-dir",
        modelDir,
        "--minimum",
        "5",
        "--train-copies",
        "8",
        "--test-copies",
        "2",
      ],
      { cwd: process.cwd(), encoding: "utf8" },
    );

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const trainFiles = readdirSync(join(modelDir, "positive_train")).filter(
      (name) => /^clip_8\d{5}\.wav$/u.test(name),
    );
    const testFiles = readdirSync(join(modelDir, "positive_test")).filter(
      (name) => /^clip_8\d{5}\.wav$/u.test(name),
    );
    assert.equal(trainFiles.length, 8);
    assert.equal(testFiles.length, 2);

    const markers = (directory, files) =>
      new Set(files.map((name) => readFileSync(join(directory, name)).at(-1)));
    const trainMarkers = markers(join(modelDir, "positive_train"), trainFiles);
    const testMarkers = markers(join(modelDir, "positive_test"), testFiles);
    assert.equal(
      [...trainMarkers].some((marker) => testMarkers.has(marker)),
      false,
    );
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
});

test("personalization inject hard negatives cua chu may va tach train test", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "liva-wake-hard-negative-"));
  const positiveDir = join(tempRoot, "positive");
  const negativeDir = join(tempRoot, "negative");
  const modelDir = join(tempRoot, "wake_liva_en_v2");
  mkdirSync(positiveDir, { recursive: true });
  mkdirSync(negativeDir, { recursive: true });

  try {
    for (let index = 0; index < 5; index += 1) {
      writeFileSync(
        join(positiveDir, `positive_${index}.wav`),
        makePcm16MonoWav(20 + index),
      );
      writeFileSync(
        join(negativeDir, `negative_${index}.wav`),
        makePcm16MonoWav(80 + index),
      );
    }

    const result = spawnSync(
      process.execPath,
      [
        "scripts/prepare-wake-enrollment.mjs",
        "--source",
        positiveDir,
        "--negative-source",
        negativeDir,
        "--model-dir",
        modelDir,
        "--minimum",
        "5",
        "--negative-minimum",
        "5",
        "--train-copies",
        "8",
        "--test-copies",
        "2",
        "--negative-train-copies",
        "6",
        "--negative-test-copies",
        "2",
      ],
      { cwd: process.cwd(), encoding: "utf8" },
    );

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const negativeTrain = readdirSync(join(modelDir, "negative_train")).filter(
      (name) => /^clip_9[0-4]\d{4}\.wav$/u.test(name),
    );
    const negativeTest = readdirSync(join(modelDir, "negative_test")).filter(
      (name) => /^clip_9[5-9]\d{4}\.wav$/u.test(name),
    );
    assert.equal(negativeTrain.length, 6);
    assert.equal(negativeTest.length, 2);

    const markers = (directory, files) =>
      new Set(files.map((name) => readFileSync(join(directory, name)).at(-1)));
    const trainMarkers = markers(join(modelDir, "negative_train"), negativeTrain);
    const testMarkers = markers(join(modelDir, "negative_test"), negativeTest);
    assert.equal(
      [...trainMarkers].some((marker) => testMarkers.has(marker)),
      false,
    );

    const manifest = JSON.parse(
      readFileSync(join(modelDir, "enrollment-manifest.json"), "utf8"),
    );
    assert.equal(manifest.negative_source_recordings, 5);
    assert.equal(manifest.negative_train_recordings, 4);
    assert.equal(manifest.negative_test_recordings, 1);
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
});

test("personalization fail-closed khi corpus thieu mau", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "liva-wake-enrollment-small-"));
  const sourceDir = join(tempRoot, "source");
  const modelDir = join(tempRoot, "wake_liva_en_v2");
  mkdirSync(sourceDir, { recursive: true });
  writeFileSync(join(sourceDir, "only.wav"), makePcm16MonoWav(42));

  try {
    const result = spawnSync(
      process.execPath,
      [
        "scripts/prepare-wake-enrollment.mjs",
        "--source",
        sourceDir,
        "--model-dir",
        modelDir,
        "--minimum",
        "2",
        "--train-copies",
        "2",
        "--test-copies",
        "1",
      ],
      { cwd: process.cwd(), encoding: "utf8" },
    );
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /at least 2 valid WAV recordings/u);
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
});
