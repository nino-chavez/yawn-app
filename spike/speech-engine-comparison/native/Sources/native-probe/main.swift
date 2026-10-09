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

/// How long Parakeet audio is split before decoding. `fluidaudio` is FluidAudio's own
/// ChunkProcessor (fixed ~15 s windows, 2 s overlap, merged by token timing) and stays the
/// default. `fixed` and `pause` are research arms: non-overlapping windows short enough for
/// FluidAudio's single-pass path, cut at a fixed length or at the quietest point near the end.
private enum Windowing: String {
    case fluidaudio
    case fixed
    case pause
    /// Pause cuts, each window decoded with 1 s of audio either side, joined at a word both
    /// neighboring windows agree on. Its windows begin and end mid-speech.
    case pauseContext = "pause-context"
    /// Windows that start and end at quiet points and overlap their neighbor by 1–4 s,
    /// joined at a word both windows agree on.
    case pauseOverlap = "pause-overlap"
}

private struct Arguments {
    let engine: Engine
    let audio: URL?
    let output: URL
    let repeats: Int
    let prepareOnly: Bool
    let windowing: Windowing

    init() throws {
        var values = ArraySlice(CommandLine.arguments.dropFirst())
        var engine: Engine?
        var audio: URL?
        var output: URL?
        var repeats = 2
        var prepareOnly = false
        var windowing = Windowing.fluidaudio

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
            case "--windowing":
                guard let raw = values.popFirst(), let parsed = Windowing(rawValue: raw) else {
                    throw ProbeError.usage("--windowing must be fluidaudio, fixed, pause, pause-context, or pause-overlap")
                }
                windowing = parsed
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
        if engine != .parakeet && windowing != .fluidaudio {
            throw ProbeError.usage("--windowing applies only to --engine parakeet")
        }
        self.engine = engine
        self.audio = audio
        self.output = output
        self.repeats = repeats
        self.prepareOnly = prepareOnly
        self.windowing = windowing
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
    let windowing: String?
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

private let probeSampleRate = 16_000
/// 186 encoder frames (14.88 s): under FluidAudio's 240,000-sample single-pass limit, so each
/// window is decoded whole rather than re-chunked by ChunkProcessor.
private let windowMaxSamples = 186 * ASRConstants.samplesPerEncoderFrame
private let pauseSearchStartSamples = 10 * probeSampleRate
private let minimumTailSamples = 2 * probeSampleRate
private let energyFrameSamples = 320  // 20 ms
private let quietSpanFrames = 10  // 200 ms

private func readMonoSamples(_ url: URL) throws -> [Float] {
    let file = try ValidatedAudio.open(url)
    guard let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length)) else {
        throw ProbeError.invalidAudio("could not allocate the audio buffer")
    }
    try file.read(into: buffer)
    guard let channel = buffer.floatChannelData?[0] else {
        throw ProbeError.invalidAudio("audio has no float channel data")
    }
    return Array(UnsafeBufferPointer(start: channel, count: Int(buffer.frameLength)))
}

/// Return the start of the quietest 200 ms span in `low..<high`, measured in 20 ms frames.
/// Ties resolve to the later span so windows stay as long as the limit allows.
private func quietestCut(_ samples: [Float], from low: Int, to high: Int) -> Int {
    let frames = (high - low) / energyFrameSamples
    guard frames > quietSpanFrames else { return high }
    var energies = [Double](repeating: 0, count: frames)
    for frame in 0..<frames {
        let start = low + frame * energyFrameSamples
        var sum = 0.0
        for index in start..<(start + energyFrameSamples) {
            let value = Double(samples[index])
            sum += value * value
        }
        energies[frame] = sum
    }
    var running = energies[0..<quietSpanFrames].reduce(0, +)
    var bestFrame = 0
    var bestEnergy = running
    for frame in 1...(frames - quietSpanFrames) {
        running += energies[frame + quietSpanFrames - 1] - energies[frame - 1]
        if running <= bestEnergy {
            bestEnergy = running
            bestFrame = frame
        }
    }
    // Cut in the middle of the quiet span, not at its edge.
    return low + (bestFrame + quietSpanFrames / 2) * energyFrameSamples
}

/// Audio decoded on each side of a pause-context cut, then discarded at the join.
private let contextSamples = 1 * probeSampleRate
/// Two windows agree on a word when the normalized text matches and the starts differ by at most this.
private let agreementToleranceSeconds = 0.25

