#!/usr/bin/env python3
"""Fetch and normalize licensed public negatives for LIVA wake-word training."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import sys
import unicodedata
from pathlib import Path
from typing import Any, Iterable


ALLOWED_SPLITS = {"train", "validation", "test"}
ALLOWED_LICENSES = {"cc-by-4.0"}


class RejectedAudioSample(ValueError):
    """A source sample is valid input data but unusable for wake-word training."""


def fail(message: str) -> None:
    raise RuntimeError(message)


def normalize_text(value: object) -> str:
    decomposed = unicodedata.normalize("NFKD", str(value or "").lower())
    ascii_text = "".join(ch for ch in decomposed if not unicodedata.combining(ch))
    return " ".join("".join(ch if ch.isalnum() else " " for ch in ascii_text).split())


def canonical_wav(record: dict[str, Any]) -> bytes:
    try:
        import numpy as np
        import soundfile as sf
        import soxr
    except ImportError as error:
        fail(
            "audio dependencies are missing; run scripts/train-wakeword-matrix.ps1 -Action Install"
        )

    audio_bytes = record.get("audio_bytes")
    source: object
    if audio_bytes is not None:
        source = io.BytesIO(audio_bytes)
    else:
        path = Path(str(record.get("audio_path", "")))
        if not path.is_file():
            fail(f"{path}: audio file is missing")
        source = str(path)

    audio, sample_rate = sf.read(source, dtype="float32", always_2d=True)
    if audio.size == 0:
        raise RejectedAudioSample("empty audio sample")
    mono = np.mean(audio, axis=1, dtype=np.float32)
    if sample_rate != 16_000:
        mono = soxr.resample(mono, sample_rate, 16_000, quality="HQ")
    if not np.isfinite(mono).all():
        raise RejectedAudioSample("audio contains NaN or infinity")
    if mono.shape[0] < 8_000:
        raise RejectedAudioSample("audio duration must be at least 0.5 seconds")
    max_samples = 32_000
    if mono.shape[0] > max_samples:
        seed = hashlib.sha256(str(record.get("group", "")).encode("utf-8")).digest()
        offset = int.from_bytes(seed[:8], "big") % (mono.shape[0] - max_samples + 1)
        mono = mono[offset : offset + max_samples]
    mono = np.clip(mono, -1.0, 1.0)
    output = io.BytesIO()
    sf.write(output, mono, 16_000, format="WAV", subtype="PCM_16")
    return output.getvalue()


def fixture_records(root: Path) -> Iterable[dict[str, Any]]:
    records_path = root / "records.jsonl"
    if not records_path.is_file():
        fail(f"{records_path}: fixture records are missing")
    for line_number, raw in enumerate(records_path.read_text(encoding="utf-8").splitlines(), 1):
        if not raw.strip():
            continue
        record = json.loads(raw)
        record["audio_path"] = str((root / record["audio_path"]).resolve())
        record["fixture_line"] = line_number
        yield record


def assigned_split(dataset_id: str, group: str) -> str:
    bucket = int.from_bytes(
        hashlib.sha256(f"{dataset_id}:{group}".encode("utf-8")).digest()[:8], "big"
    ) % 100
    if bucket < 80:
        return "train"
    if bucket < 90:
        return "validation"
    return "test"


def parquet_urls(dataset: dict[str, Any], split: str) -> list[str]:
    prefix = str(dataset["path_prefix"]).strip("/")
    return [
        f"https://huggingface.co/datasets/{dataset['repo_id']}/resolve/"
        f"{dataset['revision']}/{prefix}/{name}"
        for name in dataset["parquet_files"][split]
    ]


def parquet_file_names(dataset: dict[str, Any], split: str) -> list[str]:
    prefix = str(dataset.get("path_prefix", "")).strip("/")
    return [
        f"{prefix}/{name}" if prefix else name
        for name in dataset["parquet_files"][split]
    ]


def parquet_records(
    dataset: dict[str, Any], local_root: Path | None
) -> Iterable[dict[str, Any]]:
    try:
        import pyarrow.parquet as pq
        from huggingface_hub import hf_hub_download
    except ImportError:
        fail(
            "Parquet dependencies are missing; run scripts/train-wakeword-matrix.ps1 -Action Install"
        )

    dataset_id = dataset["id"]
    budgets = dataset["max_samples"]
    counts = {split: 0 for split in ALLOWED_SPLITS}
    source_splits = (
        ALLOWED_SPLITS if dataset["split_strategy"] == "native" else {"train"}
    )
    columns = sorted(
        {"audio", dataset["text_field"], dataset["group_field"]}
    )

    for source_split in sorted(source_splits):
        for filename in parquet_file_names(dataset, source_split):
            if dataset["split_strategy"] == "native" and counts[source_split] >= int(
                budgets[source_split]
            ):
                break
            if local_root is not None:
                parquet_path = local_root / dataset["repo_id"] / filename
                if not parquet_path.is_file():
                    fail(f"{parquet_path}: local Parquet file is missing")
            else:
                print(
                    f"wake public corpus: downloading {dataset_id}/{filename}",
                    file=sys.stderr,
                    flush=True,
                )
                parquet_path = Path(
                    hf_hub_download(
                        repo_id=dataset["repo_id"],
                        filename=filename,
                        repo_type="dataset",
                        revision=dataset["revision"],
                    )
                )
            parquet = pq.ParquetFile(parquet_path)
            file_count = 0
            file_budget = int(
                dataset.get("max_samples_per_file", {}).get(source_split, 0)
            )
            for batch in parquet.iter_batches(batch_size=16, columns=columns):
                for item in batch.to_pylist():
                    if file_budget and file_count >= file_budget:
                        break
                    group = str(item.get(dataset["group_field"], "")).strip()
                    if not group:
                        fail(
                            f"{dataset_id}: missing group field {dataset['group_field']}"
                        )
                    split = (
                        source_split
                        if dataset["split_strategy"] == "native"
                        else assigned_split(dataset_id, group)
                    )
                    if counts[split] >= int(budgets[split]):
                        continue
                    audio = item.get("audio") or {}
                    counts[split] += 1
                    file_count += 1
                    yield {
                        "dataset_id": dataset_id,
                        "split": split,
                        "text": item.get(dataset["text_field"], ""),
                        "group": group,
                        "audio_path": audio.get("path"),
                        "audio_bytes": audio.get("bytes"),
                    }
                if all(counts[split] >= int(budgets[split]) for split in ALLOWED_SPLITS):
                    return
                if file_budget and file_count >= file_budget:
                    break
            print(
                f"wake public corpus: processed {dataset_id}/{filename} counts={counts}",
                file=sys.stderr,
                flush=True,
            )


def huggingface_records(
    manifest: dict[str, Any], local_parquet_root: Path | None = None
) -> Iterable[dict[str, Any]]:
    try:
        from datasets import Audio, load_dataset
    except ImportError as error:
        fail(
            "Hugging Face datasets is missing; run scripts/train-wakeword-matrix.ps1 -Action Install"
        )

    for dataset in manifest["datasets"]:
        if dataset["format"] == "parquet_files":
            yield from parquet_records(dataset, local_parquet_root)
            continue
        dataset_id = dataset["id"]
        budgets = dataset["max_samples"]
        if dataset["split_strategy"] == "native":
            source_splits = ALLOWED_SPLITS
        else:
            source_splits = {"train"}

        counts = {split: 0 for split in ALLOWED_SPLITS}
        for source_split in sorted(source_splits):
            if dataset["format"] == "parquet_revision":
                stream = load_dataset(
                    "parquet",
                    data_files={source_split: parquet_urls(dataset, source_split)},
                    split=source_split,
                    streaming=True,
                )
            else:
                stream = load_dataset(
                    dataset["repo_id"],
                    dataset.get("config"),
                    split=source_split,
                    revision=dataset["revision"],
                    streaming=True,
                )
            stream = stream.cast_column("audio", Audio(decode=False))
            for item in stream:
                group = str(item.get(dataset["group_field"], "")).strip()
                if not group:
                    fail(f"{dataset_id}: missing group field {dataset['group_field']}")
                split = (
                    source_split
                    if dataset["split_strategy"] == "native"
                    else assigned_split(dataset_id, group)
                )
                if counts[split] >= int(budgets[split]):
                    if dataset["split_strategy"] == "native":
                        break
                    continue
                audio = item.get("audio") or {}
                counts[split] += 1
                yield {
                    "dataset_id": dataset_id,
                    "split": split,
                    "text": item.get(dataset["text_field"], ""),
                    "group": group,
                    "audio_path": audio.get("path"),
                    "audio_bytes": audio.get("bytes"),
                }


def import_records(
    records: Iterable[dict[str, Any]],
    datasets: dict[str, dict[str, Any]],
    output: Path,
    target_phrase: str,
) -> dict[str, Any]:
    seen_audio: set[str] = set()
    group_splits: dict[tuple[str, str], str] = {}
    metadata: list[dict[str, Any]] = []
    counters = {
        "accepted": 0,
        "filtered_target_phrase": 0,
        "duplicates": 0,
        "rejected_audio": 0,
    }

    for record in records:
        dataset_id = str(record.get("dataset_id", ""))
        dataset = datasets.get(dataset_id)
        if dataset is None:
            fail(f"Unknown dataset_id: {dataset_id}")
        split = str(record.get("split", ""))
        if split not in ALLOWED_SPLITS:
            fail(f"{dataset_id}: invalid split {split!r}")
        group = str(record.get("group", "")).strip()
        if not group:
            fail(f"{dataset_id}: group is required")
        group_key = (dataset_id, group)
        previous_split = group_splits.setdefault(group_key, split)
        if previous_split != split:
            fail(f"{dataset_id}: group {group!r} leaks across {previous_split}/{split}")

        text = normalize_text(record.get("text", ""))
        if target_phrase and target_phrase in text:
            counters["filtered_target_phrase"] += 1
            continue

        try:
            wav_bytes = canonical_wav(record)
        except RejectedAudioSample as error:
            counters["rejected_audio"] += 1
            print(
                f"wake public corpus: rejected {dataset_id}/{group}: {error}",
                file=sys.stderr,
                flush=True,
            )
            continue
        audio_hash = hashlib.sha256(wav_bytes).hexdigest()
        if audio_hash in seen_audio:
            counters["duplicates"] += 1
            continue
        seen_audio.add(audio_hash)

        split_dir = output / dataset_id / split
        split_dir.mkdir(parents=True, exist_ok=True)
        destination = split_dir / f"{dataset_id}_{audio_hash[:20]}.wav"
        destination.write_bytes(wav_bytes)
        metadata.append(
            {
                "dataset_id": dataset_id,
                "repo_id": dataset["repo_id"],
                "revision": dataset["revision"],
                "license": dataset["license"],
                "split": split,
                "group": group,
                "text": text,
                "sha256": audio_hash,
                "path": destination.relative_to(output).as_posix(),
            }
        )
        counters["accepted"] += 1

    metadata.sort(key=lambda item: (item["dataset_id"], item["split"], item["sha256"]))
    (output / "metadata.jsonl").write_text(
        "".join(json.dumps(item, ensure_ascii=False, sort_keys=True) + "\n" for item in metadata),
        encoding="utf-8",
    )
    return {**counters, "records": metadata}


def load_manifest(path: Path) -> dict[str, Any]:
    manifest = json.loads(path.read_text(encoding="utf-8"))
    if manifest.get("schema_version") != 1:
        fail("Unsupported public dataset manifest schema")
    for dataset in manifest.get("datasets", []):
        if dataset.get("license") not in ALLOWED_LICENSES or not dataset.get("commercial_use"):
            fail(f"{dataset.get('id')}: license is not approved")
        revision = str(dataset.get("revision", ""))
        if len(revision) != 40 or any(ch not in "0123456789abcdef" for ch in revision):
            fail(f"{dataset.get('id')}: revision must be a pinned SHA")
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--fixture-root", type=Path)
    parser.add_argument("--local-parquet-root", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    manifest = load_manifest(args.manifest.resolve())
    datasets = {item["id"]: item for item in manifest["datasets"]}
    if args.dry_run:
        print(
            json.dumps(
                {
                    "mode": "dry-run",
                    "datasets": [
                        {
                            "id": item["id"],
                            "repo_id": item["repo_id"],
                            "revision": item["revision"],
                            "license": item["license"],
                            "max_samples": item["max_samples"],
                        }
                        for item in manifest["datasets"]
                    ],
                },
                sort_keys=True,
            )
        )
        return
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    mode = (
        "fixture"
        if args.fixture_root is not None
        else "huggingface-parquet"
        if args.local_parquet_root is not None
        else "huggingface"
    )
    records = (
        fixture_records(args.fixture_root.resolve())
        if args.fixture_root is not None
        else huggingface_records(
            manifest,
            args.local_parquet_root.resolve()
            if args.local_parquet_root is not None
            else None,
        )
    )
    result = import_records(
        records,
        datasets,
        output,
        normalize_text(manifest.get("target_phrase", "")),
    )
    corpus_manifest = {
        "schema_version": 1,
        "mode": mode,
        "source_manifest": str(args.manifest.resolve()),
        "datasets": manifest["datasets"],
        "accepted": result["accepted"],
        "filtered_target_phrase": result["filtered_target_phrase"],
        "duplicates": result["duplicates"],
        "rejected_audio": result["rejected_audio"],
    }
    (output / "corpus-manifest.json").write_text(
        json.dumps(corpus_manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(corpus_manifest, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        raise SystemExit(f"wake public corpus: FAIL - {error}") from error
