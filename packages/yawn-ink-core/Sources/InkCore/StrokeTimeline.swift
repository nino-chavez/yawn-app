import Foundation

public enum StrokeTimeline {
    public static func seconds(forStrokeCreatedAt creationDate: Date, recordingStartedAt: Date) -> Double {
        max(0, creationDate.timeIntervalSince(recordingStartedAt))
    }

    public static func visibleStrokes(_ strokes: [TimedStroke], at playhead: Double) -> Set<Int> {
        Set(strokes.compactMap { stroke in
            guard let startSeconds = stroke.startSeconds, startSeconds <= playhead else {
                return stroke.startSeconds == nil ? stroke.index : nil
            }
            return stroke.index
        })
    }
}