/// Where two neighboring windows are joined, in file seconds: agreement is searched for in
/// `low...high`, preferring the agreeing word nearest `target`; with none, words split at `target`.
private struct JoinRegion {
    let low: Double
    let high: Double
    let target: Double
}

private struct PlannedWindow {
    let samples: Range<Int>
    let join: JoinRegion?
}

private func seconds(_ sample: Int) -> Double { Double(sample) / Double(probeSampleRate) }

/// Split the file into non-overlapping cores that tile it exactly. Pause-context cores are
/// shorter, so a core plus its context on both sides still fits the single-pass limit.
private func planCores(_ samples: [Float], windowing: Windowing) throws -> [Range<Int>] {
    let padded = windowing == .pauseContext
    let maxCore = padded ? windowMaxSamples - 2 * contextSamples : windowMaxSamples
    let searchStart = padded ? 8 * probeSampleRate : pauseSearchStartSamples
    var cores = [Range<Int>]()
    var start = 0
    let total = samples.count
    while start < total {
        if total - start <= maxCore {
            cores.append(start..<total)
            break
        }
        // Leave a tail long enough to decode on its own.
        let latest = min(start + maxCore, total - minimumTailSamples)
        let cut = windowing == .fixed
            ? latest
            : quietestCut(samples, from: start + searchStart, to: latest)
        cores.append(start..<cut)
        start = cut
    }
    // The plan must tile the file exactly, with every core within its limit.
    var expected = 0
    for core in cores {
        guard core.lowerBound == expected, !core.isEmpty, core.count <= maxCore else {
            throw ProbeError.invalidAudio("window plan does not tile the audio")
        }
        expected = core.upperBound
    }
    guard expected == total else { throw ProbeError.invalidAudio("window plan does not cover the audio") }
    return cores
}

/// Pause-overlap windows start and end at quiet points. Each next window starts at a quiet
/// point 1–4 s before the previous window ends, so neighbors share audio without either
/// window beginning or ending mid-speech.
private func planOverlappingWindows(_ samples: [Float]) throws -> [PlannedWindow] {
    var windows = [PlannedWindow]()
    var start = 0
    var previousEnd: Int?
    let total = samples.count
    while true {
        let join = previousEnd.map { end in
            JoinRegion(low: seconds(start), high: seconds(end), target: (seconds(start) + seconds(end)) / 2)
        }
        if total - start <= windowMaxSamples {
            windows.append(PlannedWindow(samples: start..<total, join: join))
            break
        }
        let latest = min(start + windowMaxSamples, total - minimumTailSamples)
        let end = quietestCut(samples, from: start + pauseSearchStartSamples, to: latest)
        windows.append(PlannedWindow(samples: start..<end, join: join))
        start = quietestCut(samples, from: end - 4 * probeSampleRate, to: end - 1 * probeSampleRate)
        previousEnd = end
    }
    // The plan must start at zero, end at the file's end, keep every window within the
    // single-pass limit, and make every neighbor pair overlap.
    guard windows.first?.samples.lowerBound == 0, windows.last?.samples.upperBound == total else {
        throw ProbeError.invalidAudio("window plan does not cover the audio")
    }
    for (index, window) in windows.enumerated() {
        guard !window.samples.isEmpty, window.samples.count <= windowMaxSamples else {
            throw ProbeError.invalidAudio("window plan exceeds the single-pass limit")
        }
        if index > 0 {
            let previous = windows[index - 1].samples
            guard window.samples.lowerBound > previous.lowerBound,
                  window.samples.lowerBound < previous.upperBound else {
                throw ProbeError.invalidAudio("window plan leaves neighbors without overlap")
            }
        }
    }
    return windows
}

private func planWindows(_ samples: [Float], windowing: Windowing) throws -> [PlannedWindow] {
    if windowing == .pauseOverlap { return try planOverlappingWindows(samples) }
    let cores = try planCores(samples, windowing: windowing)
    guard windowing == .pauseContext else {
        return cores.map { PlannedWindow(samples: $0, join: nil) }
    }
    let reach = seconds(contextSamples)
    return cores.enumerated().map { position, core in
        let window = max(0, core.lowerBound - contextSamples)..<min(samples.count, core.upperBound + contextSamples)
        let cut = seconds(core.lowerBound)
        return PlannedWindow(samples: window, join: position == 0 ? nil : JoinRegion(low: cut - reach, high: cut + reach, target: cut))
    }
}

