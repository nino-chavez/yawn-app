import Foundation

public enum TranscriptTimeline {
    /// Merges one speech result into the transcript.
    ///
    /// In-progress (volatile) text is provisional and its range grows as audio
    /// arrives, so a later result rarely has the same range. Any in-progress
    /// segment that overlaps or follows the incoming result is superseded by
    /// it. Final segments stay, except a final with the exact same range,
    /// which the incoming final replaces.
    public static func merging(_ existing: [TranscriptSegment], with incoming: TranscriptSegment) -> [TranscriptSegment] {
        // Final text wins: in-progress text lying wholly within a final is dropped.
        // Text that only touches or straddles a final's edge is newer audio and stays.
        if !incoming.isFinal, existing.contains(where: { $0.isFinal && $0.start <= incoming.start && incoming.end <= $0.end }) {
            return existing
        }
        let kept = existing.filter { segment in
            if segment.isFinal {
                return !(incoming.isFinal && segment.start == incoming.start && segment.end == incoming.end)
            }
            return !(overlaps(segment, incoming) || segment.start >= incoming.start)
        }
        return (kept + [incoming]).sorted { ($0.start, $0.end) < ($1.start, $1.end) }
    }

    private static func overlaps(_ a: TranscriptSegment, _ b: TranscriptSegment) -> Bool {
        a.start < b.end && b.start < a.end
    }
}
