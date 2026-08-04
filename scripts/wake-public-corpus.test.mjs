import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

function makePcm16MonoWav(marker, sampleCount = 16_000) {
  const sampleRate = 16_000;
  const dataSize = sampleCount * 2;
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

test("manifest public wake corpus pin nguồn HF và giấy phép thương mại", () => {
  const manifest = JSON.parse(
    readFileSync("tools/wakeword/public-datasets.json", "utf8"),
  );

  assert.equal(manifest.schema_version, 1);
  assert.deepEqual(
    manifest.datasets.map(({ id }) => id),
    ["fleurs_vi", "speech_commands_v2", "musan_noise"],
  );
  for (const dataset of manifest.datasets) {
    assert.match(dataset.repo_id, /^[\w.-]+\/[\w.-]+$/u);
    assert.match(dataset.revision, /^[a-f0-9]{40}$/u);
    assert.equal(dataset.license, "cc-by-4.0");
    assert.equal(dataset.commercial_use, true);
    assert.ok(dataset.max_samples.train > 0);
    assert.ok(dataset.max_samples.validation > 0);
    assert.ok(dataset.max_samples.test > 0);
  }
});

test("Speech Commands release holdout budgets enough audio for the one-hour gate", () => {
  const manifest = JSON.parse(
    readFileSync("tools/wakeword/public-datasets.json", "utf8"),
  );
  const speechCommands = manifest.datasets.find(
    ({ id }) => id === "speech_commands_v2",
  );

  assert.ok(speechCommands);
  assert.ok(speechCommands.max_samples.test >= 4_000);
});

test("fixture import lọc target phrase, dedup audio và giữ split gốc", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "liva-wake-public-"));
  const fixtureRoot = join(tempRoot, "fixture");
  const outputRoot = join(tempRoot, "output");
  mkdirSync(fixtureRoot, { recursive: true });

  try {
    const records = [
      ["train-a.wav", 11, "train", "xin chào", "speaker-a"],
      ["train-a-copy.wav", 11, "train", "xin chào", "speaker-a"],
      ["target.wav", 12, "train", "Hey Liva", "speaker-b"],
      ["validation.wav", 13, "validation", "mở nhạc", "speaker-c"],
      ["test.wav", 14, "test", "thời tiết hôm nay", "speaker-d"],
      ["too-short.wav", 15, "train", "ngắn", "speaker-e"],
    ];
    for (const [name, marker] of records) {
      writeFileSync(
        join(fixtureRoot, name),
        makePcm16MonoWav(marker, name === "too-short.wav" ? 4_000 : 16_000),
      );
    }
    writeFileSync(
      join(fixtureRoot, "records.jsonl"),
      `${records
        .map(([name, , split, text, group]) =>
          JSON.stringify({
            dataset_id: "fleurs_vi",
            audio_path: name,
            split,
            text,
            group,
          }),
        )
        .join("\n")}\n`,
    );

    const result = spawnSync(
      "tools/wakeword/venv/Scripts/python.exe",
      [
        "scripts/fetch-wake-public-corpus.py",
        "--manifest",
        "tools/wakeword/public-datasets.json",
        "--output",
        outputRoot,
        "--fixture-root",
        fixtureRoot,
      ],
      { cwd: process.cwd(), encoding: "utf8" },
    );

    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.equal(readdirSync(join(outputRoot, "fleurs_vi", "train")).length, 1);
    assert.equal(
      readdirSync(join(outputRoot, "fleurs_vi", "validation")).length,
      1,
    );
    assert.equal(readdirSync(join(outputRoot, "fleurs_vi", "test")).length, 1);
    const corpusManifest = JSON.parse(
      readFileSync(join(outputRoot, "corpus-manifest.json"), "utf8"),
    );
    assert.equal(corpusManifest.accepted, 3);
    assert.equal(corpusManifest.filtered_target_phrase, 1);
    assert.equal(corpusManifest.duplicates, 1);
    assert.equal(corpusManifest.rejected_audio, 1);
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
});

