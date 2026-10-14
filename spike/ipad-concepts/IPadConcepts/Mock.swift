import SwiftUI
import UIKit

// Every meeting, note, time and transcript line here is invented for the concept
// comparison. None of it comes from a real recording.

struct NoteLine: Identifiable {
    let id: Int
    /// Seconds into the recording when the line was written; nil if added after Stop.
    let seconds: Int?
    let text: String
    /// Horizontal start, in points from the page's writing edge.
    let indent: CGFloat
    /// A slight tilt, the way a finger writes.
    let tilt: Double
}

struct TranscriptTurn: Identifiable {
    let id: Int
    let seconds: Int
    let text: String
}

struct PastMeeting: Identifiable {
    let id: Int
    let day: String
    let time: String
    let minutes: Int
    let firstLines: [String]
    let group: String
}

enum Mock {
    static let meetingTitle = "Tue, Oct 14 · 10:02 AM"
    static let meetingLength = 1385 // 23:05

    static let lines: [NoteLine] = [
        NoteLine(id: 0, seconds: 18, text: "Garden plan — fall", indent: 36, tilt: -1.2),
        NoteLine(id: 1, seconds: 47, text: "beds 4–6: new soil", indent: 74, tilt: 0.8),
        NoteLine(id: 2, seconds: 82, text: "2 yards? delivered", indent: 104, tilt: -0.6),
        NoteLine(id: 3, seconds: 125, text: "Sam → quote by Fri", indent: 62, tilt: 1.4),
        NoteLine(id: 4, seconds: 171, text: "water timer broken", indent: 44, tilt: -1.6),
        NoteLine(id: 5, seconds: 210, text: "ask city: compost bins", indent: 80, tilt: 0.4),
        NoteLine(id: 6, seconds: 242, text: "shed lock!!", indent: 58, tilt: -2.0),
        NoteLine(id: 7, seconds: 465, text: "volunteer day Nov 8", indent: 40, tilt: 1.0),
        NoteLine(id: 8, seconds: 680, text: "budget $1,200 max", indent: 96, tilt: -0.8),
        NoteLine(id: 9, seconds: 902, text: "no vote if under", indent: 128, tilt: 0.6),
        NoteLine(id: 10, seconds: 1120, text: "photos to Dana", indent: 52, tilt: -1.0),
        NoteLine(id: 11, seconds: 1315, text: "next mtg Oct 28", indent: 46, tilt: 1.2),
    ]

    static let addedAfterStop = NoteLine(id: 12, seconds: nil, text: "call Sam Mon", indent: 70, tilt: -0.9)

    static let transcript: [TranscriptTurn] = [
        TranscriptTurn(id: 0, seconds: 4, text: "Okay, let's start with the beds along the fence."),
        TranscriptTurn(id: 1, seconds: 31, text: "Four through six are basically clay at this point."),
        TranscriptTurn(id: 2, seconds: 70, text: "We'd need maybe two yards of soil, and someone to deliver it."),
        TranscriptTurn(id: 3, seconds: 118, text: "Sam said he could get us a quote by Friday."),
        TranscriptTurn(id: 4, seconds: 164, text: "The water timer on the east side stopped working last week."),
        TranscriptTurn(id: 5, seconds: 202, text: "Does the city still give out compost bins? I think they do."),
        TranscriptTurn(id: 6, seconds: 238, text: "And the shed lock is broken again, so the tools just sit there."),
        TranscriptTurn(id: 7, seconds: 340, text: "Moving on. The volunteer day."),
        TranscriptTurn(id: 8, seconds: 451, text: "November eighth works for most people."),
        TranscriptTurn(id: 9, seconds: 545, text: "What's the most we can spend without going back to the board?"),
        TranscriptTurn(id: 10, seconds: 678, text: "If we keep it under twelve hundred, nobody has to vote on it."),
        TranscriptTurn(id: 11, seconds: 830, text: "That should cover the soil and the timer."),
        TranscriptTurn(id: 12, seconds: 982, text: "Can someone send Dana the photos from today?"),
        TranscriptTurn(id: 13, seconds: 1180, text: "I'll take care of that tonight."),
        TranscriptTurn(id: 14, seconds: 1308, text: "So, same time on the twenty-eighth?"),
        TranscriptTurn(id: 15, seconds: 1371, text: "Sounds good. Thanks, everyone."),
    ]

    static let pastMeetings: [PastMeeting] = [
        PastMeeting(id: 1, day: "Mon, Oct 13", time: "3:30 PM", minutes: 41, firstLines: ["Roof quote", "gutters too?"], group: "This week"),
        PastMeeting(id: 2, day: "Thu, Oct 9", time: "9:00 AM", minutes: 18, firstLines: ["Q4 schedule", "Ana out Nov"], group: "Last week"),
        PastMeeting(id: 3, day: "Tue, Oct 7", time: "1:15 PM", minutes: 52, firstLines: ["Budget draft", "3 options"], group: "Last week"),
    ]

    static let today = PastMeeting(id: 0, day: "Tue, Oct 14", time: "10:02 AM", minutes: 23, firstLines: ["Garden plan — fall", "beds 4–6: new soil"], group: "Today")

    // Points in time each state is set at.
    static let duringSeconds = 252 // 4:12
    static let pausedSeconds = 690 // 11:30
    static let tappedLine = 8 // "budget $1,200 max", written at 11:20
    static let interruptedSeconds = 401 // 6:41

    static func clock(_ seconds: Int) -> String {
        String(format: "%d:%02d", seconds / 60, seconds % 60)
    }
}

/// The handwriting stand-in. Falls back to another hand font, then the system font.
func inkFont(_ size: CGFloat) -> Font {
    for name in ["BradleyHandITCTT-Bold", "Noteworthy-Bold", "MarkerFelt-Wide"] where UIFont(name: name, size: size) != nil {
        return .custom(name, size: size)
    }
    return .system(size: size, weight: .medium, design: .rounded)
}
