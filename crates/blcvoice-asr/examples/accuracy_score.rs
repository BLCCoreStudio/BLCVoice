#![forbid(unsafe_code)]

use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

const FORMAT_VERSION: &str = "blcvoice-accuracy-v1";
const NORMALIZATION_POLICY: &str = "unicode-whitespace-verbatim-v1";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct EditCounts {
    substitutions: usize,
    deletions: usize,
    insertions: usize,
}

impl EditCounts {
    fn errors(self) -> usize {
        self.substitutions + self.deletions + self.insertions
    }
}

#[derive(Debug)]
struct Args {
    reference: PathBuf,
    hypothesis: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let reference_text = fs::read_to_string(&args.reference)?;
    let hypothesis_text = fs::read_to_string(&args.hypothesis)?;

    let reference_lines: Vec<&str> = reference_text.lines().collect();
    let hypothesis_lines: Vec<&str> = hypothesis_text.lines().collect();
    if reference_lines.len() != hypothesis_lines.len() {
        return Err(format!(
            "reference/hypothesis line count mismatch: {} != {}",
            reference_lines.len(),
            hypothesis_lines.len()
        )
        .into());
    }

    let mut totals = EditCounts::default();
    let mut reference_words = 0usize;
    let mut hypothesis_words = 0usize;

    for (reference, hypothesis) in reference_lines.iter().zip(&hypothesis_lines) {
        let reference_tokens: Vec<&str> = reference.split_whitespace().collect();
        let hypothesis_tokens: Vec<&str> = hypothesis.split_whitespace().collect();
        reference_words += reference_tokens.len();
        hypothesis_words += hypothesis_tokens.len();

        let counts = align_words(&reference_tokens, &hypothesis_tokens);
        totals.substitutions += counts.substitutions;
        totals.deletions += counts.deletions;
        totals.insertions += counts.insertions;
    }

    let errors = totals.errors();
    let wer = if reference_words == 0 {
        if hypothesis_words == 0 { 0.0 } else { f64::INFINITY }
    } else {
        errors as f64 / reference_words as f64
    };

    println!("format={FORMAT_VERSION}");
    println!("normalization={NORMALIZATION_POLICY}");
    println!("reference_path={}", args.reference.display());
    println!("hypothesis_path={}", args.hypothesis.display());
    println!("utterances={}", reference_lines.len());
    println!("reference_words={reference_words}");
    println!("hypothesis_words={hypothesis_words}");
    println!("substitutions={}", totals.substitutions);
    println!("deletions={}", totals.deletions);
    println!("insertions={}", totals.insertions);
    println!("word_errors={errors}");
    if wer.is_finite() {
        println!("wer={wer:.8}");
    } else {
        println!("wer=undefined_nonempty_hypothesis_with_empty_reference");
    }

    Ok(())
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut reference = None;
    let mut hypothesis = None;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--reference" => reference = args.next().map(PathBuf::from),
            "--hypothesis" => hypothesis = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                println!("usage: accuracy_score --reference FILE --hypothesis FILE");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    Ok(Args {
        reference: reference.ok_or("missing --reference FILE")?,
        hypothesis: hypothesis.ok_or("missing --hypothesis FILE")?,
    })
}

fn align_words(reference: &[&str], hypothesis: &[&str]) -> EditCounts {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Cell {
        distance: usize,
        counts: EditCounts,
    }

    let mut previous = Vec::with_capacity(hypothesis.len() + 1);
    previous.push(Cell {
        distance: 0,
        counts: EditCounts::default(),
    });
    for index in 1..=hypothesis.len() {
        previous.push(Cell {
            distance: index,
            counts: EditCounts {
                insertions: index,
                ..EditCounts::default()
            },
        });
    }

    for (reference_index, reference_word) in reference.iter().enumerate() {
        let mut current = Vec::with_capacity(hypothesis.len() + 1);
        current.push(Cell {
            distance: reference_index + 1,
            counts: EditCounts {
                deletions: reference_index + 1,
                ..EditCounts::default()
            },
        });

        for (hypothesis_index, hypothesis_word) in hypothesis.iter().enumerate() {
            if reference_word == hypothesis_word {
                current.push(previous[hypothesis_index]);
                continue;
            }

            let substitution = with_substitution(previous[hypothesis_index]);
            let deletion = with_deletion(previous[hypothesis_index + 1]);
            let insertion = with_insertion(current[hypothesis_index]);
            current.push(best_cell(substitution, deletion, insertion));
        }

        previous = current;
    }

    previous[hypothesis.len()].counts
}

fn with_substitution(mut cell: CellLike) -> CellLike {
    cell.distance += 1;
    cell.counts.substitutions += 1;
    cell
}

fn with_deletion(mut cell: CellLike) -> CellLike {
    cell.distance += 1;
    cell.counts.deletions += 1;
    cell
}

fn with_insertion(mut cell: CellLike) -> CellLike {
    cell.distance += 1;
    cell.counts.insertions += 1;
    cell
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellLike {
    distance: usize,
    counts: EditCounts,
}

impl From<CellLike> for CellLike {
    fn from(value: CellLike) -> Self {
        value
    }
}

fn best_cell(substitution: impl Into<CellLike>, deletion: impl Into<CellLike>, insertion: impl Into<CellLike>) -> CellLike {
    let substitution = substitution.into();
    let deletion = deletion.into();
    let insertion = insertion.into();
    [substitution, deletion, insertion]
        .into_iter()
        .min_by_key(|cell| (cell.distance, cell.counts.substitutions, cell.counts.deletions, cell.counts.insertions))
        .expect("three candidates")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_identity_as_zero_errors() {
        let counts = align_words(&["hello", "world"], &["hello", "world"]);
        assert_eq!(counts, EditCounts::default());
    }

    #[test]
    fn distinguishes_substitution_deletion_and_insertion() {
        assert_eq!(
            align_words(&["a", "b", "c"], &["a", "x", "c"]),
            EditCounts {
                substitutions: 1,
                deletions: 0,
                insertions: 0,
            }
        );
        assert_eq!(
            align_words(&["a", "b", "c"], &["a", "c"]),
            EditCounts {
                substitutions: 0,
                deletions: 1,
                insertions: 0,
            }
        );
        assert_eq!(
            align_words(&["a", "c"], &["a", "b", "c"]),
            EditCounts {
                substitutions: 0,
                deletions: 0,
                insertions: 1,
            }
        );
    }

    #[test]
    fn empty_reference_is_not_reported_as_zero_when_hypothesis_has_words() {
        let counts = align_words(&[], &["unexpected"]);
        assert_eq!(counts.insertions, 1);
    }
}
