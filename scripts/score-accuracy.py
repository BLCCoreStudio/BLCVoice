#!/usr/bin/env python3
"""Score ASR hypotheses against references without hiding normalization policy.

Input is JSON Lines. Each non-empty line must contain string fields:
  {"id": "sample-1", "reference": "...", "hypothesis": "..."}

The scorer applies Unicode NFC and whitespace tokenization only. Case and
punctuation remain significant so benchmark evidence cannot silently improve
through locale- or dataset-specific normalization.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence

FORMAT_VERSION = "blcvoice-accuracy-v1"
NORMALIZATION = "unicode-nfc;case-sensitive;punctuation-sensitive;whitespace-word-tokenization"


@dataclass(frozen=True)
class Sample:
    sample_id: str
    reference: str
    hypothesis: str


@dataclass(frozen=True)
class Score:
    samples: int
    reference_words: int
    word_errors: int
    reference_chars: int
    char_errors: int

    @property
    def wer(self) -> float:
        return ratio(self.word_errors, self.reference_words)

    @property
    def cer(self) -> float:
        return ratio(self.char_errors, self.reference_chars)


def normalize(text: str) -> str:
    return unicodedata.normalize("NFC", text)


def words(text: str) -> list[str]:
    return normalize(text).split()


def chars(text: str) -> list[str]:
    return list(normalize(text))


def edit_distance(reference: Sequence[str], hypothesis: Sequence[str]) -> int:
    if len(reference) < len(hypothesis):
        reference, hypothesis = hypothesis, reference

    previous = list(range(len(hypothesis) + 1))
    for ref_index, ref_item in enumerate(reference, start=1):
        current = [ref_index]
        for hyp_index, hyp_item in enumerate(hypothesis, start=1):
            substitution = previous[hyp_index - 1] + (ref_item != hyp_item)
            deletion = previous[hyp_index] + 1
            insertion = current[hyp_index - 1] + 1
            current.append(min(substitution, deletion, insertion))
        previous = current
    return previous[-1]


def ratio(errors: int, reference_units: int) -> float:
    if reference_units == 0:
        return 0.0 if errors == 0 else math.inf
    return errors / reference_units


def score(samples: Iterable[Sample]) -> Score:
    sample_count = 0
    reference_words = 0
    word_errors = 0
    reference_chars = 0
    char_errors = 0

    for sample in samples:
        sample_count += 1
        ref_words = words(sample.reference)
        hyp_words = words(sample.hypothesis)
        ref_chars = chars(sample.reference)
        hyp_chars = chars(sample.hypothesis)

        reference_words += len(ref_words)
        word_errors += edit_distance(ref_words, hyp_words)
        reference_chars += len(ref_chars)
        char_errors += edit_distance(ref_chars, hyp_chars)

    if sample_count == 0:
        raise ValueError("accuracy input contains no samples")

    return Score(
        samples=sample_count,
        reference_words=reference_words,
        word_errors=word_errors,
        reference_chars=reference_chars,
        char_errors=char_errors,
    )


def load_jsonl(path: Path) -> list[Sample]:
    samples: list[Sample] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, raw_line in enumerate(handle, start=1):
            if not raw_line.strip():
                continue
            try:
                record = json.loads(raw_line)
            except json.JSONDecodeError as error:
                raise ValueError(f"line {line_number}: invalid JSON: {error.msg}") from error

            if not isinstance(record, dict):
                raise ValueError(f"line {line_number}: expected a JSON object")
            sample_id = record.get("id")
            reference = record.get("reference")
            hypothesis = record.get("hypothesis")
            if not all(isinstance(value, str) for value in (sample_id, reference, hypothesis)):
                raise ValueError(
                    f"line {line_number}: id, reference and hypothesis must be strings"
                )
            samples.append(Sample(sample_id, reference, hypothesis))
    return samples


def finite_metric(value: float) -> str:
    return "inf" if math.isinf(value) else f"{value:.6f}"


def print_score(result: Score) -> None:
    print(f"format={FORMAT_VERSION}")
    print(f"normalization={NORMALIZATION}")
    print(f"samples={result.samples}")
    print(f"reference_words={result.reference_words}")
    print(f"word_errors={result.word_errors}")
    print(f"wer={finite_metric(result.wer)}")
    print(f"reference_chars={result.reference_chars}")
    print(f"char_errors={result.char_errors}")
    print(f"cer={finite_metric(result.cer)}")


def self_test() -> None:
    assert edit_distance([], []) == 0
    assert edit_distance(["a"], []) == 1
    assert edit_distance([], ["a"]) == 1
    assert edit_distance(["kitten"], ["sitting"]) == 1
    assert edit_distance(list("kitten"), list("sitting")) == 3
    assert normalize("I\u0307") == "İ"

    result = score(
        [
            Sample("exact", "merhaba dünya", "merhaba dünya"),
            Sample("substitution", "nasılsın bugün", "nasılsın yarın"),
        ]
    )
    assert result.samples == 2
    assert result.reference_words == 4
    assert result.word_errors == 1
    assert result.wer == 0.25
    assert result.char_errors > 0
    print("accuracy_scorer_self_test=ok")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", nargs="?", type=Path, help="JSONL reference/hypothesis file")
    parser.add_argument("--self-test", action="store_true", help="run deterministic scorer tests")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.input is None:
        print("input JSONL path is required unless --self-test is used", file=sys.stderr)
        return 2

    try:
        result = score(load_jsonl(args.input))
    except (OSError, ValueError) as error:
        print(f"accuracy scoring failed: {error}", file=sys.stderr)
        return 1

    print_score(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
