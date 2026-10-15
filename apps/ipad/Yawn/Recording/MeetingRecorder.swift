import AVFoundation
import InkCore
import Speech
import UIKit

/// Records one meeting at a time: the room to audio.wav in measurement mode,
/// transcribed live, kept for the chosen period. A recording never ends
/// silently: an interruption stops it with a note, and the audio up to then is
/// kept and transcribed like any other.
@MainActor
final class MeetingRecorder: ObservableObject {
    enum Phase: Equatable {
        case idle
        case preparing
        case recording
        case finishing
        /// The last meeting is saved; its transcript may be empty if nobody spoke.
        case transcriptReady
        /// The last meeting is saved but has no transcript: the speech model
        /// is not installed.
        case needsSpeechModel
        /// The last meeting is saved, the model is installed, but transcription
        /// failed. Transcribe again retries it.
        case transcriptionFailed
        case failed(String)
    }

    enum SpeechModel: Equatable {
        case checking, installed, installable, installing, unavailable
    }

    @Published private(set) var phase: Phase = .idle
    @Published private(set) var startedAt: Date?
    @Published private(set) var lastDuration: Double?
    @Published private(set) var transcript: [TranscriptSegment] = []
    /// Why the last recording stopped early; shown until the next one starts.
    @Published private(set) var interruptionNote: String?
    @Published private(set) var speechModel: SpeechModel = .checking
    @Published private(set) var meetings: [MeetingSummary] = []

    var isRecording: Bool { phase == .recording }

    /// A new recording can start: nothing is preparing, recording or finishing.
    var canStart: Bool {
        phase != .preparing && phase != .recording && phase != .finishing
    }

    private static let fileSampleRate = 16_000.0
    private static let maxEngineRestarts = 3

    private var engine = AVAudioEngine()
    private var engineObserver: NSObjectProtocol?
    private var sessionObservers: [NSObjectProtocol] = []
    private var audioFile: AVAudioFile?
    private var meetingFolder: URL?
    private var live: LiveTranscriber?
    private let meter = LevelMeter()
    private var inputPort: String?
    private var engineRestarts = 0
    private var batteryStart: Int?

    init() {
        UIDevice.current.isBatteryMonitoringEnabled = true
        registerSessionObservers()
        sweepExpiredAudio()
        Task { await refreshSpeechModel() }
    }

    /// Deletes audio past its keep period. Runs at launch, whenever the app
    /// comes to the front, and before every recording: the app can stay in
    /// memory for days, so launch alone would keep audio too long.
    func sweepExpiredAudio() {
        _ = try? MeetingStorage.ledger().sweep()
        if phase != .recording { meetings = MeetingStorage.summaries() }
    }

    // MARK: Speech model

    func refreshSpeechModel() async {
        speechModel = switch await AssetInventory.status(forModules: [LiveTranscriber.makeTranscriber()]) {
        case .installed: .installed
        case .supported: .installable
        case .downloading: .installing
        case .unsupported: .unavailable
        @unknown default: .unavailable
        }
    }

    func installSpeechModel() {
        Task {
            speechModel = .installing
            do {
                if let request = try await AssetInventory.assetInstallationRequest(supporting: [LiveTranscriber.makeTranscriber()]) {
                    try await request.downloadAndInstall()
                }
            } catch {}
            await refreshSpeechModel()
            if speechModel == .installed, phase == .needsSpeechModel {
                await transcribeAgain()
            }
        }
    }

    /// Retries the last meeting's transcript from its saved audio.
    func transcribeAgain() async {
        guard let folder = meetingFolder, phase == .needsSpeechModel || phase == .transcriptionFailed else { return }
        await transcribeAfterStop(folder: folder, ending: nil)
    }

    // MARK: Recording

