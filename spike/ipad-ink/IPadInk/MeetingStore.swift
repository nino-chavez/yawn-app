import AVFoundation
import InkCore
import PencilKit
import Speech
import UIKit

@MainActor
final class MeetingStore: NSObject, ObservableObject {
    enum RecordingState: Equatable {
        case ready
        case recording
        case stopped
        case transcribing
        case needsSpeechModel
        case failed(String)
    }

    @Published var consentConfirmed = false
    @Published var keepDays: Int?
    @Published private(set) var recordingState: RecordingState = .ready
    @Published private(set) var drawing = PKDrawing()
    @Published private(set) var timedStrokes: [TimedStroke] = []
    @Published private(set) var transcript: [TranscriptSegment] = []
    @Published private(set) var speechModelStatus = "Checking speech model…"
    @Published private(set) var speechModelInstallable = false
    @Published private(set) var playbackTime = 0.0

    private let audioEngine = AVAudioEngine()
    private var audioFile: AVAudioFile?
    private var meetingURL: URL?
    private var audioURL: URL?
    private var recordingStartedAt: Date?
    private var player: AVAudioPlayer?
    /// Bumped by New meeting. A transcription run that outlives its meeting
    /// still saves to that meeting's folder but no longer touches the screen.
    private var meetingGeneration = 0

    private static let fileSampleRate = 16_000.0

    override init() {
        super.init()
        removeExpiredAudio()
        Task { await refreshSpeechModelStatus() }
    }

    var canStart: Bool {
        consentConfirmed && keepDays != nil && recordingState == .ready
    }

    var isRecording: Bool {
        recordingState == .recording
    }

    /// Replay owns the canvas once a recording has stopped: drawing is off and
    /// strokes later than the playhead are dimmed rather than removed.
    var isReplaying: Bool {
        player != nil && !isRecording
    }

    /// Ink is accepted only before and during a recording. After Stop the
    /// canvas belongs to replay, and after a failure it stays read-only.
    var canDraw: Bool {
        recordingState == .ready || recordingState == .recording
    }

    var canStartNewMeeting: Bool {
        switch recordingState {
        case .stopped, .needsSpeechModel, .failed: true
        case .ready, .recording, .transcribing: false
        }
    }

    var playbackDuration: Double {
        player?.duration ?? 0
    }

    var displayedDrawing: PKDrawing {
        guard isReplaying else { return drawing }
        let visible = Set(StrokeTimeline.visibleStrokes(timedStrokes, at: playbackTime))
        let strokes = drawing.strokes.enumerated().map { offset, stroke in
            guard !visible.contains(offset) else { return stroke }
            let dimmed = PKInk(stroke.ink.inkType, color: stroke.ink.color.withAlphaComponent(0.15))
            return PKStroke(ink: dimmed, path: stroke.path, transform: stroke.transform, mask: stroke.mask)
        }
        return PKDrawing(strokes: strokes)
    }

    func startRecording() {
        guard canStart, let keepDays else { return }
        Task {
            guard await AVAudioApplication.requestRecordPermission() else {
                recordingState = .failed("Microphone access was not allowed.")
                return
            }
            beginRecording(keepDays: keepDays)
        }
    }

    func stopRecording() {
        guard isRecording else { return }
        audioEngine.inputNode.removeTap(onBus: 0)
        audioEngine.stop()
        audioFile = nil
        recordingState = .stopped
        // A failed save stays on screen; replay and transcription would hide it.
        guard writeExports() else { return }

        if let audioURL {
            do {
                // A record-only session cannot play the replay.
                try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
                try AVAudioSession.sharedInstance().setActive(true)
                player = try AVAudioPlayer(contentsOf: audioURL)
                player?.prepareToPlay()
            } catch {
                recordingState = .failed("The audio was saved, but replay could not open it: \(error.localizedDescription)")
                return
            }
        }
        transcribeSavedAudio()
    }

    /// Clears the screen for another meeting. Consent and the keep period are
    /// asked again; the previous meeting stays in its Files folder.
    func startNewMeeting() {
        guard canStartNewMeeting else { return }
        meetingGeneration += 1
        player?.stop()
        player = nil
        consentConfirmed = false
        keepDays = nil
        drawing = PKDrawing()
        timedStrokes = []
        transcript = []
        meetingURL = nil
        audioURL = nil
        recordingStartedAt = nil
        playbackTime = 0
        recordingState = .ready
    }

