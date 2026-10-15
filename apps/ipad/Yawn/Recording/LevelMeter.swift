import AVFoundation
import InkCore
import Synchronization

/// Fed on the audio thread with the 16 kHz buffers written to disk; read on the
/// main actor for the recording report.
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

    var summary: LevelSummary {
        state.withLock { $0.summary }
    }
}
