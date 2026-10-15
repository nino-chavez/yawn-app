import Foundation

/// A PencilKit-independent identity for a stroke that survives vector erasing,
/// undo, and redo.
public struct StrokeKey: Hashable, Codable, Sendable {
    public let creationTime: Double
    public let randomSeed: UInt32

    public init(creationDate: Date, randomSeed: UInt32) {
        self.creationTime = creationDate.timeIntervalSinceReferenceDate
        self.randomSeed = randomSeed
    }
}

/// The timing inputs captured from a live PencilKit stroke.
public struct StrokeSample: Sendable {
    public let key: StrokeKey
    public let creationDate: Date
    public let duration: Double

    public init(key: StrokeKey, creationDate: Date, duration: Double) {
        self.key = key
        self.creationDate = creationDate
        self.duration = duration
    }
}

/// Keeps a stroke's original timing while the current drawing changes order.
public struct StrokeLedger: Sendable {
    private struct Record: Sendable {
        let startSeconds: Double?
        let endSeconds: Double?
    }

    private var records: [StrokeKey: Record] = [:]

    public init() {}

    /// Learns timing for newly seen strokes only. Missing strokes remain recorded
    /// so that undo restores their original timing rather than retiming them.
    public mutating func reconcile(_ current: [StrokeSample], recordingStartedAt: Date?) {
        for sample in current where records[sample.key] == nil {
            let startSeconds = recordingStartedAt.map {
                StrokeTimeline.seconds(forStrokeCreatedAt: sample.creationDate, recordingStartedAt: $0)
            }
            records[sample.key] = Record(
                startSeconds: startSeconds,
                endSeconds: startSeconds.map { $0 + sample.duration }
            )
        }
    }

    /// Reindexes records to match the current order in the saved drawing.
    public func liveStrokes(order: [StrokeKey]) -> [TimedStroke] {
        order.enumerated().map { index, key in
            let record = records[key]
            return TimedStroke(
                index: index,
                startSeconds: record?.startSeconds,
                endSeconds: record?.endSeconds
            )
        }
    }
}
