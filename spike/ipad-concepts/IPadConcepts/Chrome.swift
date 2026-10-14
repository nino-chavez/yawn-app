import SwiftUI

// Controls shared by the three concepts. Concepts differ in where these sit,
// not in what they say.

enum KeepPeriod: String, CaseIterable, Identifiable {
    case one = "1 day", seven = "7 days", thirty = "30 days"
    var id: String { rawValue }
}

struct RecordingBadge: View {
    var seconds: Int
    var body: some View {
        HStack(spacing: 8) {
            Circle().fill(Color.red).frame(width: 10, height: 10)
            Text("Recording")
                .font(.subheadline.weight(.semibold))
            Text(Mock.clock(seconds))
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}

struct StopButton: View {
    var body: some View {
        Button {
        } label: {
            Label("Stop", systemImage: "stop.fill")
                .font(.subheadline.weight(.semibold))
        }
        .buttonStyle(.borderedProminent)
        .tint(.red)
    }
}

struct ToolCluster: View {
    var vertical = false
    var body: some View {
        let layout = vertical ? AnyLayout(VStackLayout(spacing: 18)) : AnyLayout(HStackLayout(spacing: 18))
        layout {
            Button { } label: { Image(systemName: "pencil.tip").fontWeight(.semibold) }
                .accessibilityLabel("Pen")
                .foregroundStyle(Color.accentColor)
            Button { } label: { Image(systemName: "eraser") }
                .accessibilityLabel("Eraser")
            Button { } label: { Image(systemName: "arrow.uturn.backward") }
                .accessibilityLabel("Undo")
        }
        .font(.title3)
        .foregroundStyle(.primary)
    }
}

/// The consent and keep-period choices. Both start unmade, and Record stays
/// off until both are made (product brief: recording starts only after the
/// operator confirms consent).
struct ConsentFields: View {
    @Binding var agreed: Bool
    @Binding var keep: KeepPeriod?

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Toggle("Everyone who can be heard agreed to this recording", isOn: $agreed)
                .font(.body.weight(.medium))
            VStack(alignment: .leading, spacing: 8) {
                Text("Keep the audio for")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Picker("Keep the audio for", selection: $keep) {
                    ForEach(KeepPeriod.allCases) { Text($0.rawValue).tag(Optional($0)) }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
            }
            Label("Stays on this iPad", systemImage: "ipad.landscape")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }
}

struct RecordButton: View {
    var title = "Record"
    var large = false
    var enabled = true
    var body: some View {
        Button { } label: {
            Label(title, systemImage: "record.circle")
                .font(large ? .title3.weight(.semibold) : .body.weight(.semibold))
                .frame(maxWidth: large ? .infinity : nil)
                .padding(.vertical, large ? 6 : 0)
        }
        .buttonStyle(.borderedProminent)
        .tint(.red)
        .controlSize(large ? .large : .regular)
        .disabled(!enabled)
    }
}

struct ReplayControls: View {
    var seconds = Mock.pausedSeconds
    var compact = false
    var body: some View {
        HStack(spacing: 14) {
            Button { } label: { Image(systemName: "play.fill") }
                .accessibilityLabel("Play")
                .font(.title3)
            Text(Mock.clock(seconds))
                .font(.subheadline.monospacedDigit())
            ProgressView(value: Double(seconds), total: Double(Mock.meetingLength))
                .frame(minWidth: compact ? 140 : 220)
            Text(Mock.clock(Mock.meetingLength))
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(.secondary)
        }
    }
}

struct TranscriptTurnRow: View {
    var turn: TranscriptTurn
    var current: Bool
    var dimmed: Bool
    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(Mock.clock(turn.seconds))
                .font(.caption.monospacedDigit())
                .foregroundStyle(.secondary)
            Text(turn.text)
                .font(.body)
                .foregroundStyle(dimmed ? .secondary : .primary)
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            if current { RoundedRectangle(cornerRadius: 10).fill(Color.accentColor.opacity(0.12)) }
        }
    }
}

struct TranscriptList: View {
    var playhead: Int? = Mock.pausedSeconds
    var body: some View {
        let current = playhead.flatMap { p in Mock.transcript.last { $0.seconds <= p }?.id }
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    ForEach(Mock.transcript) { turn in
                        TranscriptTurnRow(turn: turn, current: turn.id == current, dimmed: playhead.map { turn.seconds > $0 } ?? false)
                            .id(turn.id)
                    }
                }
                .padding(.horizontal, 8)
            }
            .onAppear { if let current { proxy.scrollTo(current, anchor: .center) } }
        }
    }
}

struct InterruptionMessage: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Needs attention", systemImage: "exclamationmark.triangle.fill")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.orange)
            Text("Recording stopped at \(Mock.clock(Mock.interruptedSeconds)) when the iPad locked. The audio up to then is saved.")
                .font(.subheadline)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

struct ResumeButton: View {
    var body: some View {
        Button { } label: {
            Label("Resume recording", systemImage: "record.circle")
                .font(.subheadline.weight(.semibold))
        }
        .buttonStyle(.borderedProminent)
        .tint(.red)
    }
}

/// A small picture of a page's first lines, standing in for a thumbnail of real ink.
struct PageThumb: View {
    var lines: [String]
    var width: CGFloat = 96
    var body: some View {
        VStack(alignment: .leading, spacing: width * 0.04) {
            ForEach(lines, id: \.self) { line in
                Text(line)
                    .font(inkFont(width * 0.13))
                    .lineLimit(1)
                    .fixedSize()
            }
            Spacer(minLength: 0)
        }
        .padding(width * 0.08)
        .frame(width: width, height: width * 0.72, alignment: .topLeading)
        .clipped()
        .background(Color(.systemBackground))
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.3)))
    }
}

func meetingSubtitle(_ m: PastMeeting) -> String { "\(m.time) · \(m.minutes) min" }

func lines(through seconds: Int) -> [NoteLine] {
    Mock.lines.filter { ($0.seconds ?? 0) <= seconds }
}
