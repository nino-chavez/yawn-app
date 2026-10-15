import Foundation
import InkCore

/// One folder per meeting in Documents/meetings; the keep-period ledger in
/// Application Support, out of reach of the Files app.
enum MeetingStorage {
    static var meetingsFolder: URL {
        URL.documentsDirectory.appendingPathComponent("meetings", isDirectory: true)
    }

    static func ledger() -> RetentionLedger {
        RetentionLedger(
            ledgerURL: URL.applicationSupportDirectory.appendingPathComponent("retention-ledger.json"),
            meetingsFolder: meetingsFolder
        )
    }

    static func makeMeetingFolder() throws -> URL {
        let folder = meetingsFolder.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    static func write<T: Encodable>(_ value: T, to url: URL) throws {
        try MeetingExportEncoder.data(for: value).write(to: url, options: .atomic)
    }

    static func read<T: Decodable>(_ type: T.Type, from url: URL) -> T? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? MeetingExportEncoder.decode(type, from: data)
    }

    /// Newest first. The folders are the source of truth.
    static func summaries() -> [MeetingSummary] {
        let folders = (try? FileManager.default.contentsOfDirectory(at: meetingsFolder, includingPropertiesForKeys: nil)) ?? []
        return folders.compactMap { folder -> MeetingSummary? in
            guard UUID(uuidString: folder.lastPathComponent) != nil,
                  let meeting = read(MeetingExport.self, from: folder.appendingPathComponent("meeting.json")) else { return nil }
            let report = read(RecordingReport.self, from: folder.appendingPathComponent("recording-report.json"))
            return MeetingSummary(
                id: folder.lastPathComponent,
                createdAt: meeting.createdAt,
                durationSeconds: report?.durationSeconds,
                transcription: report?.transcription,
                finalSegments: report?.finalSegments ?? 0,
                audioKept: FileManager.default.fileExists(atPath: folder.appendingPathComponent("audio.wav").path)
            )
        }
        .sorted { $0.createdAt > $1.createdAt }
    }
}

struct MeetingSummary: Identifiable, Equatable {
    let id: String
    let createdAt: Date
    let durationSeconds: Double?
    /// nil when the recording never finished (no report was written).
    let transcription: RecordingReport.Transcription?
    let finalSegments: Int
    let audioKept: Bool
}
