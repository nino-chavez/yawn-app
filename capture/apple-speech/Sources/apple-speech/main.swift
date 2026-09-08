import AVFoundation
import CoreMedia
import Darwin
import Foundation
import Speech

private enum HelperError: LocalizedError {
    case usage(String)
    case unavailable(String)
    case invalidAudio(String)

    var errorDescription: String? {
        switch self {
        case .usage(let message), .unavailable(let message), .invalidAudio(let message):
            return message
        }
    }
}

private enum Command { case capabilities, installAssets, transcribe }

private struct Arguments {
    let command: Command
    let locale: String
    let output: URL
    let audio: URL?

    init() throws {
        var values = ArraySlice(CommandLine.arguments.dropFirst())
        var command: Command?
        var locale = "en-US"
        var output: URL?
        var audio: URL?
        while let flag = values.popFirst() {
            switch flag {
            case "--capabilities":
                guard command == nil else { throw HelperError.usage("exactly one command is required") }
                command = .capabilities
            case "--install-assets":
                guard command == nil else { throw HelperError.usage("exactly one command is required") }
                command = .installAssets
            case "--transcribe":
                guard command == nil else { throw HelperError.usage("exactly one command is required") }
                command = .transcribe
            case "--locale":
                guard let value = values.popFirst(), !value.isEmpty else { throw HelperError.usage("--locale requires a BCP-47 identifier") }
                locale = value
            case "--audio":
                guard let value = values.popFirst(), value.hasPrefix("/") else { throw HelperError.usage("--audio requires an absolute WAV path") }
                audio = URL(fileURLWithPath: value)
            case "--out":
                guard let value = values.popFirst(), value.hasPrefix("/") else { throw HelperError.usage("--out requires an absolute path") }
                output = URL(fileURLWithPath: value)
            default: throw HelperError.usage("unrecognized arguments")
            }
        }
        guard let command, let output else { throw HelperError.usage("command and --out are required") }
        if command == .transcribe && audio == nil { throw HelperError.usage("--transcribe requires --audio") }
        if command != .transcribe && audio != nil { throw HelperError.usage("--audio is only accepted by --transcribe") }
        self.command = command; self.locale = locale; self.output = output; self.audio = audio
    }
}

private struct Capability: Codable {
    let schema: String
    let state: String
    let reason: String?
    let locale: String
    let osVersion: String
    let assetIdentity: String
}

private struct Segment: Codable { let text: String; let start: Double; let end: Double }
private struct Utterance: Codable { let text: String; let start: Double; let end: Double }
private struct Run: Codable {
    let `repeat`: Int; let processSeconds: Double; let totalSeconds: Double; let text: String
    let segments: [Segment]; let utterances: [Utterance]; let timingIssueCount: Int
    let processPeakRssBytes: UInt64; let timingGranularity: String
}
private struct Result: Codable {
    let schema: String; let engine: String; let engineVersion: String?; let modelIdentity: String
    let locale: String; let setupSeconds: Double; let runs: [Run]; let status: String
    let error: String?; let memoryScope: String
}

private func writeJSON<T: Encodable>(_ value: T, to output: URL) throws {
    let encoder = JSONEncoder(); encoder.outputFormatting = [.sortedKeys]; encoder.keyEncodingStrategy = .convertToSnakeCase
    let data = try encoder.encode(value)
    try data.write(to: output, options: .atomic)
    try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: output.path)
}
private func monotonicSeconds() -> Double { Double(DispatchTime.now().uptimeNanoseconds) / 1_000_000_000 }
private func peakRSS() -> UInt64 { var value = rusage(); return getrusage(RUSAGE_SELF, &value) == 0 ? UInt64(value.ru_maxrss) : 0 }

private struct ValidatedAudio {
    static func open(_ url: URL) throws -> AVAudioFile {
        guard url.pathExtension.lowercased() == "wav" else { throw HelperError.invalidAudio("audio must be a WAV file") }
        let file = try AVAudioFile(forReading: url); let format = file.fileFormat; let description = format.streamDescription.pointee
        guard format.channelCount == 1, format.sampleRate == 16_000, description.mFormatID == kAudioFormatLinearPCM,
              description.mBitsPerChannel == 16, (description.mFormatFlags & kAudioFormatFlagIsSignedInteger) != 0
        else { throw HelperError.invalidAudio("audio must be mono 16 kHz PCM16 WAV") }
        return file
    }
}

