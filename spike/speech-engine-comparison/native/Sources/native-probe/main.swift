import AVFoundation
import CoreMedia
import Darwin
import FluidAudio
import Foundation
import Speech

private enum ProbeError: LocalizedError {
    case usage(String)
    case invalidAudio(String)

    var errorDescription: String? {
        switch self {
        case .usage(let message), .invalidAudio(let message): return message
        }
    }
}

private enum Engine: String {
    case apple
    case parakeet
}

private struct Arguments {
    let engine: Engine
    let audio: URL?
    let output: URL
    let repeats: Int
    let prepareOnly: Bool

    init() throws {
        var values = ArraySlice(CommandLine.arguments.dropFirst())
        var engine: Engine?
        var audio: URL?
        var output: URL?
        var repeats = 2
        var prepareOnly = false

        while !values.isEmpty {
            let flag = values.removeFirst()
            switch flag {
            case "--engine":
                guard let raw = values.popFirst(), let parsed = Engine(rawValue: raw) else {
                    throw ProbeError.usage("--engine must be apple or parakeet")
                }
                engine = parsed
            case "--audio":
                guard let path = values.popFirst() else { throw ProbeError.usage("--audio requires an absolute WAV path") }
                audio = try Self.absoluteFileURL(path, flag: "--audio")
            case "--out":
                guard let path = values.popFirst() else { throw ProbeError.usage("--out requires an absolute result path") }
                output = try Self.absoluteFileURL(path, flag: "--out")
            case "--repeats":
                guard let raw = values.popFirst(), let parsed = Int(raw), parsed > 0 else {
                    throw ProbeError.usage("--repeats must be a positive integer")
                }
                repeats = parsed
            case "--prepare":
                prepareOnly = true
            default:
                throw ProbeError.usage("unknown argument: \(flag)")
            }
        }

        guard let engine else { throw ProbeError.usage("--engine is required") }
        guard let output else { throw ProbeError.usage("--out is required") }
        if !prepareOnly && audio == nil {
            throw ProbeError.usage("--audio is required unless --prepare is used")
        }
        if prepareOnly && audio != nil {
            throw ProbeError.usage("--prepare does not accept --audio")
        }
        self.engine = engine
        self.audio = audio
        self.output = output
        self.repeats = repeats
        self.prepareOnly = prepareOnly
    }

    private static func absoluteFileURL(_ path: String, flag: String) throws -> URL {
        guard path.hasPrefix("/") else { throw ProbeError.usage("\(flag) must be an absolute path") }
        return URL(fileURLWithPath: path)
    }
}

private struct Segment: Codable {
    let text: String
    let start: Double
    let end: Double
}

private struct Utterance: Codable {
    let text: String
    let start: Double
    let end: Double
}

private struct Run: Codable {
    let `repeat`: Int
    let processSeconds: Double
    let totalSeconds: Double
    let text: String
    let segments: [Segment]
    let utterances: [Utterance]
    let timingIssueCount: Int
    let processPeakRssBytes: UInt64
    let timingGranularity: String
}

private struct Result: Codable {
    let schema: String
    let engine: String
    let engineVersion: String?
    let modelIdentity: String
    let locale: String
    let setupSeconds: Double
    let runs: [Run]
    let status: String
    let error: String?
    let memoryScope: String
}

private func monotonicSeconds() -> Double {
    Double(DispatchTime.now().uptimeNanoseconds) / 1_000_000_000
}

private func processPeakRSSBytes() -> UInt64 {
    var usage = rusage()
    guard getrusage(RUSAGE_SELF, &usage) == 0 else { return 0 }
    // Darwin's ru_maxrss is bytes (unlike Linux, where it is KiB).
    return UInt64(usage.ru_maxrss)
}

private func outputResult(_ result: Result, to url: URL) throws {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    encoder.keyEncodingStrategy = .convertToSnakeCase
    let data = try encoder.encode(result)
    try data.write(to: url, options: .atomic)
    try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
}

private struct ValidatedAudio {
    static func open(_ url: URL) throws -> AVAudioFile {
        guard url.pathExtension.lowercased() == "wav" else {
            throw ProbeError.invalidAudio("audio must be a .wav file")
        }
        let file = try AVAudioFile(forReading: url)
        let format = file.fileFormat
        let description = format.streamDescription.pointee
        guard format.channelCount == 1,
              format.sampleRate == 16_000,
              description.mFormatID == kAudioFormatLinearPCM,
              description.mBitsPerChannel == 16,
              (description.mFormatFlags & kAudioFormatFlagIsSignedInteger) != 0 else {
            throw ProbeError.invalidAudio("audio must be mono 16 kHz PCM16 WAV")
        }
        return file
    }
}

