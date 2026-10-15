import Foundation

/// recording-report.json: how one recording ran, in numbers only. It holds no
/// audio and no words, so it can leave the iPad for a soak test or a bug
/// report without carrying anything said in the meeting.
public struct RecordingReport: Codable, Equatable, Sendable {
    public enum Transcription: String, Codable, Sendable {
        /// Transcribed while recording; ready moments after Stop.
        case live
        /// Live transcription failed or was unavailable; the saved audio was
        /// transcribed after Stop instead.
        case afterStop = "after_stop"
        /// No transcript: the speech model is missing or transcription failed.
        case none
    }

    /// Why a recording ended. A fixed set, so the report never carries free text.
    public enum Ending: String, Codable, Sendable {
        case stopped
        case interrupted
        case microphoneChanged = "microphone_changed"
        case microphoneUnsettled = "microphone_unsettled"
        case audioSystemReset = "audio_system_reset"
    }

    public let schema: String
    public let durationSeconds: Double
    public let ending: Ending
    public let engineRestarts: Int
    public let transcription: Transcription
    /// Seconds from the end of recording until the transcript was saved.
    public let transcriptReadySeconds: Double?
    public let finalSegments: Int
    public let batteryStartPercent: Int?
    public let batteryEndPercent: Int?
    public let level: LevelSummary

    public init(durationSeconds: Double, ending: Ending, engineRestarts: Int, transcription: Transcription, transcriptReadySeconds: Double?, finalSegments: Int, batteryStartPercent: Int?, batteryEndPercent: Int?, level: LevelSummary) {
        schema = "yawn-ipad-recording-report/1"
        self.durationSeconds = durationSeconds
        self.ending = ending
        self.engineRestarts = engineRestarts
        self.transcription = transcription
        self.transcriptReadySeconds = transcriptReadySeconds
        self.finalSegments = finalSegments
        self.batteryStartPercent = batteryStartPercent
        self.batteryEndPercent = batteryEndPercent
        self.level = level
    }

    enum CodingKeys: String, CodingKey {
        case schema
        case durationSeconds = "duration_seconds"
        case ending
        case engineRestarts = "engine_restarts"
        case transcription
        case transcriptReadySeconds = "transcript_ready_seconds"
        case finalSegments = "final_segments"
        case batteryStartPercent = "battery_start_percent"
        case batteryEndPercent = "battery_end_percent"
        case level
    }
}
