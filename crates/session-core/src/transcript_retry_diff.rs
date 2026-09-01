//! Word-level diff between a retained transcript and a retry candidate.
//!
//! D6 (design intake): the keep-or-promote decision must show *what* differs
//! between the two transcripts, not only that a retry exists. This module is
//! the pure computation behind that requirement — no I/O, no dependency on
//! the desktop crate's `TranscriptTurn`, no third-party diff crate.
//!
//! ## Tokenization contract
//!
//! A "word" is a maximal run of non-whitespace characters, exactly what
//! [`str::split_whitespace`] yields. The renderer (`view-model.mjs`) must
//! tokenize the same turn text the same way — splitting on runs of Unicode
//! whitespace and counting only the non-empty runs — so that a `WordSpan`'s
//! `start_word`/`end_word` indices line up with what it re-derives from the
//! original text. Nothing is normalized: case, punctuation, and spacing are
//! compared exactly as transcribed, so a punctuation-only edit still counts
//! as a difference.
//!
//! ## Alignment across non-1:1 turns
//!
//! A retry is a fresh transcription pass, so turn boundaries do not
//! necessarily line up between the two sides. The diff does not attempt to
//! align turns; it concatenates each side's visible words into one sequence,
//! diffs the two sequences, then maps every differing word back to its
//! source `(turn_index, word_index)` to build per-turn spans. A withheld
//! turn contributes exactly one opaque boundary token to its side's
//! sequence — its text never enters the diff, and the boundary token itself
//! never produces a span. The token is unique per occurrence, so it can
//! never appear to "match" anything (including another withheld turn's
//! token); it always surfaces as a same-side-only op, which this module
//! discards before building spans.
//!
//! ## Bounds
//!
//! The comparison is capped two ways, either of which produces
//! [`TranscriptRetryDiffState::Skipped`] with no spans: either side's word
//! count beyond [`MAX_DIFFABLE_WORDS`], or the Myers edit-script search
//! exceeding [`MAX_EDIT_BUDGET`] iterations. A skip never blocks or delays
//! the keep/promote decision — the caller renders today's comparison with a
//! single plain sentence explaining that highlighting was skipped, instead of
//! spans. Absence of spans must never be read as "identical" by a caller
//! that cannot tell skip and computed-identical apart, which is why they are
//! distinct states.

use std::collections::BTreeMap;

/// One transcript turn as the diff needs it. Callers own the real turn
/// representation (Rust or otherwise); this is the minimal projection.
#[derive(Debug, Clone, Copy)]
pub struct DiffTurnInput<'a> {
    pub withheld: bool,
    pub text: &'a str,
}

impl<'a> DiffTurnInput<'a> {
    pub fn visible(text: &'a str) -> Self {
        Self {
            withheld: false,
            text,
        }
    }

    pub fn withheld() -> Self {
        Self {
            withheld: true,
            text: "",
        }
    }
}

/// Either side's total visible word count crossing this line skips
/// highlighting entirely. Chosen to keep the O((N+M) * MAX_EDIT_BUDGET) Myers
/// search bounded even for a long meeting compared against an equally long
/// retry.
pub const MAX_DIFFABLE_WORDS: usize = 20_000;

