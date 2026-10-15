import Foundation

/// The keep-period ledger. It lives in Application Support, which the Files app
/// cannot reach, so editing or deleting a meeting's meeting.json in Files never
/// stops its audio from being deleted. Moved from the spike's MeetingStore so
/// the Yawn app and its tests share one implementation.
public struct RetentionLedger: Sendable {
    public struct Entry: Codable, Equatable, Sendable {
        public let meetingID: String
        public let createdAt: Date
        public let keepDays: Int

        public init(meetingID: String, createdAt: Date, keepDays: Int) {
            self.meetingID = meetingID
            self.createdAt = createdAt
            self.keepDays = keepDays
        }
    }

    public let ledgerURL: URL
    public let meetingsFolder: URL

    public init(ledgerURL: URL, meetingsFolder: URL) {
        self.ledgerURL = ledgerURL
        self.meetingsFolder = meetingsFolder
    }

    /// Reads the ledger. A ledger that reads but will not decode is set aside,
    /// and a damaged or missing one is rebuilt from the meetings' own
    /// meeting.json files. Only folders still named by a meeting ID are rebuilt,
    /// so a folder renamed or duplicated in Files stays out of the sweep.
    public func load(now: Date = Date()) throws -> [Entry] {
        let files = FileManager.default
        if files.fileExists(atPath: ledgerURL.path) {
            // A failed read throws; only an undecodable ledger is set aside.
            let data = try Data(contentsOf: ledgerURL)
            if let entries = try? MeetingExportEncoder.decode([Entry].self, from: data) {
                return entries
            }
            let stamp = ISO8601DateFormatter().string(from: now).replacingOccurrences(of: ":", with: "-")
            try files.moveItem(at: ledgerURL, to: ledgerURL.deletingLastPathComponent().appendingPathComponent("retention-ledger.damaged-\(stamp).json"))
        }
        let folders = (try? files.contentsOfDirectory(at: meetingsFolder, includingPropertiesForKeys: nil)) ?? []
        let rebuilt = folders.compactMap { folder -> Entry? in
            guard UUID(uuidString: folder.lastPathComponent) != nil,
                  let data = try? Data(contentsOf: folder.appendingPathComponent("meeting.json")),
                  let meeting = try? MeetingExportEncoder.decode(MeetingExport.self, from: data) else { return nil }
            // meeting.json can be edited in Files. A keep period outside 1, 7 or 30
            // days becomes 1, and a future date becomes now: bad data deletes sooner.
            let keepDays = [1, 7, 30].contains(meeting.keepDays) ? meeting.keepDays : 1
            return Entry(meetingID: folder.lastPathComponent, createdAt: min(meeting.createdAt, now), keepDays: keepDays)
        }.sorted { $0.meetingID < $1.meetingID }
        if !rebuilt.isEmpty {
            try save(rebuilt)
        }
        return rebuilt
    }

    /// Adds or replaces one meeting's entry; written before any audio exists.
    public func record(_ entry: Entry) throws {
        let entries = try load().filter { $0.meetingID != entry.meetingID } + [entry]
        try save(entries)
    }

    /// Deletes audio.wav for every meeting past its keep period. The page,
    /// strokes and transcript stay. Returns the meeting IDs whose audio went.
    @discardableResult
    public func sweep(now: Date = Date()) throws -> [String] {
        let records = try load(now: now).map {
            MeetingRetentionRecord(meetingID: $0.meetingID, createdAt: $0.createdAt, keepDays: $0.keepDays)
        }
        var removed: [String] = []
        for meetingID in AudioRetentionSweep.meetingIDsToDeleteAudio(for: records, now: now) {
            let audio = meetingsFolder.appendingPathComponent(meetingID, isDirectory: true).appendingPathComponent("audio.wav")
            if FileManager.default.fileExists(atPath: audio.path) {
                try FileManager.default.removeItem(at: audio)
                removed.append(meetingID)
            }
        }
        return removed
    }

    private func save(_ entries: [Entry]) throws {
        try FileManager.default.createDirectory(at: ledgerURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try MeetingExportEncoder.data(for: entries).write(to: ledgerURL, options: .atomic)
    }
}