private func capability(locale identifier: String, reason: String? = nil) async -> Capability {
    let unavailable: (String) -> Capability = { Capability(schema: "apple-speech-capability/1", state: "unavailable", reason: $0, locale: identifier, osVersion: ProcessInfo.processInfo.operatingSystemVersionString, assetIdentity: "os-managed") }
    #if arch(arm64)
    guard #available(macOS 26.0, *) else { return unavailable("Apple SpeechTranscriber requires macOS 26 or later") }
    if let reason { return unavailable(reason) }
    return await speechCapability(locale: identifier)
    #else
    return unavailable("Apple SpeechTranscriber requires Apple silicon")
    #endif
}

@available(macOS 26.0, *)
private func speechCapability(locale identifier: String) async -> Capability {
    let requested = Locale(identifier: identifier)
    guard SpeechTranscriber.isAvailable else { return Capability(schema: "apple-speech-capability/1", state: "unavailable", reason: "SpeechTranscriber is unavailable on this Mac", locale: identifier, osVersion: ProcessInfo.processInfo.operatingSystemVersionString, assetIdentity: "os-managed") }
    guard let resolved = await SpeechTranscriber.supportedLocale(equivalentTo: requested) else { return Capability(schema: "apple-speech-capability/1", state: "unavailable", reason: "requested locale is unsupported", locale: identifier, osVersion: ProcessInfo.processInfo.operatingSystemVersionString, assetIdentity: "os-managed") }
    let transcriber = SpeechTranscriber(locale: resolved, transcriptionOptions: [], reportingOptions: [], attributeOptions: [.audioTimeRange])
    let installed = await SpeechTranscriber.installedLocales
    let ready = transcriber.selectedLocales.allSatisfy { selected in installed.contains { $0.identifier(.bcp47) == selected.identifier(.bcp47) } }
    return Capability(schema: "apple-speech-capability/1", state: ready ? "ready" : "assets-required", reason: ready ? nil : "Apple on-device speech assets must be installed explicitly", locale: resolved.identifier(.bcp47), osVersion: ProcessInfo.processInfo.operatingSystemVersionString, assetIdentity: "os-managed")
}

@available(macOS 26.0, *)
private func installAssets(locale identifier: String) async throws {
    let status = await speechCapability(locale: identifier)
    guard status.state != "unavailable" else { throw HelperError.unavailable(status.reason ?? "Apple SpeechTranscriber is unavailable") }
    guard status.state == "assets-required" else { return }
    let locale = Locale(identifier: status.locale)
    let transcriber = SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [], attributeOptions: [.audioTimeRange])
    guard let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) else { throw HelperError.unavailable("Apple did not provide an asset installation request") }
    try await request.downloadAndInstall()
    let final = await speechCapability(locale: identifier)
    guard final.state == "ready" else { throw HelperError.unavailable(final.reason ?? "Apple speech assets are not ready") }
}

@available(macOS 26.0, *)
private func timedSegments(_ text: AttributedString) -> ([Segment], Int) {
    var values: [Segment] = []; var issues = 0
    for run in text.runs {
        let fragment = String(text[run.range].characters).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !fragment.isEmpty else { continue }
        guard let range = run.audioTimeRange else { issues += 1; continue }
        let start = CMTimeGetSeconds(range.start); let end = CMTimeGetSeconds(CMTimeRangeGetEnd(range))
        guard start.isFinite, end.isFinite, end > start else { issues += 1; continue }
        values.append(Segment(text: fragment, start: start, end: end))
    }
    return (values, issues)
}