/// The Myers search gives up after this many rounds (the shortest-edit-script
/// length it is willing to consider) rather than let a pathologically
/// different pair of transcripts turn an O(ND) search into an unbounded one.
/// A real retry of the same meeting is mostly the same words in the same
/// order, so D stays small in the case this feature exists for; this budget
/// only ever bites on two transcripts that share almost nothing.
pub const MAX_EDIT_BUDGET: usize = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptRetryDiffState {
    /// The diff ran to completion. `current`/`candidate` spans may still both
    /// be empty — that means the sides were checked and found identical
    /// (aside from any withheld turns, which are never part of the check).
    Computed,
    /// One of the bounds above was hit. No spans were produced on either
    /// side, and this is *not* evidence the sides are identical.
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordSpan {
    pub start_word: u32,
    pub end_word: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnDiffSpans {
    pub turn_index: u32,
    /// This turn's total word count, exactly as `diff_transcript_turns`
    /// counted it (`str::split_whitespace().count()`). The renderer must
    /// tokenize the same turn text into the same number of words before it
    /// trusts `spans`'s word indices against that text — see the module docs'
    /// tokenization contract. A mismatch means the renderer's tokenizer
    /// disagreed with this one (for example on an unusual whitespace
    /// character), and the safe response is to render that turn's text plain
    /// rather than risk a span landing on the wrong word.
    pub word_count: u32,
    pub spans: Vec<WordSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRetryDiff {
    pub state: TranscriptRetryDiffState,
    pub current: Vec<TurnDiffSpans>,
    pub candidate: Vec<TurnDiffSpans>,
}

impl TranscriptRetryDiff {
    fn skipped() -> Self {
        Self {
            state: TranscriptRetryDiffState::Skipped,
            current: Vec::new(),
            candidate: Vec::new(),
        }
    }
}

/// Compares the current transcript's visible turns against a retry
/// candidate's visible turns and returns per-side word-level highlight spans.
pub fn diff_transcript_turns(
    current: &[DiffTurnInput],
    candidate: &[DiffTurnInput],
) -> TranscriptRetryDiff {
    let (current_tokens, current_positions) = build_sequence(current);
    let (candidate_tokens, candidate_positions) = build_sequence(candidate);

    if current_tokens.len() > MAX_DIFFABLE_WORDS || candidate_tokens.len() > MAX_DIFFABLE_WORDS {
        return TranscriptRetryDiff::skipped();
    }

    let offset = diff_offset(
        current_tokens.len(),
        candidate_tokens.len(),
        MAX_EDIT_BUDGET,
    );
    let trace = match myers_trace(&current_tokens, &candidate_tokens, MAX_EDIT_BUDGET, offset) {
        Some(trace) => trace,
        None => return TranscriptRetryDiff::skipped(),
    };

    let ops = backtrack(
        current_tokens.len() as isize,
        candidate_tokens.len() as isize,
        &trace,
        offset,
    );

    let mut current_flagged = Vec::new();
    let mut candidate_flagged = Vec::new();
    for op in ops {
        match op {
            Op::Delete(a_idx) => current_flagged.push(a_idx),
            Op::Insert(b_idx) => candidate_flagged.push(b_idx),
            Op::Keep => {}
        }
    }

    TranscriptRetryDiff {
        state: TranscriptRetryDiffState::Computed,
        current: turn_diff_entries(current, &current_positions, &current_flagged),
        candidate: turn_diff_entries(candidate, &candidate_positions, &candidate_flagged),
    }
}

/// Builds one entry per visible turn (word count plus any diff spans),
/// including turns with no differences at all — the renderer needs every
/// visible turn's word count to check its own tokenizer against this one,
/// not just the turns that turned out to differ.
fn turn_diff_entries(
    turns: &[DiffTurnInput],
    positions: &[Position],
    flagged: &[usize],
) -> Vec<TurnDiffSpans> {
    let mut spans_by_turn = spans_map_from_flags(positions, flagged);
    turns
        .iter()
        .enumerate()
        .filter(|(_, turn)| !turn.withheld)
        .map(|(turn_index, turn)| {
            let turn_index = turn_index as u32;
            TurnDiffSpans {
                turn_index,
                word_count: turn.text.split_whitespace().count() as u32,
                spans: spans_by_turn.remove(&turn_index).unwrap_or_default(),
            }
        })
        .collect()
}

// --- token sequence construction -------------------------------------------------

#[derive(Clone, Copy)]
enum Token<'a> {
    Word(&'a str),
    /// A withheld turn's stand-in. Every occurrence compares unequal to every
    /// other token, including another `Boundary` — see the `PartialEq` impl.
    Boundary,
}

impl<'a> PartialEq for Token<'a> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Token::Word(a), Token::Word(b)) => a == b,
            // A boundary token never equals anything, including another
            // boundary token: withheld content must never be treated as
            // matching, and must never anchor an alignment across sides.
            _ => false,
        }
    }
}

#[derive(Clone, Copy)]
enum Position {
    Word { turn_index: u32, word_index: u32 },
    Boundary,
}

fn build_sequence<'a>(turns: &[DiffTurnInput<'a>]) -> (Vec<Token<'a>>, Vec<Position>) {
    let mut tokens = Vec::new();
    let mut positions = Vec::new();
    for (turn_index, turn) in turns.iter().enumerate() {
        if turn.withheld {
            tokens.push(Token::Boundary);
            positions.push(Position::Boundary);
            continue;
        }
        for (word_index, word) in turn.text.split_whitespace().enumerate() {
            tokens.push(Token::Word(word));
            positions.push(Position::Word {
                turn_index: turn_index as u32,
                word_index: word_index as u32,
            });
        }
    }
    (tokens, positions)
}

