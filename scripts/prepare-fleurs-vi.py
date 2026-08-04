#!/usr/bin/env python3
"""Materialize a pinned FLEURS Vietnamese test subset as PCM16/16 kHz WAV.

Runtime dependencies (kept outside the LIVA product):
    py -m pip install datasets soundfile
"""

from __future__ import annotations

import argparse
import io
import json
import math
import wave
from pathlib import Path
from typing import Iterable, Mapping, Sequence


DATASET_ID = "google/fleurs"
DATASET_CONFIG = "vi_vn"
DATASET_SPLIT = "test"
DATASET_REVISION = "70bb2e84b976b7e960aa89f1c648e09c59f894dd"
SAMPLE_RATE = 16_000


def _decoded_audio(value: object) -> tuple[Sequence[float], int]:
    if isinstance(value, Mapping):
        if value.get("bytes") is not None or value.get("path"):
            source = io.BytesIO(value["bytes"]) if value.get("bytes") is not None else str(value["path"])
            try:
                import soundfile
            except ImportError as error:
                raise RuntimeError("undecoded FLEURS audio requires `py -m pip install soundfile`") from error
            samples, sample_rate = soundfile.read(source, dtype="float32", always_2d=False)
            if getattr(samples, "ndim", 1) != 1:
                raise ValueError("FLEURS source audio is not mono")
            samples = samples.tolist()
            return samples, sample_rate
        samples = value.get("array")
        sample_rate = value.get("sampling_rate")
    elif hasattr(value, "get_all_samples"):
        decoded = value.get_all_samples()
        samples = getattr(decoded, "data", None)
        sample_rate = getattr(decoded, "sample_rate", None)
    else:
        raise TypeError(f"unsupported audio value: {type(value).__name__}")

    if hasattr(samples, "detach"):
        samples = samples.detach().cpu()
    if hasattr(samples, "numpy"):
        samples = samples.numpy()
    if hasattr(samples, "tolist"):
        samples = samples.tolist()
    if samples and isinstance(samples[0], (list, tuple)):
        if len(samples) != 1:
            raise ValueError("FLEURS sample is not mono")
        samples = samples[0]
    if not isinstance(samples, Sequence) or not isinstance(sample_rate, int):
        raise TypeError("decoded audio is missing samples or sample rate")
    return samples, sample_rate


def _pcm16(samples: Sequence[float]) -> bytes:
    encoded = bytearray()
    for sample in samples:
        value = float(sample)
        if not math.isfinite(value):
            raise ValueError("audio contains a non-finite sample")
        integer = max(-32768, min(32767, round(max(-1.0, min(1.0, value)) * 32768.0)))
        encoded.extend(int(integer).to_bytes(2, "little", signed=True))
    return bytes(encoded)


def materialize(rows: Iterable[Mapping[str, object]], output: Path, limit: int) -> Path:
    if limit < 1:
        raise ValueError("limit must be positive")
    audio_dir = output / "audio"
    audio_dir.mkdir(parents=True, exist_ok=True)
    manifest = output / "fleurs-vi-test.jsonl"
    records: list[str] = []

    for index, row in enumerate(rows):
        if index >= limit:
            break
        samples, sample_rate = _decoded_audio(row["audio"])
        if sample_rate != SAMPLE_RATE:
            raise ValueError(
                f"sample {index} is {sample_rate} Hz; expected datasets Audio resampling to {SAMPLE_RATE} Hz"
            )
        transcript = str(row["transcription"]).strip()
        if not transcript:
            raise ValueError(f"sample {index} has an empty transcription")

        relative_audio = Path("audio") / f"{index:04d}.wav"
        with wave.open(str(output / relative_audio), "wb") as wav_file:
            wav_file.setnchannels(1)
            wav_file.setsampwidth(2)
            wav_file.setframerate(SAMPLE_RATE)
            wav_file.writeframes(_pcm16(samples))
        records.append(
            json.dumps(
                {"audio": relative_audio.as_posix(), "transcript": transcript},
                ensure_ascii=False,
            )
        )

    if len(records) < limit:
        raise ValueError(f"dataset returned only {len(records)} rows; requested {limit}")
    manifest.write_text("\n".join(records) + "\n", encoding="utf-8")
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=Path("data/benchmarks/fleurs-vi"))
    parser.add_argument("--limit", type=int, default=100)
    args = parser.parse_args()

    try:
        from datasets import Audio, load_dataset
    except ImportError as error:
        raise SystemExit('missing dependency: run `py -m pip install datasets soundfile`') from error

    rows = load_dataset(
        DATASET_ID,
        DATASET_CONFIG,
        split=DATASET_SPLIT,
        revision=DATASET_REVISION,
    ).cast_column("audio", Audio(decode=False))
    manifest = materialize(rows, args.output, args.limit)
    print(manifest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