test("dry-run lập kế hoạch tải đúng revision và ngân sách từng split", () => {
  const result = spawnSync(
    "tools/wakeword/venv/Scripts/python.exe",
    [
      "scripts/fetch-wake-public-corpus.py",
      "--manifest",
      "tools/wakeword/public-datasets.json",
      "--output",
      "tools/wakeword/work/public-corpus",
      "--dry-run",
    ],
    { cwd: process.cwd(), encoding: "utf8" },
  );

  assert.equal(result.status, 0, result.stderr || result.stdout);
  const plan = JSON.parse(result.stdout);
  assert.equal(plan.mode, "dry-run");
  assert.deepEqual(
    plan.datasets.map(({ id }) => id),
    ["fleurs_vi", "speech_commands_v2", "musan_noise"],
  );
  assert.equal(plan.datasets[0].revision.length, 40);
  assert.equal(plan.datasets[1].max_samples.train, 6000);
});

test("adapter HF đọc audio streaming và tạo corpus có provenance", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "liva-wake-hf-adapter-"));
  const fakeModuleRoot = join(tempRoot, "fake-modules");
  const datasetsModule = join(fakeModuleRoot, "datasets");
  const audioRoot = join(tempRoot, "audio");
  const outputRoot = join(tempRoot, "output");
  const manifestPath = join(tempRoot, "public-datasets.json");
  mkdirSync(datasetsModule, { recursive: true });
  mkdirSync(audioRoot, { recursive: true });

  try {
    for (let index = 0; index < 9; index += 1) {
      writeFileSync(
        join(audioRoot, `${index}.wav`),
        makePcm16MonoWav(30 + index),
      );
    }
    writeFileSync(
      join(datasetsModule, "__init__.py"),
      `
import os

class Audio:
    def __init__(self, decode=False):
        self.decode = decode

class Stream(list):
    def cast_column(self, *_args, **_kwargs):
        return self

def load_dataset(path, config=None, split=None, **_kwargs):
    root = os.environ["LIVA_FAKE_AUDIO_ROOT"]
    split_index = {"train": 0, "validation": 1, "test": 2}.get(split, 0)
    if path == "google/fleurs":
        return Stream([{"audio": {"path": os.path.join(root, f"{split_index}.wav")}, "transcription": "xin chao", "id": f"fleurs-{split}"}])
    if path == "google/speech_commands":
        return Stream([{"audio": {"path": os.path.join(root, f"{3 + split_index}.wav")}, "label": "yes", "speaker_id": f"commands-{split}"}])
    if path == "corypaik/musan":
        return Stream([
            {"audio": {"path": os.path.join(root, "6.wav")}, "source": "free-sound", "path": "noise/a.wav"},
            {"audio": {"path": os.path.join(root, "7.wav")}, "source": "free-sound", "path": "noise/b.wav"},
            {"audio": {"path": os.path.join(root, "8.wav")}, "source": "free-sound", "path": "noise/c.wav"},
        ])
    raise RuntimeError(f"unexpected dataset: {path} {config} {split}")
`,
    );
    const fakeManifest = JSON.parse(
      readFileSync("tools/wakeword/public-datasets.json", "utf8"),
    );
    for (const dataset of fakeManifest.datasets) {
      dataset.format = "huggingface";
    }
    writeFileSync(manifestPath, JSON.stringify(fakeManifest));

    const result = spawnSync(
      "tools/wakeword/venv/Scripts/python.exe",
      [
        "scripts/fetch-wake-public-corpus.py",
        "--manifest",
        manifestPath,
        "--output",
        outputRoot,
      ],
      {
        cwd: process.cwd(),
        encoding: "utf8",
        env: {
          ...process.env,
          PYTHONPATH: fakeModuleRoot,
          LIVA_FAKE_AUDIO_ROOT: audioRoot,
        },
      },
    );

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const corpusManifest = JSON.parse(
      readFileSync(join(outputRoot, "corpus-manifest.json"), "utf8"),
    );
    assert.equal(corpusManifest.mode, "huggingface");
    assert.equal(corpusManifest.accepted, 9);
    assert.equal(corpusManifest.datasets[1].revision.length, 40);
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
});