// --- Myers shortest-edit-script diff ----------------------------------------------

enum Op {
    Keep,
    Insert(usize),
    Delete(usize),
}

fn diff_offset(n: usize, m: usize, max_d: usize) -> isize {
    // Sized to the *bound*, not the raw sequence length: `k` never ranges
    // beyond +/- min(max_d, n+m) rounds, so an offset any larger only wastes
    // memory the budget exists to avoid spending. +1 is headroom so the
    // forward pass's `idx + 1` probe at the extreme k values never reads
    // past the end of the `v` array.
    (max_d.min(n + m)) as isize + 1
}

/// Returns the sequence of `v` snapshots (one per round, taken *before* that
/// round's updates) needed to backtrack an edit script, or `None` if no
/// script within `max_d` rounds reaches the end of both sequences.
fn myers_trace(a: &[Token], b: &[Token], max_d: usize, offset: isize) -> Option<Vec<Vec<isize>>> {
    let n = a.len() as isize;
    let m = b.len() as isize;
    let width = (2 * offset + 1) as usize;
    let bound = (max_d as isize).min(n + m);

    let mut v = vec![0isize; width];
    let mut trace = Vec::with_capacity((bound + 1) as usize);

    for d in 0..=bound {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let idx = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[idx - 1] < v[idx + 1]) {
                v[idx + 1]
            } else {
                v[idx - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx] = x;
            if x >= n && y >= m {
                return Some(trace);
            }
            k += 2;
        }
    }
    None
}

fn backtrack(a_len: isize, b_len: isize, trace: &[Vec<isize>], offset: isize) -> Vec<Op> {
    let mut x = a_len;
    let mut y = b_len;
    let mut ops = Vec::new();

    for d in (0..trace.len()).rev() {
        let d = d as isize;
        let v = &trace[d as usize];
        let k = x - y;
        let idx = (k + offset) as usize;
        let prev_k = if k == -d || (k != d && v[idx - 1] < v[idx + 1]) {
            k + 1
        } else {
            k - 1
        };
        let prev_idx = (prev_k + offset) as usize;
        let prev_x = v[prev_idx];
        let prev_y = prev_x - prev_k;

        while x > prev_x && y > prev_y {
            ops.push(Op::Keep);
            x -= 1;
            y -= 1;
        }

        if d > 0 {
            if x == prev_x {
                ops.push(Op::Insert((y - 1) as usize));
            } else {
                ops.push(Op::Delete((x - 1) as usize));
            }
        }

        x = prev_x;
        y = prev_y;
    }

    ops.reverse();
    ops
}

// --- span reconstruction -----------------------------------------------------------

fn spans_map_from_flags(positions: &[Position], flagged: &[usize]) -> BTreeMap<u32, Vec<WordSpan>> {
    let mut result: BTreeMap<u32, Vec<WordSpan>> = BTreeMap::new();
    let mut open: Option<(u32, u32, u32)> = None; // (turn_index, start_word, end_word)

    for &i in flagged {
        match positions[i] {
            Position::Boundary => {
                if let Some((turn_index, start_word, end_word)) = open.take() {
                    push_span(&mut result, turn_index, start_word, end_word);
                }
            }
            Position::Word {
                turn_index,
                word_index,
            } => {
                let continues =
                    matches!(open, Some((t, _, end)) if t == turn_index && end == word_index);
                if continues {
                    if let Some((t, start, _)) = open {
                        open = Some((t, start, word_index + 1));
                    }
                } else {
                    if let Some((turn_index, start_word, end_word)) = open.take() {
                        push_span(&mut result, turn_index, start_word, end_word);
                    }
                    open = Some((turn_index, word_index, word_index + 1));
                }
            }
        }
    }
    if let Some((turn_index, start_word, end_word)) = open.take() {
        push_span(&mut result, turn_index, start_word, end_word);
    }
    result
}

