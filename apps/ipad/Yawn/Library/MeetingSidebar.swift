import InkCore
import SwiftUI

/// The meeting list, newest first. Opening a meeting, the page picture, the
/// small rail and search arrive in slice 5.
struct MeetingSidebar: View {
    let meetings: [MeetingSummary]

    var body: some View {
        Group {
            if meetings.isEmpty {
                ContentUnavailableView(
                    "No meetings yet",
                    systemImage: "list.bullet.rectangle",
                    description: Text("Each meeting you record appears here, newest first.")
                )
            } else {
                List(meetings) { meeting in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(meeting.createdAt, format: .dateTime.weekday(.abbreviated).month(.abbreviated).day().hour().minute())
                            .font(.body.weight(.medium))
                        Text(Self.detail(meeting))
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                    .accessibilityElement(children: .combine)
                }
                .listStyle(.sidebar)
            }
        }
        .navigationTitle("Meetings")
    }

    static func detail(_ meeting: MeetingSummary) -> String {
        var parts: [String] = []
        if let seconds = meeting.durationSeconds {
            let minutes = Int((seconds / 60).rounded())
            parts.append(minutes >= 60 ? "\(minutes / 60) h \(minutes % 60) min" : "\(max(1, minutes)) min")
        }
        switch meeting.transcription {
        case nil: parts.append("Didn’t finish")
        case .none?: parts.append("Needs attention")
        case .live?, .afterStop?: parts.append(meeting.finalSegments > 0 ? "Transcript ready" : "No speech transcribed")
        }
        if !meeting.audioKept { parts.append("Audio released") }
        return parts.joined(separator: " · ")
    }
}
