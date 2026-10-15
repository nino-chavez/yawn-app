import AVFoundation
import InkCore
import Synchronization

/// A test control for comparing how the iPad records a room. Measurement is
/// the default, so a normal recording is unchanged. See README, "Microphone
/// mode test".
enum MicrophoneMode: String, CaseIterable, Identifiable, Sendable {
    case measurement
    /// Measurement with the input gain raised from the system's 0.5 to 1. Added
    /// after round one showed the gain is settable on this iPad.
    case measurementFullGain = "measurement_full_gain"
    case standard = "default"
    case videoRecording = "video_recording"
    case voiceProcessing = "voice_processing"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .measurement: "Measurement"
        case .measurementFullGain: "Measurement, full input gain"
        case .standard: "Default"
        case .videoRecording: "Video recording"
        case .voiceProcessing: "Voice processing"
        }
    }

    /// Voice processing runs through the voice-chat I/O unit, which needs a
    /// play-and-record session; video recording is valid with record.
    var category: AVAudioSession.Category {
        self == .voiceProcessing ? .playAndRecord : .record
    }

    var sessionMode: AVAudioSession.Mode {
        switch self {
        case .measurement, .measurementFullGain: .measurement
        case .standard: .default
        case .videoRecording: .videoRecording
        case .voiceProcessing: .voiceChat
        }
    }
}

/// Fed on the audio thread with the 16 kHz buffers that are written to disk;
/// read on the main actor for the meter and the saved check.
final class LevelMeter: Sendable {
    private let state = Mutex(LevelAccumulator())

    func reset() {
        state.withLock { $0 = LevelAccumulator() }
    }

    func add(_ buffer: AVAudioPCMBuffer) {
        guard let channels = buffer.floatChannelData else { return }
        let samples = UnsafeBufferPointer(start: channels[0], count: Int(buffer.frameLength))
        state.withLock { $0.add(samples) }
    }

    var latestWindowRMS: Double {
        state.withLock { $0.latestWindowRMS }
    }

    var summary: LevelSummary {
        state.withLock { $0.summary }
    }
}

extension RecordingSessionState {
    /// What the session and engine actually are once recording has started.
    @MainActor
    static func current(engine: AVAudioEngine) -> RecordingSessionState {
        let session = AVAudioSession.sharedInstance()
        let input = session.currentRoute.inputs.first
        let source = input?.selectedDataSource ?? session.inputDataSource
        let format = engine.inputNode.outputFormat(forBus: 0)
        return RecordingSessionState(
            category: session.category.rawValue,
            mode: session.mode.rawValue,
            voiceProcessing: engine.inputNode.isVoiceProcessingEnabled,
            inputPortType: input?.portType.rawValue,
            inputPortName: input?.portName,
            dataSource: source?.dataSourceName,
            polarPattern: source?.selectedPolarPattern?.rawValue,
            inputGainSettable: session.isInputGainSettable,
            inputGain: Double(session.inputGain),
            sessionSampleRate: session.sampleRate,
            inputSampleRate: format.sampleRate,
            inputChannels: Int(format.channelCount)
        )
    }
}