private struct AppleSetup {
    let locale: Locale
    let setupSeconds: Double
}

private func prepareApple() async throws -> AppleSetup {
    let started = monotonicSeconds()
    guard SpeechTranscriber.isAvailable else {
        throw ProbeError.invalidAudio("SpeechTranscriber is unavailable on this Mac")
    }
    let locale = await SpeechTranscriber.supportedLocale(equivalentTo: Locale(identifier: "en-US"))
        ?? Locale(identifier: "en-US")
    let transcriber = SpeechTranscriber(
        locale: locale,
        transcriptionOptions: [],
        reportingOptions: [],
        attributeOptions: [.audioTimeRange]
    )
    let installed = await SpeechTranscriber.installedLocales
    let selected = transcriber.selectedLocales
    if !selected.allSatisfy({ selectedLocale in
        installed.contains { $0.identifier(.bcp47) == selectedLocale.identifier(.bcp47) }
    }), let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
        try await request.downloadAndInstall()
    }
    let analyzer = SpeechAnalyzer(modules: [transcriber])
    try await analyzer.prepareToAnalyze(in: nil)
    return AppleSetup(locale: locale, setupSeconds: monotonicSeconds() - started)
}

private func appleSegments(_ text: AttributedString) -> (segments: [Segment], timingIssueCount: Int) {
    var segments = [Segment]()
    var timingIssueCount = 0
    for run in text.runs {
        let fragment = String(text[run.range].characters).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !fragment.isEmpty else { continue }
        guard let range = run.audioTimeRange else {
            timingIssueCount += 1
            continue
        }
        let start = CMTimeGetSeconds(range.start)
        let end = CMTimeGetSeconds(CMTimeRangeGetEnd(range))
        guard start.isFinite, end.isFinite else {
            timingIssueCount += 1
            continue
        }
        segments.append(Segment(text: fragment, start: start, end: end))
    }
    return (segments, timingIssueCount)
}