fn push_span(
    result: &mut BTreeMap<u32, Vec<WordSpan>>,
    turn_index: u32,
    start_word: u32,
    end_word: u32,
) {
    result.entry(turn_index).or_default().push(WordSpan {
        start_word,
        end_word,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side_turns<'a>(diff: &'a TranscriptRetryDiff, side: &str) -> &'a [TurnDiffSpans] {
        if side == "current" {
            &diff.current
        } else {
            &diff.candidate
        }
    }

    fn spans_for(diff: &TranscriptRetryDiff, side: &str, turn_index: u32) -> Vec<(u32, u32)> {
        side_turns(diff, side)
            .iter()
            .find(|t| t.turn_index == turn_index)
            .map(|t| t.spans.iter().map(|s| (s.start_word, s.end_word)).collect())
            .unwrap_or_default()
    }

    fn word_count_for(diff: &TranscriptRetryDiff, side: &str, turn_index: u32) -> Option<u32> {
        side_turns(diff, side)
            .iter()
            .find(|t| t.turn_index == turn_index)
            .map(|t| t.word_count)
    }

    // Every visible turn gets an entry (so the renderer always has a word
    // count to check its own tokenizer against, even when nothing differs);
    // "no differences" means every entry's spans are empty, not that the
    // side's Vec itself is empty.
    fn has_any_span(diff: &TranscriptRetryDiff, side: &str) -> bool {
        side_turns(diff, side).iter().any(|t| !t.spans.is_empty())
    }

    #[test]
    fn identical_sides_produce_no_spans() {
        let current = [DiffTurnInput::visible("we shipped the retry diff today")];
        let candidate = [DiffTurnInput::visible("we shipped the retry diff today")];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        assert!(!has_any_span(&diff, "current"));
        assert!(!has_any_span(&diff, "candidate"));
        // The turn still gets an entry — just with an empty spans list — so
        // the renderer has a word count to check its own tokenizer against.
        assert_eq!(word_count_for(&diff, "current", 0), Some(6));
        assert_eq!(word_count_for(&diff, "candidate", 0), Some(6));
    }

    #[test]
    fn insertion_is_attributed_only_to_the_candidate() {
        let current = [DiffTurnInput::visible("ship the diff")];
        let candidate = [DiffTurnInput::visible("ship the word diff")];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        assert!(
            !has_any_span(&diff, "current"),
            "nothing was removed from current"
        );
        assert_eq!(spans_for(&diff, "candidate", 0), vec![(2, 3)]);
        assert_eq!(word_count_for(&diff, "current", 0), Some(3));
        assert_eq!(word_count_for(&diff, "candidate", 0), Some(4));
    }

    #[test]
    fn deletion_is_attributed_only_to_the_current_side() {
        let current = [DiffTurnInput::visible("ship the word diff")];
        let candidate = [DiffTurnInput::visible("ship the diff")];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        assert_eq!(spans_for(&diff, "current", 0), vec![(2, 3)]);
        assert!(
            !has_any_span(&diff, "candidate"),
            "nothing was added to candidate"
        );
    }

    #[test]
    fn substitution_marks_both_sides_at_the_replaced_word() {
        let current = [DiffTurnInput::visible("the quarterly review starts monday")];
        let candidate = [DiffTurnInput::visible(
            "the quarterly review starts tuesday",
        )];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        assert_eq!(spans_for(&diff, "current", 0), vec![(4, 5)]);
        assert_eq!(spans_for(&diff, "candidate", 0), vec![(4, 5)]);
    }

    #[test]
    fn substitution_maps_back_to_the_correct_turn_when_turns_do_not_align() {
        // The candidate has an extra turn ahead of the substitution, so turn
        // counts differ (a fresh transcription pass, not a 1:1 replay). Every
        // word is distinct across turns so there is exactly one shortest edit
        // script — an LCS-based diff is not required to prefer the
        // "semantically obvious" alignment when more than one equally short
        // script exists, so a fair test avoids repeated tokens that would
        // make more than one alignment optimal.
        let current = [
            DiffTurnInput::visible("alpha bravo charlie"),
            DiffTurnInput::visible("delta echo starts monday"),
        ];
        let candidate = [
            DiffTurnInput::visible("zulu yankee whiskey victor"),
            DiffTurnInput::visible("alpha bravo charlie"),
            DiffTurnInput::visible("delta echo starts tuesday"),
        ];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        assert!(spans_for(&diff, "current", 0).is_empty());
        assert_eq!(spans_for(&diff, "current", 1), vec![(3, 4)]);
        // The whole extra candidate turn is candidate-only.
        assert_eq!(spans_for(&diff, "candidate", 0), vec![(0, 4)]);
        assert!(spans_for(&diff, "candidate", 1).is_empty());
        assert_eq!(spans_for(&diff, "candidate", 2), vec![(3, 4)]);
    }

    #[test]
    fn withheld_turns_are_excluded_and_never_diffed() {
        let current = [
            DiffTurnInput::visible("kept turn one"),
            DiffTurnInput::withheld(),
            DiffTurnInput::visible("kept turn two"),
        ];
        let candidate = [
            DiffTurnInput::visible("kept turn one"),
            DiffTurnInput::withheld(),
            DiffTurnInput::visible("kept turn two"),
        ];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        // The withheld turn (index 1) never gets an entry at all — not even
        // an empty one — and the two visible turns otherwise read identical.
        assert!(diff.current.iter().all(|t| t.turn_index != 1));
        assert!(diff.candidate.iter().all(|t| t.turn_index != 1));
        assert!(!has_any_span(&diff, "current"));
        assert!(!has_any_span(&diff, "candidate"));
    }

    #[test]
    fn a_withheld_turn_never_falsely_matches_another_withheld_turn() {
        // Two withheld turns, at different positions on each side, must not
        // let the aligner treat them as a matching anchor and skip content
        // around them.
        let current = [
            DiffTurnInput::withheld(),
            DiffTurnInput::visible("alpha bravo"),
        ];
        let candidate = [
            DiffTurnInput::visible("alpha bravo"),
            DiffTurnInput::withheld(),
        ];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        // The visible words match exactly regardless of the withheld turn's
        // position; the boundary tokens produce no spans.
        assert!(!has_any_span(&diff, "current"));
        assert!(!has_any_span(&diff, "candidate"));
    }

    #[test]
    fn punctuation_only_difference_is_detected() {
        let current = [DiffTurnInput::visible("are we done")];
        let candidate = [DiffTurnInput::visible("are we done?")];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        assert_eq!(spans_for(&diff, "current", 0), vec![(2, 3)]);
        assert_eq!(spans_for(&diff, "candidate", 0), vec![(2, 3)]);
    }

    #[test]
    fn over_budget_word_count_skips_without_spans() {
        let long_current = "word ".repeat(MAX_DIFFABLE_WORDS + 1);
        let current = [DiffTurnInput::visible(long_current.as_str())];
        let candidate = [DiffTurnInput::visible("word")];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Skipped);
        assert!(diff.current.is_empty());
        assert!(diff.candidate.is_empty());
    }

    #[test]
    fn over_budget_edit_distance_skips_without_spans() {
        // Two entirely disjoint word sequences longer than the edit budget:
        // the shortest edit script must exceed MAX_EDIT_BUDGET rounds.
        let current_text: String = (0..(MAX_EDIT_BUDGET + 200))
            .map(|i| format!("cur{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let candidate_text: String = (0..(MAX_EDIT_BUDGET + 200))
            .map(|i| format!("cand{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let current = [DiffTurnInput::visible(current_text.as_str())];
        let candidate = [DiffTurnInput::visible(candidate_text.as_str())];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Skipped);
        assert!(diff.current.is_empty());
        assert!(diff.candidate.is_empty());
    }

    #[test]
    fn empty_sides_are_identical() {
        let diff = diff_transcript_turns(&[], &[]);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        assert!(diff.current.is_empty());
        assert!(diff.candidate.is_empty());
    }

    #[test]
    fn multiple_non_contiguous_spans_in_one_turn_stay_separate() {
        let current = [DiffTurnInput::visible("alpha bravo charlie delta echo")];
        let candidate = [DiffTurnInput::visible("alpha X charlie Y echo")];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(diff.state, TranscriptRetryDiffState::Computed);
        assert_eq!(spans_for(&diff, "current", 0), vec![(1, 2), (3, 4)]);
        assert_eq!(spans_for(&diff, "candidate", 0), vec![(1, 2), (3, 4)]);
    }

    #[test]
    fn word_count_matches_split_whitespace_across_irregular_spacing() {
        // Multiple spaces and a tab still yield one word count per the
        // documented tokenization contract; the renderer's own tokenizer is
        // checked against exactly this number before it trusts a span.
        let current = [DiffTurnInput::visible("alpha   bravo\tcharlie")];
        let candidate = [DiffTurnInput::visible("alpha   bravo\tcharlie delta")];
        let diff = diff_transcript_turns(&current, &candidate);
        assert_eq!(word_count_for(&diff, "current", 0), Some(3));
        assert_eq!(word_count_for(&diff, "candidate", 0), Some(4));
    }
}
