import Foundation

public struct MeetingExport: Codable, Equatable, Sendable {
    public let schema: String
    public let createdAt: Date
    public let consentEveryoneAudible: Bool
    public let keepDays: Int
    public let audioSampleRate: Int

    public init(createdAt: Date, consentEveryoneAudible: Bool, keepDays: Int, audioSampleRate: Int) {
        schema = "ipad-ink-meeting/1"
        self.createdAt = createdAt
        self.consentEveryoneAudible = consentEveryoneAudible
        self.keepDays = keepDays
        self.audioSampleRate = audioSampleRate
    }

    enum CodingKeys: String, CodingKey {
        case schema
        case createdAt = "created_at"
        case consentEveryoneAudible = "consent_everyone_audible"
        case keepDays = "keep_days"
        case audioSampleRate = "audio_sample_rate"
    }
}
