import Foundation
import Testing
@testable import InkCore

@Test("retention selects expired audio and preserves unexpired audio")
func retentionSelectsOnlyExpiredAudio() {
    let now = Date(timeIntervalSince1970: 2_000_000_000)
    let expired = MeetingRetentionRecord(meetingID: "expired", createdAt: now.addingTimeInterval(-86_401), keepDays: 1)
    let current = MeetingRetentionRecord(meetingID: "current", createdAt: now.addingTimeInterval(-86_399), keepDays: 1)

    #expect(AudioRetentionSweep.meetingIDsToDeleteAudio(for: [expired, current], now: now) == ["expired"])
}

@Test("zero-second ink and exact thirty-day retention boundaries are inclusive")
func boundariesAreInclusive() {
    let started = Date(timeIntervalSince1970: 1_000)
    #expect(StrokeTimeline.seconds(forStrokeCreatedAt: started, recordingStartedAt: started) == 0)

    let now = started.addingTimeInterval(30 * 86_400)
    let meeting = MeetingRetentionRecord(meetingID: "thirty", createdAt: started, keepDays: 30)
    #expect(AudioRetentionSweep.meetingIDsToDeleteAudio(for: [meeting], now: now) == ["thirty"])
}

@Test("final transcript replaces a volatile result for the same audio range")
func finalTranscriptReplacesVolatileResult() {
    let volatile = TranscriptSegment(start: 2, end: 4, text: "Good morn", isFinal: false)
    let final = TranscriptSegment(start: 2, end: 4, text: "Good morning", isFinal: true)
    let merged = TranscriptTimeline.merging(TranscriptTimeline.merging([], with: volatile), with: final)

    #expect(merged == [final])
}

@Test("a final result replaces in-progress text whose range differs")
func finalReplacesOverlappingVolatileWithDifferentRange() {
    var merged: [TranscriptSegment] = []
    merged = TranscriptTimeline.merging(merged, with: TranscriptSegment(start: 2, end: 3.1, text: "Good", isFinal: false))
    merged = TranscriptTimeline.merging(merged, with: TranscriptSegment(start: 2, end: 3.9, text: "Good morn", isFinal: false))
    let final = TranscriptSegment(start: 2, end: 4.2, text: "Good morning", isFinal: true)
    merged = TranscriptTimeline.merging(merged, with: final)

    #expect(merged == [final])
}

@Test("newer in-progress text replaces older in-progress text and keeps earlier finals")
func volatileReplacesVolatileAndKeepsFinals() {
    let firstFinal = TranscriptSegment(start: 0, end: 1.8, text: "Hello there.", isFinal: true)
    var merged = TranscriptTimeline.merging([], with: firstFinal)
    merged = TranscriptTimeline.merging(merged, with: TranscriptSegment(start: 2, end: 2.5, text: "We", isFinal: false))
    let newest = TranscriptSegment(start: 2, end: 3.4, text: "We should", isFinal: false)
    merged = TranscriptTimeline.merging(merged, with: newest)

    #expect(merged == [firstFinal, newest])
}

@Test("a late in-progress result never duplicates final text for the same audio")
func lateVolatileDoesNotDuplicateFinal() {
    let final = TranscriptSegment(start: 2, end: 4.2, text: "Good morning", isFinal: true)
    let merged = TranscriptTimeline.merging([final], with: TranscriptSegment(start: 2, end: 4.2, text: "Good morn", isFinal: false))

    #expect(merged == [final])
}

@Test("in-progress text that straddles a final's end still replaces older in-progress text")
func straddlingVolatileIsKept() {
    let final = TranscriptSegment(start: 2, end: 4.2, text: "Good morning", isFinal: true)
    let older = TranscriptSegment(start: 4.2, end: 5, text: "and", isFinal: false)
    let newer = TranscriptSegment(start: 4.1, end: 6, text: "and then we", isFinal: false)
    let merged = TranscriptTimeline.merging([final, older], with: newer)

    #expect(merged == [final, newer])
}

@Test("late in-progress text spanning two finals is dropped")
func lateVolatileSpanningFinalsIsDropped() {
    let first = TranscriptSegment(start: 2, end: 4.2, text: "Good morning", isFinal: true)
    let second = TranscriptSegment(start: 4.2, end: 6, text: "and then we", isFinal: true)
    let merged = TranscriptTimeline.merging([first, second], with: TranscriptSegment(start: 2, end: 6, text: "Good morning and then we", isFinal: false))

    #expect(merged == [first, second])
}

@Test("a finished transcript keeps final text only")
func completedTranscriptKeepsFinalsOnly() {
    let final = TranscriptSegment(start: 0, end: 2, text: "Hello there.", isFinal: true)
    let leftover = TranscriptSegment(start: 2, end: 3, text: "We sh", isFinal: false)

    #expect(TranscriptTimeline.completed([final, leftover]) == [final])
}

