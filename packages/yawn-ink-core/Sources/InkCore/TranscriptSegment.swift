import Foundation

public struct TranscriptSegment: Codable, Equatable, Identifiable, Sendable {
    public let start: Double
    public let end: Double
    public let text: String
    public let isFinal: Bool

    public var id: String { "\(start)-\(end)-\(isFinal)" }

    public init(start: Double, end: Double, text: String, isFinal: Bool) {
        self.start = start
        self.end = end
        self.text = text
        self.isFinal = isFinal
    }

    enum CodingKeys: String, CodingKey {
        case start
        case end
        case text
        case isFinal = "final"
    }
}