private func runApple(url: URL, locale: Locale, index: Int, firstTotal: Double) async throws -> Run {
    // The file is reopened for every repeat. Its open/validation cost is deliberately included
    // in process_seconds along with decode, while the OS asset download/warm happens in setup.
    let started = monotonicSeconds()
    let file = try ValidatedAudio.open(url)
    let transcriber = SpeechTranscriber(
        locale: locale,
        transcriptionOptions: [],
        reportingOptions: [],
        attributeOptions: [.audioTimeRange]
    )
    let analyzer = SpeechAnalyzer(modules: [transcriber])
    let collector = Task { () throws -> [(String, [Segment], Utterance?, Int)] in
        var final = [(String, [Segment], Utterance?, Int)]()
        for try await item in transcriber.results where item.isFinal {
            let itemText = String(item.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
            let start = CMTimeGetSeconds(item.range.start)
            let end = CMTimeGetSeconds(CMTimeRangeGetEnd(item.range))
            let utterance = !itemText.isEmpty && start.isFinite && end.isFinite
                ? Utterance(text: itemText, start: start, end: end)
                : nil
            let timedSegments = appleSegments(item.text)
            let issueCount = timedSegments.timingIssueCount + (itemText.isEmpty || utterance != nil ? 0 : 1)
            final.append((itemText, timedSegments.segments, utterance, issueCount))
        }
        return final
    }
    try await analyzer.start(inputAudioFile: file, finishAfterFile: true)
    try await analyzer.finalizeAndFinishThroughEndOfInput()
    let results = try await collector.value
    let process = monotonicSeconds() - started
    let text = results.map(\.0).joined(separator: " ").trimmingCharacters(in: .whitespacesAndNewlines)
    return Run(
        repeat: index,
        processSeconds: process,
        totalSeconds: index == 0 ? firstTotal + process : process,
        text: text,
        segments: results.flatMap(\.1),
        utterances: results.compactMap(\.2),
        timingIssueCount: results.reduce(0) { $0 + $1.3 },
        processPeakRssBytes: processPeakRSSBytes(),
        timingGranularity: "SpeechTranscriber final-result attributed spans (audioTimeRange); process_seconds includes WAV open and validation"
    )
}

private struct ParakeetSetup {
    let manager: AsrManager
    let setupSeconds: Double
}

private func prepareParakeet() async throws -> ParakeetSetup {
    let started = monotonicSeconds()
    let models = try await AsrModels.downloadAndLoad(version: .v3, encoderPrecision: .int8)
    let manager = AsrManager(config: .default)
    try await manager.loadModels(models)
    return ParakeetSetup(manager: manager, setupSeconds: monotonicSeconds() - started)
}

private func runParakeet(url: URL, manager: AsrManager, index: Int, firstTotal: Double) async throws -> Run {
    let started = monotonicSeconds()
    _ = try ValidatedAudio.open(url) // Validate the comparable input before FluidAudio opens it.
    var decoderState = try TdtDecoderState()
    let transcript = try await manager.transcribe(url, decoderState: &decoderState)
    let process = monotonicSeconds() - started
    let segments = transcript.tokenTimings.map { timings in
        buildWordTimings(from: timings).map {
            Segment(text: $0.word, start: $0.startTime, end: $0.endTime)
        }
    } ?? []
    return Run(
        repeat: index,
        processSeconds: process,
        totalSeconds: index == 0 ? firstTotal + process : process,
        text: transcript.text.trimmingCharacters(in: .whitespacesAndNewlines),
        segments: segments,
        utterances: [],
        timingIssueCount: transcript.tokenTimings == nil && !transcript.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? 1 : 0,
        processPeakRssBytes: processPeakRSSBytes(),
        timingGranularity: transcript.tokenTimings == nil
            ? "unavailable: FluidAudio returned no token timings; process_seconds includes WAV open and validation"
            : "FluidAudio token timings grouped to words by FluidAudio buildWordTimings; process_seconds includes WAV open and validation"
    )
}

@main
private struct NativeProbe {
    static func main() async {
        do {
            let arguments = try Arguments()
            let result: Result
            switch arguments.engine {
            case .apple:
                let setup = try await prepareApple()
                var runs = [Run]()
                if !arguments.prepareOnly {
                    for index in 0..<arguments.repeats {
                        runs.append(try await runApple(url: arguments.audio!, locale: setup.locale, index: index, firstTotal: setup.setupSeconds))
                    }
                }
                result = Result(
                    schema: "speech-engine-result/1",
                    engine: "apple",
                    engineVersion: ProcessInfo.processInfo.operatingSystemVersionString,
                    modelIdentity: "Apple SpeechTranscriber on-device asset for \(setup.locale.identifier(.bcp47)); OS-managed version not exposed",
                    locale: setup.locale.identifier(.bcp47),
                    setupSeconds: setup.setupSeconds,
                    runs: arguments.prepareOnly ? [] : runs,
                    status: "ok",
                    error: nil,
                    memoryScope: "process-only; Apple system service not included"
                )
            case .parakeet:
                let setup = try await prepareParakeet()
                var runs = [Run]()
                if !arguments.prepareOnly {
                    for index in 0..<arguments.repeats {
                        runs.append(try await runParakeet(url: arguments.audio!, manager: setup.manager, index: index, firstTotal: setup.setupSeconds))
                    }
                }
                result = Result(
                    schema: "speech-engine-result/1",
                    engine: "parakeet",
                    engineVersion: "FluidAudio 0.15.6",
                    modelIdentity: "FluidAudio Parakeet TDT 0.6B v3, int8 encoder; model asset revision not exposed by API",
                    locale: "en-US",
                    setupSeconds: setup.setupSeconds,
                    runs: arguments.prepareOnly ? [] : runs,
                    status: "ok",
                    error: nil,
                    memoryScope: "process-only; Apple system service not included"
                )
            }
            try outputResult(result, to: arguments.output)
        } catch {
            // Transcript text is never printed. If --out was parsed, preserve the diagnostic there.
            if let arguments = try? Arguments() {
                let fallback = Result(
                    schema: "speech-engine-result/1",
                    engine: arguments.engine.rawValue,
                    engineVersion: nil,
                    modelIdentity: "not available",
                    locale: "unknown",
                    setupSeconds: 0,
                    runs: [],
                    status: "error",
                    error: error.localizedDescription,
                    memoryScope: "process-only; Apple system service not included"
                )
                try? outputResult(fallback, to: arguments.output)
            }
            fputs("native-probe failed: \(error.localizedDescription)\\n", stderr)
            exit(1)
        }
    }
}