@Test("replay includes strokes at the playhead and untimed strokes")
func replayVisibilityUsesInclusivePlayhead() {
    let strokes = [
        TimedStroke(index: 0, startSeconds: 0),
        TimedStroke(index: 1, startSeconds: 2),
        TimedStroke(index: 2, startSeconds: nil)
    ]
    #expect(StrokeTimeline.visibleStrokes(strokes, at: 0) == [0, 2])
    #expect(StrokeTimeline.visibleStrokes(strokes, at: 2) == [0, 1, 2])
}

@Test("erasing, undoing, and redoing preserves each stroke's original timing")
func eraseUndoRedoPreservesStrokeTiming() {
    let recordingStartedAt = Date(timeIntervalSince1970: 1_000)
    let first = strokeSample(offset: 1, seed: 11, duration: 0.2)
    let middle = strokeSample(offset: 2, seed: 22, duration: 0.3)
    let last = strokeSample(offset: 3, seed: 33, duration: 0.4)
    var ledger = StrokeLedger()

    ledger.reconcile([first, middle, last], recordingStartedAt: recordingStartedAt)
    ledger.reconcile([first, last], recordingStartedAt: recordingStartedAt)
    #expect(ledger.liveStrokes(order: [first.key, last.key]) == [
        TimedStroke(index: 0, startSeconds: 1, endSeconds: 1.2),
        TimedStroke(index: 1, startSeconds: 3, endSeconds: 3.4)
    ])

    ledger.reconcile([first, middle, last], recordingStartedAt: recordingStartedAt)
    #expect(ledger.liveStrokes(order: [first.key, middle.key, last.key]) == [
        TimedStroke(index: 0, startSeconds: 1, endSeconds: 1.2),
        TimedStroke(index: 1, startSeconds: 2, endSeconds: 2.3),
        TimedStroke(index: 2, startSeconds: 3, endSeconds: 3.4)
    ])

    ledger.reconcile([first, last], recordingStartedAt: recordingStartedAt)
    #expect(ledger.liveStrokes(order: [first.key, last.key]) == [
        TimedStroke(index: 0, startSeconds: 1, endSeconds: 1.2),
        TimedStroke(index: 1, startSeconds: 3, endSeconds: 3.4)
    ])
}

@Test("strokes drawn before recording remain untimed")
func strokesBeforeRecordingRemainUntimed() {
    let sample = strokeSample(offset: 1, seed: 11, duration: 0.2)
    var ledger = StrokeLedger()

    ledger.reconcile([sample], recordingStartedAt: nil)

    #expect(ledger.liveStrokes(order: [sample.key]) == [
        TimedStroke(index: 0, startSeconds: nil, endSeconds: nil)
    ])
}

@Test("reconciling an unchanged drawing is idempotent")
func reconcileIsIdempotent() {
    let recordingStartedAt = Date(timeIntervalSince1970: 1_000)
    let sample = strokeSample(offset: 1, seed: 11, duration: 0.2)
    var ledger = StrokeLedger()

    ledger.reconcile([sample], recordingStartedAt: recordingStartedAt)
    let firstResult = ledger.liveStrokes(order: [sample.key])
    ledger.reconcile([sample], recordingStartedAt: recordingStartedAt.addingTimeInterval(10))

    #expect(ledger.liveStrokes(order: [sample.key]) == firstResult)
}

private func strokeSample(offset: TimeInterval, seed: UInt32, duration: Double) -> StrokeSample {
    let creationDate = Date(timeIntervalSince1970: 1_000 + offset)
    let key = StrokeKey(creationDate: creationDate, randomSeed: seed)
    return StrokeSample(key: key, creationDate: creationDate, duration: duration)
}

@Test("export JSON has exactly the documented keys")
func exportJSONKeysAreExact() throws {
    let meeting = MeetingExport(createdAt: .distantPast, consentEveryoneAudible: true, keepDays: 7, audioSampleRate: 16_000)
    let stroke = TimedStroke(index: 0, startSeconds: 1.25)
    let transcript = TranscriptSegment(start: 1, end: 2, text: "A note", isFinal: true)

    let meetingObject = try #require(JSONSerialization.jsonObject(with: MeetingExportEncoder.data(for: meeting)) as? [String: Any])
    let strokeObject = try #require(JSONSerialization.jsonObject(with: MeetingExportEncoder.data(for: [stroke])) as? [[String: Any]])
    let transcriptObject = try #require(JSONSerialization.jsonObject(with: MeetingExportEncoder.data(for: [transcript])) as? [[String: Any]])

    #expect(Set(meetingObject.keys) == ["schema", "created_at", "consent_everyone_audible", "keep_days", "audio_sample_rate"])
    #expect(Set(try #require(strokeObject.first).keys) == ["index", "start_seconds", "end_seconds"])
    #expect(Set(try #require(transcriptObject.first).keys) == ["start", "end", "text", "final"])
}