test("ma trận có control và bốn ứng viên HF với model name tách biệt", () => {
  const variants = JSON.parse(
    readFileSync("tools/wakeword/variants.json", "utf8"),
  );

  assert.equal(variants.schema_version, 1);
  assert.deepEqual(
    variants.variants.map(({ id }) => id),
    [
      "control_medium",
      "fleurs_medium",
      "commands_medium",
      "hybrid_medium",
      "hybrid_large",
    ],
  );
  assert.deepEqual(variants.variants[0].datasets, []);
  assert.deepEqual(variants.variants[3].datasets, [
    "fleurs_vi",
    "speech_commands_v2",
    "musan_noise",
  ]);
  assert.equal(variants.variants[4].model_size, "large");
  for (const variant of variants.variants) {
    assert.match(variant.model_name, /^wake_liva_en_v3_[a-z_]+$/u);
    assert.equal(variant.requires_owner_hard_negatives_for_promotion, true);
  }
});

test("variant builder dùng hardlink corpus gốc và không inject public test", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "liva-wake-variants-"));
  const baseDir = join(tempRoot, "wake_liva_en_v2");
  const publicDir = join(tempRoot, "public");
  const ownerPositive = join(tempRoot, "owner-positive");
  const outputDir = join(tempRoot, "variants");
  const matrixPath = join(tempRoot, "variants.json");
  mkdirSync(ownerPositive, { recursive: true });

  try {
    for (const [split, marker] of [
      ["positive_train", 1],
      ["positive_test", 2],
      ["negative_train", 3],
      ["negative_test", 4],
      ["background_train", 5],
      ["background_test", 6],
    ]) {
      const directory = join(baseDir, split);
      mkdirSync(directory, { recursive: true });
      writeFileSync(join(directory, "clip_000000.wav"), makePcm16MonoWav(marker));
    }
    for (let index = 0; index < 5; index += 1) {
      writeFileSync(
        join(ownerPositive, `positive-${index}.wav`),
        makePcm16MonoWav(20 + index),
      );
    }

    const publicRecords = [];
    for (const [split, marker] of [
      ["train", 40],
      ["validation", 41],
      ["test", 42],
    ]) {
      const directory = join(publicDir, "fleurs_vi", split);
      mkdirSync(directory, { recursive: true });
      const path = join(directory, `${split}.wav`);
      writeFileSync(path, makePcm16MonoWav(marker));
      publicRecords.push({
        dataset_id: "fleurs_vi",
        split,
        path: `fleurs_vi/${split}/${split}.wav`,
        sha256: String(marker).padStart(64, "0"),
      });
    }
    writeFileSync(
      join(publicDir, "metadata.jsonl"),
      `${publicRecords.map((record) => JSON.stringify(record)).join("\n")}\n`,
    );
    writeFileSync(
      matrixPath,
      JSON.stringify({
        schema_version: 1,
        owner_minimum: 5,
        owner_train_copies: 8,
        owner_test_copies: 2,
        variants: [
          {
            id: "control_medium",
            model_name: "wake_liva_en_v3_control_medium",
            datasets: [],
            model_size: "medium",
            steps: 100,
            adversarial_negative_batch: 50,
            requires_owner_hard_negatives_for_promotion: true,
          },
          {
            id: "fleurs_medium",
            model_name: "wake_liva_en_v3_fleurs_medium",
            datasets: ["fleurs_vi"],
            model_size: "medium",
            steps: 100,
            adversarial_negative_batch: 50,
            requires_owner_hard_negatives_for_promotion: true,
          },
        ],
      }),
    );

    const result = spawnSync(
      process.execPath,
      [
        "scripts/prepare-wake-variants.mjs",
        "--matrix",
        matrixPath,
        "--base-model-dir",
        baseDir,
        "--public-corpus-dir",
        publicDir,
        "--owner-positive-dir",
        ownerPositive,
        "--output-dir",
        outputDir,
        "--config-template",
        "tools/wakeword/hey_liva_prod.yaml",
      ],
      {
        cwd: process.cwd(),
        encoding: "utf8",
        env: { ...process.env, LIVA_WAKE_FORCE_COPY: "1" },
      },
    );

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const hybridDir = join(outputDir, "wake_liva_en_v3_fleurs_medium");
    assert.ok(readdirSync(join(hybridDir, "negative_train")).includes("clip_600000.wav"));
    assert.ok(readdirSync(join(hybridDir, "negative_test")).includes("clip_700000.wav"));
    assert.equal(
      readdirSync(join(hybridDir, "negative_test")).includes("clip_700001.wav"),
      false,
    );
    assert.equal(
      readdirSync(join(outputDir, "wake_liva_en_v3_control_medium", "negative_train")).includes(
        "clip_600000.wav",
      ),
      false,
    );
    const buildManifest = JSON.parse(
      readFileSync(join(hybridDir, "variant-manifest.json"), "utf8"),
    );
    assert.equal(buildManifest.production_eligible, false);
    assert.equal(buildManifest.public_holdout_records, 1);
    assert.notEqual(
      statSync(join(baseDir, "positive_train", "clip_000000.wav")).ino,
      statSync(join(hybridDir, "positive_train", "clip_000000.wav")).ino,
    );
    assert.match(
      readFileSync(join(outputDir, "configs", "fleurs_medium.yaml"), "utf8"),
      /model_name: wake_liva_en_v3_fleurs_medium/u,
    );
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
});