    func addStrokes(_ strokes: [PKStroke], drawing: PKDrawing) {
        guard canDraw else { return }
        self.drawing = drawing
        let recordingStart = recordingStartedAt
        let additions = strokes.enumerated().map { offset, stroke in
            let start = recordingStart.map { StrokeTimeline.seconds(forStrokeCreatedAt: stroke.path.creationDate, recordingStartedAt: $0) }
            let duration = stroke.path.last?.timeOffset ?? 0
            return TimedStroke(index: timedStrokes.count + offset, startSeconds: start, endSeconds: start.map { $0 + duration })
        }
        timedStrokes.append(contentsOf: additions)
        if !isRecording {
            writeExports()
        }
    }

    func togglePlayback() {
        guard let player else { return }
        if player.isPlaying {
            player.pause()
        } else {
            player.play()
        }
        refreshPlaybackTime()
    }

    func seek(to seconds: Double) {
        guard let player else { return }
        player.currentTime = min(max(0, seconds), player.duration)
        playbackTime = player.currentTime
    }

    func seekToStroke(at index: Int) {
        guard isReplaying, let seconds = timedStrokes.first(where: { $0.index == index })?.startSeconds else { return }
        seek(to: seconds)
    }

    func refreshPlaybackTime() {
        guard let player, player.currentTime != playbackTime else { return }
        playbackTime = player.currentTime
    }