    func start(keepDays: Int) async {
        guard canStart else { return }
        sweepExpiredAudio()
        phase = .preparing
        interruptionNote = nil
        transcript = []
        guard await AVAudioApplication.requestRecordPermission() else {
            phase = .failed("Yawn can’t use the microphone. Allow it in Settings › Privacy & Security › Microphone.")
            return
        }
        var folder: URL?
        do {
            // Measurement mode keeps system processing to a minimum; replay is
            // boosted instead (docs/evidence/ipad-mic-mode-ab-2026-10-15.json).
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement)
            try session.setActive(true)
            engine = AVAudioEngine()
            observeEngineConfiguration()

            let meetingFolder = try MeetingStorage.makeMeetingFolder()
            folder = meetingFolder
            // meeting.json and the ledger entry exist before any audio does, so
            // the keep-period sweep can always find what this meeting records.
            let created = Date()
            try MeetingStorage.write(MeetingExport(createdAt: created, consentEveryoneAudible: true, keepDays: keepDays, audioSampleRate: Int(Self.fileSampleRate)),
                                     to: meetingFolder.appendingPathComponent("meeting.json"))
            try MeetingStorage.ledger().record(.init(meetingID: meetingFolder.lastPathComponent, createdAt: created, keepDays: keepDays))

            let file = try AVAudioFile(forWriting: meetingFolder.appendingPathComponent("audio.wav"), settings: [
                AVFormatIDKey: kAudioFormatLinearPCM,
                AVSampleRateKey: Self.fileSampleRate,
                AVNumberOfChannelsKey: 1,
                AVLinearPCMBitDepthKey: 16,
                AVLinearPCMIsFloatKey: false,
                AVLinearPCMIsBigEndianKey: false,
            ], commonFormat: .pcmFormatFloat32, interleaved: false)
            live = await LiveTranscriber.start(inputFormat: engine.inputNode.outputFormat(forBus: 0))
            meter.reset()
            engineRestarts = 0
            try installTap(writingTo: file)
            engine.prepare()
            try engine.start()

            inputPort = Self.currentInputPort()
            batteryStart = Self.batteryPercent()
            self.meetingFolder = meetingFolder
            audioFile = file
            startedAt = Date()
            UIApplication.shared.isIdleTimerDisabled = true
            phase = .recording
            meetings = MeetingStorage.summaries()
        } catch {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            live?.feeder.finish()
            live = nil
            Self.releaseAudioSession()
            UIApplication.shared.isIdleTimerDisabled = false
            // A meeting that never started keeps no audio behind.
            if let folder { try? FileManager.default.removeItem(at: folder) }
            phase = .failed("Recording could not start: \(error.localizedDescription)")
        }
    }

    func stop() {
        end(.stopped, note: nil)
    }

    /// Every way a recording ends comes through here.
    private func end(_ ending: RecordingReport.Ending, note: String?) {
        guard phase == .recording, let folder = meetingFolder else { return }
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        let frames = audioFile?.length ?? 0
        // Finalizes the WAV header now, before anything reads the file back.
        audioFile?.close()
        audioFile = nil
        Self.releaseAudioSession()
        UIApplication.shared.isIdleTimerDisabled = false
        lastDuration = Double(frames) / Self.fileSampleRate
        if let note {
            interruptionNote = "Recording stopped at \(Self.clock(lastDuration ?? 0)) because \(note). The audio up to then is saved."
        }
        phase = .finishing
        Task { await transcribeAfterStop(folder: folder, ending: ending) }
    }

    /// Prefers the live transcript; falls back to transcribing the saved file.
    /// `ending` is nil for a retry after installing the speech model.
    private func transcribeAfterStop(folder: URL, ending: RecordingReport.Ending?) async {
        phase = .finishing
        let stoppedAt = Date()
        var transcription = RecordingReport.Transcription.none
        var segments: [TranscriptSegment]?
        if let live {
            segments = await live.finish()
            self.live = nil
            if segments != nil { transcription = .live }
        }
        if segments == nil {
            await refreshSpeechModel()
            if speechModel == .installed {
                segments = try? await LiveTranscriber.transcribeFile(at: folder.appendingPathComponent("audio.wav"))
                if segments != nil { transcription = .afterStop }
            }
        }
        let readySeconds = segments == nil ? nil : Date().timeIntervalSince(stoppedAt)
        let finals = segments ?? []
        try? MeetingStorage.write(finals, to: folder.appendingPathComponent("transcript.json"))
        transcript = finals
        if let ending {
            let report = RecordingReport(
                durationSeconds: lastDuration ?? 0,
                ending: ending,
                engineRestarts: engineRestarts,
                transcription: transcription,
                transcriptReadySeconds: readySeconds,
                finalSegments: finals.count,
                batteryStartPercent: batteryStart,
                batteryEndPercent: Self.batteryPercent(),
                level: meter.summary
            )
            try? MeetingStorage.write(report, to: folder.appendingPathComponent("recording-report.json"))
        } else if var report = MeetingStorage.read(RecordingReport.self, from: folder.appendingPathComponent("recording-report.json")) {
            report = RecordingReport(durationSeconds: report.durationSeconds, ending: report.ending, engineRestarts: report.engineRestarts,
                                     transcription: transcription, transcriptReadySeconds: report.transcriptReadySeconds,
                                     finalSegments: finals.count, batteryStartPercent: report.batteryStartPercent,
                                     batteryEndPercent: report.batteryEndPercent, level: report.level)
            try? MeetingStorage.write(report, to: folder.appendingPathComponent("recording-report.json"))
        }
        if segments != nil {
            phase = .transcriptReady
        } else {
            await refreshSpeechModel()
            phase = speechModel == .installed ? .transcriptionFailed : .needsSpeechModel
        }
        meetings = MeetingStorage.summaries()
    }

    private func installTap(writingTo file: AVAudioFile) throws {
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard let converter = AVAudioConverter(from: format, to: file.processingFormat) else {
            throw CocoaError(.featureUnsupported)
        }
        live?.feeder.setInputFormat(format)
        input.installTap(onBus: 0, bufferSize: 4_096, format: format, block: Self.makeTapBlock(file: file, converter: converter, meter: meter, feeder: live?.feeder))
    }

    /// Built outside the main actor: the engine calls the tap on its audio
    /// thread, and a main-actor closure would trap there under Swift 6.
    nonisolated private static func makeTapBlock(file: AVAudioFile, converter: AVAudioConverter, meter: LevelMeter, feeder: LiveFeeder?) -> AVAudioNodeTapBlock {
        let outputFormat = converter.outputFormat
        return { buffer, _ in
            feeder?.feed(buffer)
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

    // MARK: Interruptions

    private func registerSessionObservers() {
        let center = NotificationCenter.default
        sessionObservers = [
            center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] notification in
                guard let value = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                      AVAudioSession.InterruptionType(rawValue: value) == .began else { return }
                Task { @MainActor [weak self] in
                    self?.end(.interrupted, note: "a call or another app took the microphone")
                }
            },
            center.addObserver(forName: AVAudioSession.mediaServicesWereResetNotification, object: nil, queue: .main) { [weak self] _ in
                Task { @MainActor [weak self] in
                    guard let self else { return }
                    self.end(.audioSystemReset, note: "the iPad’s audio system restarted")
                    // Every audio object is dead after a reset.
                    self.engine = AVAudioEngine()
                    self.observeEngineConfiguration()
                }
            },
        ]
    }

    private func observeEngineConfiguration() {
        if let engineObserver { NotificationCenter.default.removeObserver(engineObserver) }
        engineObserver = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: .main) { [weak self] _ in
            Task { @MainActor [weak self] in
                // A real change stops the engine; one that leaves it running
                // has interrupted nothing.
                guard let self, !self.engine.isRunning else { return }
                self.handleEngineConfigurationChange()
            }
        }
    }

    /// The same microphone reconfiguring restarts capture into the same file;
    /// a different microphone, or one that will not settle, ends the recording.
    private func handleEngineConfigurationChange() {
        guard phase == .recording, let file = audioFile else { return }
        guard Self.currentInputPort() == inputPort else {
            end(.microphoneChanged, note: "the microphone changed")
            return
        }
        guard engineRestarts < Self.maxEngineRestarts else {
            end(.microphoneUnsettled, note: "the microphone kept reconfiguring")
            return
        }
        do {
            engine.inputNode.removeTap(onBus: 0)
            try installTap(writingTo: file)
            engine.prepare()
            try engine.start()
            engineRestarts += 1
        } catch {
            end(.microphoneUnsettled, note: "the microphone could not restart after it reconfigured")
        }
    }

    // MARK: Helpers

    private static func currentInputPort() -> String? {
        guard let input = AVAudioSession.sharedInstance().currentRoute.inputs.first else { return nil }
        return "\(input.portType.rawValue):\(input.uid)"
    }

    /// Lets other apps' audio resume once Yawn is not recording.
    private static func releaseAudioSession() {
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    private static func batteryPercent() -> Int? {
        let level = UIDevice.current.batteryLevel
        return level < 0 ? nil : Int((level * 100).rounded())
    }

    static func clock(_ seconds: Double) -> String {
        let total = max(0, Int(seconds))
        let hours = total / 3_600, minutes = (total % 3_600) / 60, secs = total % 60
        return hours > 0 ? String(format: "%d:%02d:%02d", hours, minutes, secs) : String(format: "%d:%02d", minutes, secs)
    }
}