test("selector chọn experimental winner nhưng chặn promotion khi thiếu owner negatives", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "liva-wake-selector-"));
  const reportsDir = join(tempRoot, "reports");
  const outputPath = join(tempRoot, "winner.json");
  mkdirSync(reportsDir, { recursive: true });

  try {
    for (const [id, recall, fpph, accepted] of [
      ["control_medium", 0.8, 0, false],
      ["hybrid_medium", 0.93, 0.5, true],
    ]) {
      writeFileSync(
        join(reportsDir, `${id}.json`),
        JSON.stringify({
          model_path: `${id}.onnx`,
          model_sha256: id.padEnd(64, "0"),
          metrics: {
            recall,
            false_positives_per_hour: fpph,
            negative_hours: 1.2,
            accepted,
          },
        }),
      );
      writeFileSync(
        join(reportsDir, `${id}.variant.json`),
        JSON.stringify({
          variant_id: id,
          owner_hard_negatives_present: false,
          public_holdout_records: 100,
        }),
      );
    }

    const result = spawnSync(
      process.execPath,
      [
        "scripts/select-wake-candidate.mjs",
        "--reports-dir",
        reportsDir,
        "--matrix",
        "tools/wakeword/variants.json",
        "--output",
        outputPath,
      ],
      { cwd: process.cwd(), encoding: "utf8" },
    );

    assert.equal(result.status, 2, result.stderr || result.stdout);
    const selection = JSON.parse(readFileSync(outputPath, "utf8"));
    assert.equal(selection.experimental_winner.variant_id, "hybrid_medium");
    assert.equal(selection.production_winner, null);
    assert.match(selection.blockers.join(" "), /owner hard-negative/u);
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
});

