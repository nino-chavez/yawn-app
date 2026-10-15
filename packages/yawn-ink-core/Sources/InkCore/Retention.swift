import Foundation

public struct MeetingRetentionRecord: Equatable, Sendable {
    public let meetingID: String
    public let createdAt: Date
    public let keepDays: Int

    public init(meetingID: String, createdAt: Date, keepDays: Int) {
        self.meetingID = meetingID
        self.createdAt = createdAt
        self.keepDays = keepDays
    }
}

public enum AudioRetentionSweep {
    public static func meetingIDsToDeleteAudio(for meetings: [MeetingRetentionRecord], now: Date) -> [String] {
        meetings.compactMap { meeting in
            guard let expiry = Calendar(identifier: .gregorian).date(byAdding: .day, value: meeting.keepDays, to: meeting.createdAt) else {
                return nil
            }
            return now >= expiry ? meeting.meetingID : nil
        }
    }
}
