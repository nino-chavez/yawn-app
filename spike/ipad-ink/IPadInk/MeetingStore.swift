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
    private var strokeLedger = StrokeLedger()
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
        strokeLedger = StrokeLedger()
        transcript = []
        meetingURL = nil
        audioURL = nil
        recordingStartedAt = nil
        playbackTime = 0
        recordingState = .ready
    }

    func drawingChanged(_ drawing: PKDrawing) {
        guard canDraw else { return }
        self.drawing = drawing
        let samples = drawing.strokes.map { stroke in
            let creationDate = stroke.path.creationDate
            let key = StrokeKey(creationDate: creationDate, randomSeed: stroke.randomSeed)
            return StrokeSample(key: key, creationDate: creationDate, duration: stroke.path.last?.timeOffset ?? 0)
        }
        strokeLedger.reconcile(samples, recordingStartedAt: recordingStartedAt)
        timedStrokes = strokeLedger.liveStrokes(order: samples.map(\.key))
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
                let segments = TranscriptTimeline.completed(try await readResults.value)
                // Saved to the meeting this run belongs to, even after New meeting.
                try write(segments, to: folder.appendingPathComponent("transcript.json"))
                if generation == meetingGeneration { transcript = segments }
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
            // The ledger entry and meeting.json exist before any audio does, so the
            // keep-period sweep can always find what this meeting records.
            let startedAt = Date()
            let meeting = MeetingExport(createdAt: startedAt, consentEveryoneAudible: true, keepDays: keepDays, audioSampleRate: Int(Self.fileSampleRate))
            try write(meeting, to: meetingFolder.appendingPathComponent("meeting.json"))
            try recordRetention(meetingID: meetingFolder.lastPathComponent, createdAt: startedAt, keepDays: keepDays)

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

    /// The keep period lives in Application Support, which the Files app cannot
    /// reach. meeting.json in the shared folder is an export, not the authority:
    /// editing or deleting it in Files must not stop audio deletion.
    private struct RetentionEntry: Codable {
        let meetingID: String
        let createdAt: Date
        let keepDays: Int
    }

    private static func retentionLedgerURL() throws -> URL {
        let support = try FileManager.default.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        return support.appendingPathComponent("retention-ledger.json")
    }

    private static func meetingsFolder() -> URL? {
        try? FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: false)
            .appendingPathComponent("meetings", isDirectory: true)
    }

    /// Reads the ledger. A damaged ledger is set aside, and a damaged or
    /// missing one is rebuilt once from the meetings' own meeting.json files.
    /// Only folders still named by their meeting ID are rebuilt, so a folder
    /// renamed or duplicated in Files stays out of the sweep.
    private static func loadRetentionLedger() throws -> [RetentionEntry] {
        let url = try retentionLedgerURL()
        if FileManager.default.fileExists(atPath: url.path) {
            // A failed read throws; only a ledger that reads but will not decode is set aside.
            let data = try Data(contentsOf: url)
            if let entries = try? MeetingExportEncoder.decode([RetentionEntry].self, from: data) {
                return entries
            }
            let stamp = ISO8601DateFormatter().string(from: Date()).replacingOccurrences(of: ":", with: "-")
            try FileManager.default.moveItem(at: url, to: url.deletingLastPathComponent().appendingPathComponent("retention-ledger.damaged-\(stamp).json"))
        }
        let folders = meetingsFolder().flatMap { try? FileManager.default.contentsOfDirectory(at: $0, includingPropertiesForKeys: nil) } ?? []
        let rebuilt = folders.compactMap { folder -> RetentionEntry? in
            guard UUID(uuidString: folder.lastPathComponent) != nil,
                  let data = try? Data(contentsOf: folder.appendingPathComponent("meeting.json")),
                  let meeting = try? MeetingExportEncoder.decode(MeetingExport.self, from: data) else { return nil }
            // meeting.json can be edited in Files. A keep period outside 1, 7 or 30
            // days becomes 1, and a future date becomes now: bad data deletes sooner.
            let keepDays = [1, 7, 30].contains(meeting.keepDays) ? meeting.keepDays : 1
            return RetentionEntry(meetingID: folder.lastPathComponent, createdAt: min(meeting.createdAt, Date()), keepDays: keepDays)
        }
        if !rebuilt.isEmpty {
            try MeetingExportEncoder.data(for: rebuilt).write(to: url, options: .atomic)
        }
        return rebuilt
    }

    private func recordRetention(meetingID: String, createdAt: Date, keepDays: Int) throws {
        // A rebuild can already include this meeting from its meeting.json.
        let entries = try Self.loadRetentionLedger().filter { $0.meetingID != meetingID }
            + [RetentionEntry(meetingID: meetingID, createdAt: createdAt, keepDays: keepDays)]
        try write(entries, to: Self.retentionLedgerURL())
    }

    private func removeExpiredAudio() {
        guard let meetings = Self.meetingsFolder(), let ledger = try? Self.loadRetentionLedger() else { return }
        let records = ledger.map {
            MeetingRetentionRecord(meetingID: $0.meetingID, createdAt: $0.createdAt, keepDays: $0.keepDays)
        }
        for meetingID in AudioRetentionSweep.meetingIDsToDeleteAudio(for: records, now: Date()) {
            try? FileManager.default.removeItem(at: meetings.appendingPathComponent(meetingID, isDirectory: true).appendingPathComponent("audio.wav"))
        }
    }
}
