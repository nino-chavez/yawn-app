import AVFoundation
import InkCore
import Speech
import Synchronization

/// Transcribes the meeting while it records, so the transcript is ready moments
/// after Stop (docs/ipad-build-plan-2026-10-15.md, "Why transcribe live"). It
/// is fed the same microphone buffers as audio.wav, starting with the same
/// first buffer, so its times line up with the saved audio.
@MainActor
final class LiveTranscriber {
    let feeder: LiveFeeder
    private let transcriber: SpeechTranscriber
    private let analyzer: SpeechAnalyzer
    private var resultsTask: Task<Void, Never>?
    private var segments: [TranscriptSegment] = []
    private var failed = false

    static func makeTranscriber() -> SpeechTranscriber {
        SpeechTranscriber(locale: .current, preset: .timeIndexedProgressiveTranscription)
    }

    /// nil when the speech model is not installed or live analysis cannot
    /// start; the recording then goes ahead and is transcribed after Stop.
    static func start(inputFormat: AVAudioFormat) async -> LiveTranscriber? {
        let transcriber = makeTranscriber()
        guard await AssetInventory.status(forModules: [transcriber]) == .installed,
              let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]),
              let feeder = LiveFeeder(inputFormat: inputFormat, analyzerFormat: format) else { return nil }
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        do {
            try await analyzer.prepareToAnalyze(in: format)
            try await analyzer.start(inputSequence: feeder.stream)
        } catch {
            feeder.finish()
            return nil
        }
        return LiveTranscriber(feeder: feeder, transcriber: transcriber, analyzer: analyzer)
    }

    private init(feeder: LiveFeeder, transcriber: SpeechTranscriber, analyzer: SpeechAnalyzer) {
        self.feeder = feeder
        self.transcriber = transcriber
        self.analyzer = analyzer
        resultsTask = Task { [weak self, transcriber] in
            do {
                for try await result in transcriber.results {
                    self?.merge(TranscriptSegment(
                        start: result.range.start.seconds,
                        end: result.range.end.seconds,
                        text: String(result.text.characters),
                        isFinal: result.isFinal
                    ))
                }
            } catch {
                self?.failed = true
            }
        }
    }

    private func merge(_ segment: TranscriptSegment) {
        segments = TranscriptTimeline.merging(segments, with: segment)
    }

    /// Ends the input and waits for the last results. nil if live analysis
    /// failed at any point, so the caller transcribes the saved audio instead.
    func finish() async -> [TranscriptSegment]? {
        feeder.finish()
        do {
            try await analyzer.finalizeAndFinishThroughEndOfInput()
        } catch {
            failed = true
        }
        await resultsTask?.value
        return failed ? nil : TranscriptTimeline.completed(segments)
    }

    /// Transcribes a saved recording: the fallback when live analysis did not
    /// run, and the retry after the speech model is installed.
    static func transcribeFile(at url: URL) async throws -> [TranscriptSegment] {
        let transcriber = makeTranscriber()
        let collect = Task { () throws -> [TranscriptSegment] in
            var segments: [TranscriptSegment] = []
            for try await result in transcriber.results {
                segments = TranscriptTimeline.merging(segments, with: TranscriptSegment(
                    start: result.range.start.seconds,
                    end: result.range.end.seconds,
                    text: String(result.text.characters),
                    isFinal: result.isFinal
                ))
            }
            return segments
        }
        do {
            let file = try AVAudioFile(forReading: url)
            let analyzer = SpeechAnalyzer(modules: [transcriber])
            if let last = try await analyzer.analyzeSequence(from: file) {
                try await analyzer.finalizeAndFinish(through: last)
            } else {
                await analyzer.cancelAndFinishNow()
            }
            return TranscriptTimeline.completed(try await collect.value)
        } catch {
            collect.cancel()
            throw error
        }
    }
}

/// Converts microphone buffers to the analyzer's format on the audio thread and
/// hands them to the analyzer's input stream.
final class LiveFeeder: Sendable {
    let stream: AsyncStream<AnalyzerInput>
    private let continuation: AsyncStream<AnalyzerInput>.Continuation
    private let analyzerFormat: AVAudioFormat
    private let converter: Mutex<AVAudioConverter>

    init?(inputFormat: AVAudioFormat, analyzerFormat: AVAudioFormat) {
        guard let converter = AVAudioConverter(from: inputFormat, to: analyzerFormat) else { return nil }
        (stream, continuation) = AsyncStream<AnalyzerInput>.makeStream()
        self.analyzerFormat = analyzerFormat
        self.converter = Mutex(converter)
    }

    /// After the engine restarts in place, the microphone's format may differ.
    func setInputFormat(_ format: AVAudioFormat) {
        guard let replacement = AVAudioConverter(from: format, to: analyzerFormat) else { return }
        converter.withLock { $0 = replacement }
    }

    func feed(_ buffer: AVAudioPCMBuffer) {
        converter.withLock { converter in
            let ratio = analyzerFormat.sampleRate / buffer.format.sampleRate
            let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 32
            guard let converted = AVAudioPCMBuffer(pcmFormat: analyzerFormat, frameCapacity: capacity) else { return }
            var supplied = false
            var error: NSError?
            converter.convert(to: converted, error: &error) { _, status in
                if supplied {
                    status.pointee = .noDataNow
                    return nil
                }
                supplied = true
                status.pointee = .haveData
                return buffer
            }
            if error == nil, converted.frameLength > 0 {
                continuation.yield(AnalyzerInput(buffer: converted))
            }
        }
    }

    func finish() {
        continuation.finish()
    }
}
