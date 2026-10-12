import Foundation

public struct TimedStroke: Codable, Equatable, Identifiable, Sendable {
    public let index: Int
    public let startSeconds: Double?
    public let endSeconds: Double?

    public var id: Int { index }

    public init(index: Int, startSeconds: Double?, endSeconds: Double? = nil) {
        self.index = index
        self.startSeconds = startSeconds
        self.endSeconds = endSeconds
    }

    enum CodingKeys: String, CodingKey {
        case index
        case startSeconds = "start_seconds"
        case endSeconds = "end_seconds"
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(index, forKey: .index)
        try container.encode(startSeconds, forKey: .startSeconds)
        try container.encode(endSeconds, forKey: .endSeconds)
    }
}