test("matrix runner pin datasets và không có đường promote model tự động", () => {
  const script = readFileSync("scripts/train-wakeword-matrix.ps1", "utf8");

  assert.match(
    script,
    /ValidateSet\("Doctor", "Install", "Fetch", "Prepare", "Augment", "Fit", "Train", "Benchmark", "Select", "Resume", "All"\)/u,
  );
  assert.match(script, /datasets==4\.8\.5/u);
  assert.match(script, /fetch-wake-public-corpus\.py/u);
  assert.match(script, /prepare-wake-variants\.mjs/u);
  assert.match(script, /select-wake-candidate\.mjs/u);
  assert.match(script, /wakeword_benchmark/u);
  assert.match(script, /"Augment"/u);
  assert.match(script, /"Fit"/u);
  assert.doesNotMatch(script, /Copy-Item[^\n]+models\\wake_liva/u);
});

test("Parquet adapter đọc theo batch từ cache cục bộ", () => {
  const tempRoot = mkdtempSync(join(tmpdir(), "liva-wake-parquet-"));
  const parquetRoot = join(tempRoot, "parquet");
  const datasetRoot = join(parquetRoot, "google", "fleurs", "vi_vn");
  const outputRoot = join(tempRoot, "output");
  const manifestPath = join(tempRoot, "manifest.json");
  const generatorPath = join(tempRoot, "make_parquet.py");
  mkdirSync(datasetRoot, { recursive: true });

  try {
    writeFileSync(
      generatorPath,
      `
import sys
from pathlib import Path
import pyarrow as pa
import pyarrow.parquet as pq

root = Path(sys.argv[1])
for index, split in enumerate(("train-a", "train-b", "validation", "test")):
    wav = (Path(sys.argv[2]) / f"{index}.wav").read_bytes()
    copies = 2 if split == "train-a" else 1
    table = pa.table({
        "audio": pa.array([{"bytes": wav, "path": f"{split}.wav"}] * copies, type=pa.struct([("bytes", pa.binary()), ("path", pa.string())])),
        "transcription": ["xin chao"] * copies,
        "id": [f"speaker-{split}-{copy}" for copy in range(copies)],
    })
    pq.write_table(table, root / f"{split}.parquet")
`,
    );
    const audioRoot = join(tempRoot, "audio");
    mkdirSync(audioRoot, { recursive: true });
    for (let index = 0; index < 4; index += 1) {
      writeFileSync(join(audioRoot, `${index}.wav`), makePcm16MonoWav(70 + index));
    }
    const generated = spawnSync(
      "tools/wakeword/venv/Scripts/python.exe",
      [generatorPath, datasetRoot, audioRoot],
      { cwd: process.cwd(), encoding: "utf8" },
    );
    assert.equal(generated.status, 0, generated.stderr || generated.stdout);
    writeFileSync(
      manifestPath,
      JSON.stringify({
        schema_version: 1,
        target_phrase: "hey liva",
        datasets: [
          {
            id: "fleurs_vi",
            repo_id: "google/fleurs",
            revision: "1".repeat(40),
            config: "vi_vn",
            format: "parquet_files",
            license: "cc-by-4.0",
            commercial_use: true,
            split_strategy: "native",
            text_field: "transcription",
            group_field: "id",
            parquet_files: {
              train: ["vi_vn/train-a.parquet", "vi_vn/train-b.parquet"],
              validation: ["vi_vn/validation.parquet"],
              test: ["vi_vn/test.parquet"],
            },
            max_samples: { train: 2, validation: 1, test: 1 },
            max_samples_per_file: { train: 1 },
          },
        ],
      }),
    );

    const result = spawnSync(
      "tools/wakeword/venv/Scripts/python.exe",
      [
        "scripts/fetch-wake-public-corpus.py",
        "--manifest",
        manifestPath,
        "--output",
        outputRoot,
        "--local-parquet-root",
        parquetRoot,
      ],
      { cwd: process.cwd(), encoding: "utf8" },
    );

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const corpusManifest = JSON.parse(
      readFileSync(join(outputRoot, "corpus-manifest.json"), "utf8"),
    );
    assert.equal(corpusManifest.accepted, 4);
    assert.equal(corpusManifest.mode, "huggingface-parquet");
  } finally {
    rmSync(tempRoot, { recursive: true, force: true });
  }
});