private func agreementKey(_ word: String) -> String {
    word.lowercased().filter { $0.isLetter || $0.isNumber || $0 == "'" }
}

/// Join a window's words onto the transcript so far. Prefer the agreeing word nearest the
/// region's target; with no agreement, split by word start time at the target.
private func join(_ merged: inout [Segment], _ incoming: [Segment], in region: JoinRegion) -> Bool {
    var best: (left: Int, right: Int, distance: Double)?
    for left in merged.indices where merged[left].start >= region.low && merged[left].start <= region.high {
        let key = agreementKey(merged[left].text)
        guard !key.isEmpty else { continue }
        for right in incoming.indices where incoming[right].start >= region.low && incoming[right].start <= region.high {
            guard agreementKey(incoming[right].text) == key,
                  abs(incoming[right].start - merged[left].start) <= agreementToleranceSeconds else { continue }
            let distance = abs(merged[left].start - region.target)
            if best == nil || distance < best!.distance {
                best = (left, right, distance)
            }
        }
    }
    if let best {
        merged.removeSubrange((best.left + 1)...)
        merged += incoming[(best.right + 1)...]
        return true
    }
    merged.removeAll { $0.start >= region.target }
    merged += incoming.filter { $0.start >= region.target }
    return false
}

private func runParakeetWindowed(url: URL, manager: AsrManager, windowing: Windowing, index: Int, firstTotal: Double) async throws -> Run {
    let started = monotonicSeconds()
    let samples = try readMonoSamples(url)
    let plan = try planWindows(samples, windowing: windowing)
    let minimum = ASRConstants.minimumRequiredSamples(forSampleRate: probeSampleRate)
    var merged = [Segment]()
    var timingIssues = 0
    var joins = 0
    var agreedJoins = 0
    for planned in plan {
        let window = planned.samples
        guard window.count >= minimum else { continue }
        // Each window is decoded independently, from a fresh decoder state.
        var decoderState = try TdtDecoderState()
        let result = try await manager.transcribe(Array(samples[window]), decoderState: &decoderState)
        let offset = seconds(window.lowerBound)
        guard let timings = result.tokenTimings else {
            if !result.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { timingIssues += 1 }
            continue
        }
        let words = buildWordTimings(from: timings).map {
            Segment(text: $0.word, start: $0.startTime + offset, end: $0.endTime + offset)
        }
        if let region = planned.join {
            joins += 1
            if join(&merged, words, in: region) { agreedJoins += 1 }
        } else {
            merged += words
        }
    }
    let process = monotonicSeconds() - started
    let joinNote = joins == 0 ? "no overlap" : "\(agreedJoins) of \(joins) joins on an agreeing word"
    return Run(
        repeat: index,
        processSeconds: process,
        totalSeconds: index == 0 ? firstTotal + process : process,
        text: merged.map(\.text).joined(separator: " "),
        segments: merged,
        utterances: [],
        timingIssueCount: timingIssues,
        processPeakRssBytes: processPeakRSSBytes(),
        timingGranularity: "FluidAudio token timings grouped to words per \(windowing.rawValue) window (\(plan.count) windows, \(joinNote)), offset to file time; process_seconds includes WAV read and window planning"
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
                    memoryScope: "process-only; Apple system service not included",
                    windowing: nil
                )
            case .parakeet:
                let setup = try await prepareParakeet()
                var runs = [Run]()
                if !arguments.prepareOnly {
                    for index in 0..<arguments.repeats {
                        if arguments.windowing == .fluidaudio {
                            runs.append(try await runParakeet(url: arguments.audio!, manager: setup.manager, index: index, firstTotal: setup.setupSeconds))
                        } else {
                            runs.append(try await runParakeetWindowed(url: arguments.audio!, manager: setup.manager, windowing: arguments.windowing, index: index, firstTotal: setup.setupSeconds))
                        }
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
                    memoryScope: "process-only; Apple system service not included",
                    windowing: arguments.windowing.rawValue
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
                    memoryScope: "process-only; Apple system service not included",
                    windowing: arguments.engine == .parakeet ? arguments.windowing.rawValue : nil
                )
                try? outputResult(fallback, to: arguments.output)
            }
            fputs("native-probe failed: \(error.localizedDescription)\\n", stderr)
            exit(1)
        }
    }
}
