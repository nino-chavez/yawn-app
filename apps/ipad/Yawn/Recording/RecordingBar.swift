import SwiftUI

/// While recording: the state, the elapsed time and Stop, together and in the
/// recording color, and nothing else (experience brief, Hierarchy).
struct RecordingBadge: View {
    let startedAt: Date
    let onStop: () -> Void

    var body: some View {
        HStack(spacing: 14) {
            TimelineView(.periodic(from: startedAt, by: 1)) { context in
                HStack(spacing: 8) {
                    Circle().fill(.red).frame(width: 10, height: 10)
                    Text("Recording")
                        .font(.subheadline.weight(.semibold))
                    Text(MeetingRecorder.clock(context.date.timeIntervalSince(startedAt)))
                        .font(.subheadline.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
                .accessibilityElement(children: .combine)
            }
            Button(action: onStop) {
                Label("Stop", systemImage: "stop.fill")
                    .font(.subheadline.weight(.semibold))
                    .labelStyle(.titleAndIcon)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.capsule)
            .tint(.red)
        }
    }
}

/// One status, small, in the bar: preparing, finishing.
struct BarStatus: View {
    let text: String

    var body: some View {
        HStack(spacing: 8) {
            ProgressView()
            Text(text)
                .font(.subheadline.weight(.medium))
        }
    }
}