    func installSpeechModel() {
        Task {
            let transcriber = Self.makeTranscriber()
            do {
                if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
                    speechModelStatus = "Installing Apple’s on-device speech model…"
                    speechModelInstallable = false
                    try await request.downloadAndInstall()
                }
                await refreshSpeechModelStatus()
                if recordingState == .needsSpeechModel {
                    transcribeSavedAudio()
                }
            } catch {
                speechModelStatus = "Speech model installation failed: \(error.localizedDescription)"
            }
        }
    }

    func refreshSpeechModelStatus() async {
        let status = await AssetInventory.status(forModules: [Self.makeTranscriber()])
        speechModelInstallable = status == .supported
        speechModelStatus = switch status {
        case .installed: "Apple’s on-device speech model is installed."
        case .supported: "Apple’s on-device speech model is available to install."
        case .downloading: "Apple’s on-device speech model is downloading."
        case .unsupported: "This locale does not have an on-device speech model."
        @unknown default: "Speech model status is unavailable."
        }
    }

    /// Transcribes the saved audio after Stop. Live transcription was cut from
    /// this spike; see README.
    func transcribeSavedAudio() {
        guard let audioURL, let folder = meetingURL, FileManager.default.fileExists(atPath: audioURL.path) else { return }
        let generation = meetingGeneration
        recordingState = .transcribing
        Task {
            let transcriber = Self.makeTranscriber()
            guard await AssetInventory.status(forModules: [transcriber]) == .installed else {
                if generation == meetingGeneration { recordingState = .needsSpeechModel }
                return
            }

            let readResults = Task { () throws -> [TranscriptSegment] in
                var segments: [TranscriptSegment] = []
                for try await result in transcriber.results {
                    let segment = TranscriptSegment(
                        start: result.range.start.seconds,
                        end: result.range.end.seconds,
                        text: String(result.text.characters),
                        isFinal: result.isFinal
                    )
                    segments = TranscriptTimeline.merging(segments, with: segment)
                    if generation == self.meetingGeneration { self.transcript = segments }
                }
                return segments
            }
            do {
                if generation == meetingGeneration { transcript = [] }
                let audioFile = try AVAudioFile(forReading: audioURL)
                let analyzer = SpeechAnalyzer(modules: [transcriber])
                if let lastSample = try await analyzer.analyzeSequence(from: audioFile) {
                    try await analyzer.finalizeAndFinish(through: lastSample)
                } else {
                    await analyzer.cancelAndFinishNow()
                }
                let segments = try await readResults.value
                // Saved to the meeting this run belongs to, even after New meeting.
                try write(segments, to: folder.appendingPathComponent("transcript.json"))
                if generation == meetingGeneration { recordingState = .stopped }
            } catch {
                readResults.cancel()
                if generation == meetingGeneration {
                    recordingState = .failed("Transcription could not finish: \(error.localizedDescription)")
                }
            }
        }
    }

    private static func makeTranscriber() -> SpeechTranscriber {
        SpeechTranscriber(locale: .current, preset: .timeIndexedProgressiveTranscription)
    }

    private func beginRecording(keepDays: Int) {
        var folder: URL?
        do {
            // Measurement mode turns off voice processing so the room is
            // recorded as the microphone hears it.
            try AVAudioSession.sharedInstance().setCategory(.record, mode: .measurement)
            try AVAudioSession.sharedInstance().setActive(true)

            let meetingFolder = try makeMeetingFolder()
            folder = meetingFolder
            // meeting.json comes first: the keep-period sweep finds audio through it.
            let startedAt = Date()
            let meeting = MeetingExport(createdAt: startedAt, consentEveryoneAudible: true, keepDays: keepDays, audioSampleRate: Int(Self.fileSampleRate))
            try write(meeting, to: meetingFolder.appendingPathComponent("meeting.json"))

            let destination = meetingFolder.appendingPathComponent("audio.wav")
            let settings: [String: Any] = [
                AVFormatIDKey: kAudioFormatLinearPCM,
                AVSampleRateKey: Self.fileSampleRate,
                AVNumberOfChannelsKey: 1,
                AVLinearPCMBitDepthKey: 16,
                AVLinearPCMIsFloatKey: false,
                AVLinearPCMIsBigEndianKey: false,
            ]
            // The file is 16-bit on disk; it accepts 16 kHz mono float buffers.
            let file = try AVAudioFile(forWriting: destination, settings: settings, commonFormat: .pcmFormatFloat32, interleaved: false)
            let input = audioEngine.inputNode
            let inputFormat = input.outputFormat(forBus: 0)
            guard let converter = AVAudioConverter(from: inputFormat, to: file.processingFormat) else {
                throw CocoaError(.featureUnsupported)
            }
            input.installTap(onBus: 0, bufferSize: 4_096, format: inputFormat, block: Self.makeTapBlock(file: file, converter: converter))
            audioEngine.prepare()
            try audioEngine.start()

            meetingURL = meetingFolder
            audioURL = destination
            audioFile = file
            recordingStartedAt = startedAt
            recordingState = .recording
        } catch {
            audioEngine.inputNode.removeTap(onBus: 0)
            audioEngine.stop()
            // A meeting that never started keeps no audio behind.
            if let folder { try? FileManager.default.removeItem(at: folder) }
            recordingState = .failed("Recording could not start: \(error.localizedDescription)")
        }
    }

    /// Built outside the main actor: the engine calls the tap on its audio
    /// thread, and a main-actor closure would trap there under Swift 6.
    nonisolated private static func makeTapBlock(file: AVAudioFile, converter: AVAudioConverter) -> AVAudioNodeTapBlock {
        let outputFormat = converter.outputFormat
        return { buffer, _ in
            let ratio = outputFormat.sampleRate / buffer.format.sampleRate
            let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 32
            guard let converted = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: capacity) else { return }
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
                try? file.write(from: converted)
            }
        }
    }

    private func merge(_ segment: TranscriptSegment) {
        transcript = TranscriptTimeline.merging(transcript, with: segment)
    }

    private func makeMeetingFolder() throws -> URL {
        let documents = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let folder = documents.appendingPathComponent("meetings", isDirectory: true).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    @discardableResult
    private func writeExports() -> Bool {
        guard let meetingURL else { return false }
        do {
            try drawing.dataRepresentation().write(to: meetingURL.appendingPathComponent("drawing.data"), options: .atomic)
            try write(timedStrokes, to: meetingURL.appendingPathComponent("strokes.json"))
            try write(transcript, to: meetingURL.appendingPathComponent("transcript.json"))
            return true
        } catch {
            recordingState = .failed("The meeting could not be exported: \(error.localizedDescription)")
            return false
        }
    }

    private func write<T: Encodable>(_ value: T, to url: URL) throws {
        try MeetingExportEncoder.data(for: value).write(to: url, options: .atomic)
    }

    private func removeExpiredAudio() {
        guard let documents = try? FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: false) else { return }
        let meetings = documents.appendingPathComponent("meetings", isDirectory: true)
        let folders = (try? FileManager.default.contentsOfDirectory(at: meetings, includingPropertiesForKeys: nil)) ?? []
        let records = folders.compactMap { folder -> MeetingRetentionRecord? in
            guard let data = try? Data(contentsOf: folder.appendingPathComponent("meeting.json")),
                  let meeting = try? MeetingExportEncoder.decode(MeetingExport.self, from: data) else {
                return nil
            }
            return MeetingRetentionRecord(meetingID: folder.lastPathComponent, createdAt: meeting.createdAt, keepDays: meeting.keepDays)
        }
        let expired = Set(AudioRetentionSweep.meetingIDsToDeleteAudio(for: records, now: Date()))
        for folder in folders where expired.contains(folder.lastPathComponent) {
            try? FileManager.default.removeItem(at: folder.appendingPathComponent("audio.wav"))
        }
    }
}