@available(macOS 26.0, *)
private func transcribe(_ url: URL, locale identifier: String) async throws -> Result {
    let status = await speechCapability(locale: identifier)
    guard status.state == "ready" else { throw HelperError.unavailable(status.reason ?? "Apple speech assets are not ready") }
    let started = monotonicSeconds(); let file = try ValidatedAudio.open(url); let locale = Locale(identifier: status.locale)
    let transcriber = SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [], attributeOptions: [.audioTimeRange])
    let analyzer = SpeechAnalyzer(modules: [transcriber])
    let collector = Task { () throws -> [(String, [Segment], Utterance?, Int)] in
        var final: [(String, [Segment], Utterance?, Int)] = []
        for try await item in transcriber.results where item.isFinal {
            let text = String(item.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
            let start = CMTimeGetSeconds(item.range.start); let end = CMTimeGetSeconds(CMTimeRangeGetEnd(item.range))
            let utterance = !text.isEmpty && start.isFinite && end.isFinite && end > start ? Utterance(text: text, start: start, end: end) : nil
            let timed = timedSegments(item.text)
            final.append((text, timed.0, utterance, timed.1 + ((text.isEmpty || utterance != nil) ? 0 : 1)))
        }
        return final
    }
    try await analyzer.prepareToAnalyze(in: nil)
    try await analyzer.start(inputAudioFile: file, finishAfterFile: true)
    try await analyzer.finalizeAndFinishThroughEndOfInput()
    let final = try await collector.value; let elapsed = monotonicSeconds() - started
    let run = Run(repeat: 0, processSeconds: elapsed, totalSeconds: elapsed, text: final.map(\.0).joined(separator: " ").trimmingCharacters(in: .whitespacesAndNewlines), segments: final.flatMap(\.1), utterances: final.compactMap(\.2), timingIssueCount: final.reduce(0) { $0 + $1.3 }, processPeakRssBytes: peakRSS(), timingGranularity: "SpeechTranscriber final-result attributed spans (audioTimeRange); process_seconds includes WAV open and validation")
    return Result(schema: "speech-engine-result/1", engine: "apple", engineVersion: ProcessInfo.processInfo.operatingSystemVersionString, modelIdentity: "Apple SpeechTranscriber on-device asset for \(status.locale); OS-managed version not exposed", locale: status.locale, setupSeconds: 0, runs: [run], status: "ok", error: nil, memoryScope: "process-only; Apple system service not included")
}

@main private struct AppleSpeech {
    static func main() async {
        var parsed: Arguments?
        do {
            let arguments = try Arguments(); parsed = arguments
            switch arguments.command {
            case .capabilities: try writeJSON(await capability(locale: arguments.locale), to: arguments.output)
            case .installAssets:
                guard #available(macOS 26.0, *) else { throw HelperError.unavailable("Apple SpeechTranscriber requires macOS 26 or later") }
                let current = await capability(locale: arguments.locale)
                guard current.state != "unavailable" else { throw HelperError.unavailable(current.reason ?? "Apple SpeechTranscriber is unavailable") }
                try await installAssets(locale: arguments.locale)
                try writeJSON(await capability(locale: arguments.locale), to: arguments.output)
            case .transcribe:
                guard #available(macOS 26.0, *) else { throw HelperError.unavailable("Apple SpeechTranscriber requires macOS 26 or later") }
                let current = await capability(locale: arguments.locale)
                guard current.state == "ready" else { throw HelperError.unavailable(current.reason ?? "Apple speech assets are not ready") }
                try writeJSON(await transcribe(arguments.audio!, locale: arguments.locale), to: arguments.output)
            }
        } catch {
            if let arguments = parsed {
                switch arguments.command {
                case .capabilities, .installAssets:
                    try? writeJSON(await capability(locale: arguments.locale, reason: error.localizedDescription), to: arguments.output)
                case .transcribe:
                    let failure = Result(schema: "speech-engine-result/1", engine: "apple", engineVersion: nil, modelIdentity: "not available", locale: arguments.locale, setupSeconds: 0, runs: [], status: "error", error: "Apple speech transcription failed", memoryScope: "process-only; Apple system service not included")
                    try? writeJSON(failure, to: arguments.output)
                }
            }
            fputs("apple-speech failed\n", stderr)
            exit(1)
        }
    }
}
