# Accuracy evidence

BLCVoice accuracy claims require explicit reference transcripts and hypotheses produced by a named engine/model/runtime. Deterministic generated audio used by latency benchmarks is not an accuracy corpus.

## Canonical scorer

Use the engine-neutral scorer in `blcvoice-asr`:

```bash
cargo run --release -p blcvoice-asr --example accuracy_score -- \
  --reference /path/to/reference.txt \
  --hypothesis /path/to/hypothesis.txt
```

Both UTF-8 files must contain the same number of lines. Each line is one aligned utterance. The scorer aggregates edit counts across the corpus and reports substitutions, deletions, insertions, total word errors and corpus word error rate (WER).

The output begins with `format=blcvoice-accuracy-v1` and records `normalization=unicode-whitespace-verbatim-v1`. That policy is intentionally conservative: Unicode whitespace defines word boundaries, while case, punctuation and token text are otherwise preserved verbatim. The scorer does not silently lowercase, strip punctuation, expand contractions or apply language-specific rewriting. If a future evaluation needs another normalization policy, it must use a separately named/versioned policy so results cannot be compared under hidden preprocessing differences.

An empty reference corpus with a non-empty hypothesis does not produce a misleading zero WER; the scorer emits an explicit undefined value while still reporting insertion counts.

## Evidence bundle

A saved accuracy result is interpretable only when kept with:

- the exact repository commit and scorer format/normalization version;
- the exact reference and hypothesis files, or immutable identifiers for them;
- corpus provenance, language(s), licensing/redistribution constraints and any exclusions;
- ASR engine and adapter version;
- exact model identity/artifact version;
- backend/acceleration and relevant decode/runtime settings;
- operating system, architecture and hardware context when the hypothesis was generated.

Do not commit private user audio or transcripts to the repository. Public benchmark corpora must be reviewed for redistribution/license constraints before adding any dataset artifact.

## Interpretation

WER is computed as `(substitutions + deletions + insertions) / reference_words`. Lower is better, but values are only comparable when corpus, alignment and normalization policy are equivalent. Accuracy and latency/resource measurements are separate evidence classes; a faster benchmark run does not imply better recognition quality, and a WER result does not prove desktop-session compatibility.

The scorer follows the standard reference-versus-hypothesis edit-accounting model used by established ASR evaluation tooling such as NIST sclite and JiWER. BLCVoice keeps the implementation dependency-free and benchmark-only so evaluation policy does not become production recognition business logic.
