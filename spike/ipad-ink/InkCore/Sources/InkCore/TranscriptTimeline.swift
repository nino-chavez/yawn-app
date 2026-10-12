import Foundation

public enum TranscriptTimeline {
    public static func merging(_ existing: [TranscriptSegment], with incoming: TranscriptSegment) -> [TranscriptSegment] {
        var merged = existing
        if let index = merged.firstIndex(where: { $0.start == incoming.start && $0.end == incoming.end }) {
            let current = merged[index]
            if incoming.isFinal || !current.isFinal {
                merged[index] = incoming
            }
        } else {
            merged.append(incoming)
        }
        return merged.sorted { $0.start < $1.start }
    }
}
