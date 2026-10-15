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
    /// Set when something other than Stop ended the recording. It stays on
    /// screen beside the normal after-Stop states until New meeting.
    @Published private(set) var interruptionNote: String?
    /// Test control, see MicrophoneTest.swift. Changing it applies to the next
    /// recording only.
    @Published var microphoneMode: MicrophoneMode = .measurement
    /// The last 100 ms window's level while recording, in dBFS.
    @Published private(set) var inputLevel: Double?
    /// What the last recording's microphone test measured, shown after Stop.
    @Published private(set) var lastRecordingCheck: RecordingCheck?
    /// How much louder replay plays than the saved audio, in dB.
    @Published private(set) var replayGainDB: Double?

    private var audioEngine = AVAudioEngine()
    private var audioFile: AVAudioFile?
    private var meetingURL: URL?
    private var audioURL: URL?
    private var recordingStartedAt: Date?
    private var strokeLedger = StrokeLedger()
    private var player: AVAudioPlayer?
    private var audioSessionObservers: [NSObjectProtocol] = []
    private var engineObserver: NSObjectProtocol?
    private let levelMeter = LevelMeter()
    private var recordingMode: MicrophoneMode = .measurement
    private var recordingSession: RecordingSessionState?
    /// The input port a recording started on, as "type:uid". A reconfiguration
    /// on the same port restarts capture; a different port ends the recording.
    private var recordingInputPort: String?
    private var engineRestarts = 0
    /// Set when a test mode raised the input gain, so Stop can put it back.
    private var inputGainToRestore: Float?
    private static let maxEngineRestarts = 3
    /// Bumped by New meeting. A transcription run that outlives its meeting
    /// still saves to that meeting's folder but no longer touches the screen.
    private var meetingGeneration = 0

    private static let fileSampleRate = 16_000.0

    override init() {
        super.init()
        registerAudioSessionObservers()
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
                UIApplication.shared.isIdleTimerDisabled = false
                recordingState = .failed("Microphone access was not allowed.")
                return
            }
            beginRecording(keepDays: keepDays)
        }
    }

    func stopRecording() {
        guard isRecording else { return }
        finishAudioCapture()
        recordingState = .stopped
        // A failed save stays on screen; replay and transcription would hide it.
        guard writeExports() else { return }

        guard prepareReplay(reportFailure: true) else { return }
        transcribeSavedAudio()
    }

    /// Clears the screen for another meeting. Consent and the keep period are
    /// asked again; the previous meeting stays in its Files folder.
    func startNewMeeting() {
        guard canStartNewMeeting else { return }
        meetingGeneration += 1
        UIApplication.shared.isIdleTimerDisabled = false
        interruptionNote = nil
        lastRecordingCheck = nil
        replayGainDB = nil
        inputLevel = nil
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
            // Measurement mode keeps system processing to a minimum so the room
            // is recorded as the microphone hears it; the other modes are a test.
            let mode = microphoneMode
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(mode.category, mode: mode.sessionMode)
            try session.setActive(true)
            if mode == .measurementFullGain, session.isInputGainSettable {
                inputGainToRestore = session.inputGain
                try session.setInputGain(1)
            }
            // A fresh engine per recording: voice processing belongs to the
            // engine's I/O unit and must not carry into the next recording.
            audioEngine = AVAudioEngine()
            observeEngineConfiguration()
            if mode == .voiceProcessing {
                try audioEngine.inputNode.setVoiceProcessingEnabled(true)
                // The voice-processing unit runs only with its output in the graph.
                _ = audioEngine.mainMixerNode
            }

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
            levelMeter.reset()
            engineRestarts = 0
            try installCaptureTap(writingTo: file)
            audioEngine.prepare()
            try audioEngine.start()
            recordingMode = mode
            recordingSession = RecordingSessionState.current(engine: audioEngine)
            recordingInputPort = Self.currentInputPort()

            meetingURL = meetingFolder
            audioURL = destination
            audioFile = file
            recordingStartedAt = startedAt
            UIApplication.shared.isIdleTimerDisabled = true
            recordingState = .recording
        } catch {
            audioEngine.inputNode.removeTap(onBus: 0)
            audioEngine.stop()
            restoreInputGain()
            UIApplication.shared.isIdleTimerDisabled = false
            // A meeting that never started keeps no audio behind.
            if let folder { try? FileManager.default.removeItem(at: folder) }
            recordingState = .failed("Recording could not start: \(error.localizedDescription)")
        }
    }

    private func registerAudioSessionObservers() {
        let center = NotificationCenter.default
        audioSessionObservers = [
            center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] notification in
                guard let typeValue = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                      let type = AVAudioSession.InterruptionType(rawValue: typeValue) else { return }
                Task { @MainActor [weak self] in
                    switch type {
                    case .began: self?.finishInterruptedRecording(reason: "a call or another app took the microphone")
                    // Replay could not take the audio session during the interruption.
                    case .ended: self?.retryReplayIfNeeded()
                    @unknown default: break
                    }
                }
            },
            center.addObserver(forName: AVAudioSession.mediaServicesWereResetNotification, object: nil, queue: .main) { [weak self] _ in
                Task { @MainActor [weak self] in
                    self?.recoverFromMediaServicesReset()
                }
            },
            // .ended is not always delivered, so coming back to the app retries too.
            center.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
                Task { @MainActor [weak self] in
                    self?.retryReplayIfNeeded()
                }
            },
        ]
        observeEngineConfiguration()
    }

    private func observeEngineConfiguration() {
        if let engineObserver { NotificationCenter.default.removeObserver(engineObserver) }
        engineObserver = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: audioEngine, queue: .main) { [weak self] _ in
            Task { @MainActor [weak self] in
                // A real hardware change stops the engine. One that leaves it
                // running, such as turning voice processing on before start,
                // has not interrupted anything.
                guard let self, !self.audioEngine.isRunning else { return }
                self.handleEngineConfigurationChange()
            }
        }
    }

    /// After a media-services reset every audio object is dead, so the engine
    /// and the player are rebuilt rather than reused.
    private func recoverFromMediaServicesReset() {
        finishInterruptedRecording(reason: "the iPad’s audio system restarted")
        audioEngine = AVAudioEngine()
        observeEngineConfiguration()
        player = nil
        retryReplayIfNeeded()
    }

    /// An interruption ends the recording like Stop does, and the normal
    /// after-Stop path (replay, transcription, a missing speech model, a
    /// failure) carries on; the note says when and why it stopped.
    private func finishInterruptedRecording(reason: String) {
        guard isRecording else { return }
        let framesWritten = finishAudioCapture()
        interruptionNote = "Recording stopped at \(Self.elapsedTime(framesWritten)) because \(reason). The audio up to then is saved."
        recordingState = .stopped
        guard writeExports() else { return }
        // During a call the session cannot be reactivated; replay retries later.
        prepareReplay(reportFailure: false)
        transcribeSavedAudio()
    }

    /// The engine stops itself when its I/O unit reconfigures. Voice processing
    /// does that right after start on the same microphone, so the same input
    /// port restarts capture into the same file; only a different input, or a
    /// microphone that will not settle, ends the recording.
    private func handleEngineConfigurationChange() {
        guard isRecording, let file = audioFile else { return }
        guard Self.currentInputPort() == recordingInputPort else {
            finishInterruptedRecording(reason: "the microphone changed")
            return
        }
        guard engineRestarts < Self.maxEngineRestarts else {
            finishInterruptedRecording(reason: "the microphone kept reconfiguring")
            return
        }
        do {
            audioEngine.inputNode.removeTap(onBus: 0)
            try installCaptureTap(writingTo: file)
            audioEngine.prepare()
            try audioEngine.start()
            engineRestarts += 1
            // The check describes the configuration that actually recorded.
            recordingSession = RecordingSessionState.current(engine: audioEngine)
        } catch {
            finishInterruptedRecording(reason: "the microphone could not restart after it reconfigured")
        }
    }

    /// Reads the input format now, because voice processing can change it.
    private func installCaptureTap(writingTo file: AVAudioFile) throws {
        let input = audioEngine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)
        guard let converter = AVAudioConverter(from: inputFormat, to: file.processingFormat) else {
            throw CocoaError(.featureUnsupported)
        }
        input.installTap(onBus: 0, bufferSize: 4_096, format: inputFormat, block: Self.makeTapBlock(file: file, converter: converter, meter: levelMeter))
    }

    private static func currentInputPort() -> String? {
        guard let input = AVAudioSession.sharedInstance().currentRoute.inputs.first else { return nil }
        return "\(input.portType.rawValue):\(input.uid)"
    }

    private func restoreInputGain() {
        guard let gain = inputGainToRestore else { return }
        try? AVAudioSession.sharedInstance().setInputGain(gain)
        inputGainToRestore = nil
    }

    private func retryReplayIfNeeded() {
        guard player == nil, !isRecording, recordingState != .ready, audioURL != nil else { return }
        prepareReplay(reportFailure: false)
    }

    @discardableResult
    private func finishAudioCapture() -> AVAudioFramePosition {
        audioEngine.inputNode.removeTap(onBus: 0)
        audioEngine.stop()
        if audioEngine.inputNode.isVoiceProcessingEnabled {
            try? audioEngine.inputNode.setVoiceProcessingEnabled(false)
        }
        let framesWritten = audioFile?.length ?? 0
        audioFile = nil
        UIApplication.shared.isIdleTimerDisabled = false
        inputLevel = nil
        saveRecordingCheck(framesWritten: framesWritten)
        restoreInputGain()
        return framesWritten
    }

    @discardableResult
    private func prepareReplay(reportFailure: Bool) -> Bool {
        guard let audioURL else { return false }
        do {
            // A record-only session cannot play the replay.
            try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default)
            try AVAudioSession.sharedInstance().setActive(true)
            // The room is recorded quietly; replay plays a boosted copy held in
            // memory so speech is easy to hear. The saved file is unchanged.
            let replay = try Self.boostedReplay(of: audioURL)
            player = try replay.map { try AVAudioPlayer(data: $0.data) } ?? AVAudioPlayer(contentsOf: audioURL)
            replayGainDB = replay?.gainDB ?? 0
            player?.prepareToPlay()
            return true
        } catch {
            if reportFailure {
                recordingState = .failed("The audio was saved, but replay could not open it: \(error.localizedDescription)")
            }
            return false
        }
    }

    /// nil for an empty recording, which plays as saved.
    private static func boostedReplay(of url: URL) throws -> (data: Data, gainDB: Double)? {
        let file = try AVAudioFile(forReading: url)
        let frames = AVAudioFrameCount(file.length)
        guard frames > 0, let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: frames) else { return nil }
        try file.read(into: buffer)
        guard let channel = buffer.floatChannelData?[0] else { return nil }
        let samples = Array(UnsafeBufferPointer(start: channel, count: Int(buffer.frameLength)))
        let gain = ReplayBoost.gainDB(speechDBFS: ReplayBoost.speechLevel(samples))
        let data = ReplayBoost.wavData(ReplayBoost.boosted(samples, gainDB: gain), sampleRate: Int(file.processingFormat.sampleRate))
        return (data, gain)
    }

    private static func elapsedTime(_ framesWritten: AVAudioFramePosition) -> String {
        let seconds = max(0, Int(framesWritten / AVAudioFramePosition(fileSampleRate)))
        return "\(seconds / 60):\(String(format: "%02d", seconds % 60))"
    }

    /// Built outside the main actor: the engine calls the tap on its audio
    /// thread, and a main-actor closure would trap there under Swift 6.
    nonisolated private static func makeTapBlock(file: AVAudioFile, converter: AVAudioConverter, meter: LevelMeter) -> AVAudioNodeTapBlock {
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
                meter.add(converted)
            }
        }
    }

    func refreshInputLevel() {
        guard isRecording else { return }
        inputLevel = levelMeter.latestWindowRMS
    }

    /// recording-check.json beside the audio: the requested mode, the session
    /// the system actually gave, and the level of what was written.
    private func saveRecordingCheck(framesWritten: AVAudioFramePosition) {
        guard let meetingURL, let recordingSession else { return }
        let check = RecordingCheck(
            requestedMode: recordingMode.rawValue,
            session: recordingSession,
            durationSeconds: Double(framesWritten) / Self.fileSampleRate,
            engineRestarts: engineRestarts,
            level: levelMeter.summary
        )
        lastRecordingCheck = check
        self.recordingSession = nil
        try? write(check, to: meetingURL.appendingPathComponent("recording-check.json"))
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
