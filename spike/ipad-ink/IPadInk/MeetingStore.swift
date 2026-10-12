import AVFoundation
import InkCore
import PencilKit
import Speech

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
    @Published private(set) var playbackTime = 0.0

    private let audioEngine = AVAudioEngine()
    private var audioFile: AVAudioFile?
    private var meetingURL: URL?
    private var audioURL: URL?
    private var recordingStartedAt: Date?
    private var player: AVAudioPlayer?

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

    var playbackDuration: Double {
        player?.duration ?? 0
    }

    var displayedDrawing: PKDrawing {
        guard !isRecording, player != nil else { return drawing }
        let visible = StrokeTimeline.visibleStrokes(timedStrokes, at: playbackTime)
        let strokes = drawing.strokes.enumerated().compactMap { visible.contains($0.offset) ? $0.element : nil }
        return PKDrawing(strokes: strokes)
    }

    func startRecording() {
        guard canStart, let keepDays else { return }
        AVAudioSession.sharedInstance().requestRecordPermission { [weak self] granted in
            Task { @MainActor in
                guard let self else { return }
                guard granted else {
                    self.recordingState = .failed("Microphone access was not allowed.")
                    return
                }
                self.beginRecording(keepDays: keepDays)
            }
        }
    }

    func stopRecording() {
        guard isRecording else { return }
        audioEngine.inputNode.removeTap(onBus: 0)
        audioEngine.stop()
        audioFile = nil
        recordingState = .stopped
        writeExports()

        if let audioURL {
            do {
                player = try AVAudioPlayer(contentsOf: audioURL)
                player?.prepareToPlay()
            } catch {
                recordingState = .failed("The audio was saved, but replay could not open it: \(error.localizedDescription)")
            }
        }
        transcribeAfterStop()
    }

    func addStrokes(_ strokes: [PKStroke], drawing: PKDrawing) {
        self.drawing = drawing
        let recordingStart = recordingStartedAt
        let additions = strokes.enumerated().map { offset, stroke in
            let start = recordingStart.map { StrokeTimeline.seconds(forStrokeCreatedAt: stroke.path.creationDate, recordingStartedAt: $0) }
            return TimedStroke(index: timedStrokes.count + offset, startSeconds: start, endSeconds: start)
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
        guard let seconds = timedStrokes.first(where: { $0.index == index })?.startSeconds else { return }
        seek(to: seconds)
    }

    func refreshPlaybackTime() {
        playbackTime = player?.currentTime ?? 0
    }

    func installSpeechModel() {
        Task {
            let transcriber = SpeechTranscriber(locale: .current, preset: .timeIndexedProgressiveTranscription)
            do {
                guard let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) else {
                    await refreshSpeechModelStatus()
                    return
                }
                speechModelStatus = "Installing Apple’s on-device speech model…"
                try await request.downloadAndInstall()
                await refreshSpeechModelStatus()
            } catch {
                speechModelStatus = "Speech model installation failed: \(error.localizedDescription)"
            }
        }
    }

    func refreshSpeechModelStatus() async {
        let transcriber = SpeechTranscriber(locale: .current, preset: .timeIndexedProgressiveTranscription)
        let status = await AssetInventory.status(forModules: [transcriber])
        speechModelStatus = switch status {
        case .installed: "Apple’s on-device speech model is installed."
        case .supported: "Apple’s on-device speech model is available to install."
        case .downloading: "Apple’s on-device speech model is downloading."
        case .unsupported: "This locale does not have an on-device speech model."
        @unknown default: "Speech model status is unavailable."
        }
    }

    private func beginRecording(keepDays: Int) {
        do {
            try AVAudioSession.sharedInstance().setCategory(.record, mode: .measurement)
            try AVAudioSession.sharedInstance().setActive(true)

            let folder = try makeMeetingFolder()
            let destination = folder.appendingPathComponent("audio.wav")
            let settings: [String: Any] = [
                AVFormatIDKey: kAudioFormatLinearPCM,
                AVSampleRateKey: 16_000,
                AVNumberOfChannelsKey: 1,
                AVLinearPCMBitDepthKey: 16,
                AVLinearPCMIsFloatKey: false,
                AVLinearPCMIsBigEndianKey: false
            ]
            let file = try AVAudioFile(forWriting: destination, settings: settings)
            let input = audioEngine.inputNode
            input.installTap(onBus: 0, bufferSize: 1_024, format: input.outputFormat(forBus: 0)) { buffer, _ in
                try? file.write(from: buffer)
            }
            audioEngine.prepare()
            try audioEngine.start()

            meetingURL = folder
            audioURL = destination
            audioFile = file
            recordingStartedAt = Date()
            recordingState = .recording
            let meeting = MeetingExport(createdAt: recordingStartedAt ?? Date(), consentEveryoneAudible: true, keepDays: keepDays, audioSampleRate: 16_000)
            try write(meeting, to: folder.appendingPathComponent("meeting.json"))
        } catch {
            recordingState = .failed("Recording could not start: \(error.localizedDescription)")
        }
    }

    private func transcribeAfterStop() {
        guard let audioURL else { return }
        Task {
            let transcriber = SpeechTranscriber(locale: .current, preset: .timeIndexedProgressiveTranscription)
            let status = await AssetInventory.status(forModules: [transcriber])
            guard status == .installed else {
                recordingState = .needsSpeechModel
                return
            }

            do {
                recordingState = .transcribing
                let audioFile = try AVAudioFile(forReading: audioURL)
                let analyzer = try await SpeechAnalyzer(inputAudioFile: audioFile, modules: [transcriber])
                let readResults = Task { [weak self] in
                    for try await result in transcriber.results {
                        guard let self else { return }
                        let start = result.range.start.seconds
                        let end = result.range.end.seconds
                        let segment = TranscriptSegment(start: start, end: end, text: String(result.text.characters), isFinal: result.isFinal)
                        await self.merge(segment)
                    }
                }
                try await analyzer.start(inputAudioFile: audioFile, finishAfterFile: true)
                _ = try await readResults.value
                recordingState = .stopped
                writeExports()
            } catch {
                recordingState = .failed("Transcription could not finish: \(error.localizedDescription)")
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

    private func writeExports() {
        guard let meetingURL else { return }
        do {
            try drawing.dataRepresentation().write(to: meetingURL.appendingPathComponent("drawing.data"), options: .atomic)
            try write(timedStrokes, to: meetingURL.appendingPathComponent("strokes.json"))
            try write(transcript, to: meetingURL.appendingPathComponent("transcript.json"))
        } catch {
            recordingState = .failed("The meeting could not be exported: \(error.localizedDescription)")
        }
    }

    private func write<T: Encodable>(_ value: T, to url: URL) throws {
        try MeetingExportEncoder.data(for: value).write(to: url, options: .atomic)
    }

    private func removeExpiredAudio() {
        guard let documents = try? FileManager.default.url(for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: false) else { return }
        let meetings = documents.appendingPathComponent("meetings", isDirectory: true)
        let folders = (try? FileManager.default.contentsOfDirectory(at: meetings, includingPropertiesForKeys: nil)) ?? []
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let records = folders.compactMap { folder -> MeetingRetentionRecord? in
            guard let data = try? Data(contentsOf: folder.appendingPathComponent("meeting.json")), let meeting = try? decoder.decode(MeetingExport.self, from: data) else {
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