/// 1 kHz at 16 kHz: 16 samples a period, so every 100 ms window holds whole periods.
private func sine(amplitude: Float, windows: Int) -> [Float] {
    (0..<(windows * LevelAccumulator.windowSamples)).map { amplitude * Float(sin(2 * Double.pi * 1_000 * Double($0) / 16_000)) }
}

@Test("a full-scale sine reads about -3 dBFS RMS and counts its peaks as clipped")
func fullScaleSineLevel() {
    var level = LevelAccumulator()
    level.add(sine(amplitude: 1, windows: 10))
    let summary = level.summary
    #expect(abs(summary.rmsDBFS - -3.01) < 0.05)
    #expect(abs(summary.windowRMSP90DBFS - -3.01) < 0.05)
    #expect(summary.peakDBFS == 0)
    #expect(summary.clippedSamples > 0)
    #expect(summary.windows == 10)
}

@Test("a 0.1-amplitude sine reads about -23 dBFS and nothing clips")
func quietSineLevel() {
    var level = LevelAccumulator()
    level.add(sine(amplitude: 0.1, windows: 5))
    let summary = level.summary
    #expect(abs(summary.rmsDBFS - -23.01) < 0.05)
    #expect(abs(summary.peakDBFS - -20) < 0.05)
    #expect(summary.clippedSamples == 0)
    #expect(abs(level.latestWindowRMS - -23.01) < 0.05)
}

@Test("silence reports the floor, never -infinity, and encodes as finite JSON")
func silenceStaysFinite() throws {
    var level = LevelAccumulator()
    level.add([Float](repeating: 0, count: 3 * LevelAccumulator.windowSamples))
    let summary = level.summary
    #expect(summary.rmsDBFS == LevelAccumulator.floorDBFS)
    #expect(summary.speechMinusNoiseDB == 0)
    let empty = LevelAccumulator().summary
    #expect(empty.windowRMSP90DBFS == LevelAccumulator.floorDBFS)
    let json = String(decoding: try MeetingExportEncoder.data(for: summary), as: UTF8.self)
    #expect(!json.lowercased().contains("inf"))
}

@Test("speech minus noise is the gap between loud and quiet windows")
func speechMinusNoiseGap() {
    var level = LevelAccumulator()
    level.add(sine(amplitude: 0.01, windows: 5))  // about -43 dBFS
    level.add(sine(amplitude: 0.5, windows: 5))   // about -9 dBFS
    let summary = level.summary
    #expect(abs(summary.windowRMSP10DBFS - -43.01) < 0.05)
    #expect(abs(summary.windowRMSP90DBFS - -9.03) < 0.05)
    #expect(abs(summary.speechMinusNoiseDB - 33.98) < 0.1)
}

@Test("one clipped sample is counted")
func singleClippedSample() {
    var level = LevelAccumulator()
    var samples = [Float](repeating: 0.1, count: LevelAccumulator.windowSamples)
    samples[100] = -1
    level.add(samples)
    #expect(level.summary.clippedSamples == 1)
}

@Test("recording check JSON has exactly the documented keys")
func recordingCheckKeys() throws {
    let session = RecordingSessionState(category: "AVAudioSessionCategoryRecord", mode: "AVAudioSessionModeMeasurement", voiceProcessing: false, inputPortType: "MicrophoneBuiltIn", inputPortName: "iPad Microphone", dataSource: nil, polarPattern: nil, inputGainSettable: false, inputGain: 1, sessionSampleRate: 48_000, inputSampleRate: 48_000, inputChannels: 1)
    var level = LevelAccumulator()
    level.add(sine(amplitude: 0.1, windows: 1))
    let check = RecordingCheck(requestedMode: "measurement", session: session, durationSeconds: 0.1, engineRestarts: 0, level: level.summary)
    let object = try #require(JSONSerialization.jsonObject(with: MeetingExportEncoder.data(for: check)) as? [String: Any])
    #expect(Set(object.keys) == ["schema", "requested_mode", "session", "duration_seconds", "engine_restarts", "level"])
    #expect(object["schema"] as? String == "ipad-ink-recording-check/1")
    let levelKeys = Set((object["level"] as? [String: Any] ?? [:]).keys)
    #expect(levelKeys == ["window_seconds", "samples", "windows", "rms_dbfs", "peak_dbfs", "window_rms_p10_dbfs", "window_rms_p50_dbfs", "window_rms_p90_dbfs", "speech_minus_noise_db", "clipped_samples"])
    // A nil data source is omitted, not written as a fake value.
    #expect((object["session"] as? [String: Any])?["data_source"] == nil)
}
