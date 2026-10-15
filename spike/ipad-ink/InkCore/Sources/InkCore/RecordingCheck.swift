import Foundation

/// The audio session as the system actually configured it, read back after
/// activation. The system can override what the app asked for.
public struct RecordingSessionState: Codable, Equatable, Sendable {
    public let category: String
    public let mode: String
    public let voiceProcessing: Bool
    public let inputPortType: String?
    public let inputPortName: String?
    public let dataSource: String?
    public let polarPattern: String?
    public let inputGainSettable: Bool
    public let inputGain: Double
    public let sessionSampleRate: Double
    public let inputSampleRate: Double
    public let inputChannels: Int

    public init(category: String, mode: String, voiceProcessing: Bool, inputPortType: String?, inputPortName: String?, dataSource: String?, polarPattern: String?, inputGainSettable: Bool, inputGain: Double, sessionSampleRate: Double, inputSampleRate: Double, inputChannels: Int) {
        self.category = category
        self.mode = mode
        self.voiceProcessing = voiceProcessing
        self.inputPortType = inputPortType
        self.inputPortName = inputPortName
        self.dataSource = dataSource
        self.polarPattern = polarPattern
        self.inputGainSettable = inputGainSettable
        self.inputGain = inputGain
        self.sessionSampleRate = sessionSampleRate
        self.inputSampleRate = inputSampleRate
        self.inputChannels = inputChannels
    }

    enum CodingKeys: String, CodingKey {
        case category
        case mode
        case voiceProcessing = "voice_processing"
        case inputPortType = "input_port_type"
        case inputPortName = "input_port_name"
        case dataSource = "data_source"
        case polarPattern = "polar_pattern"
        case inputGainSettable = "input_gain_settable"
        case inputGain = "input_gain"
        case sessionSampleRate = "session_sample_rate"
        case inputSampleRate = "input_sample_rate"
        case inputChannels = "input_channels"
    }
}

/// recording-check.json: which microphone mode a test recording asked for,
/// what the session really was, and the level of what was written. No audio
/// or words.
public struct RecordingCheck: Codable, Equatable, Sendable {
    public let schema: String
    public let requestedMode: String
    public let session: RecordingSessionState
    public let durationSeconds: Double
    /// Times capture restarted in place after the microphone reconfigured
    /// without changing (voice processing does this right after start).
    public let engineRestarts: Int
    public let level: LevelSummary

    public init(requestedMode: String, session: RecordingSessionState, durationSeconds: Double, engineRestarts: Int, level: LevelSummary) {
        schema = "ipad-ink-recording-check/1"
        self.requestedMode = requestedMode
        self.session = session
        self.durationSeconds = durationSeconds
        self.engineRestarts = engineRestarts
        self.level = level
    }

    enum CodingKeys: String, CodingKey {
        case schema
        case requestedMode = "requested_mode"
        case session
        case durationSeconds = "duration_seconds"
        case engineRestarts = "engine_restarts"
        case level
    }
}
