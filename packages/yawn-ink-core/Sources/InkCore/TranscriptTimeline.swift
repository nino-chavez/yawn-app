import Foundation

public enum TranscriptTimeline {
    /// Merges one speech result into the transcript.
    ///
    /// Final text is authoritative. In-progress (volatile) text is the
    /// provisional tail of audio after the last final, and its range grows as
    /// audio arrives:
    /// - an in-progress result replaces all earlier in-progress text, and is
    ///   dropped when the finals already reach its end;
    /// - a final result replaces in-progress text it overlaps or precedes, and
    ///   a final with the exact same range.
    public static func merging(_ existing: [TranscriptSegment], with incoming: TranscriptSegment) -> [TranscriptSegment] {
        let finals = existing.filter(\.isFinal)
        guard incoming.isFinal else {
            let lastFinalEnd = finals.map(\.end).max() ?? -.infinity
            guard incoming.end > lastFinalEnd else { return existing }
            return sorted(finals + [incoming])
        }
        let keptFinals = finals.filter { !($0.start == incoming.start && $0.end == incoming.end) }
        let keptInProgress = existing.filter { !$0.isFinal && !(overlaps($0, incoming) || $0.start >= incoming.start) }
        return sorted(keptFinals + keptInProgress + [incoming])
    }

    /// The transcript of a finished run: final text only. In-progress text left
    /// over at the end is a guess and is not saved.
    public static func completed(_ segments: [TranscriptSegment]) -> [TranscriptSegment] {
        segments.filter(\.isFinal)
    }

    private static func sorted(_ segments: [TranscriptSegment]) -> [TranscriptSegment] {
        segments.sorted { ($0.start, $0.end) < ($1.start, $1.end) }
    }

    private static func overlaps(_ a: TranscriptSegment, _ b: TranscriptSegment) -> Bool {
        a.start < b.end && b.start < a.end
    }
}
