import SwiftUI

/// C · Time rail. Meetings by day are the home screen. Consent is a step of its
/// own. On the page, a rail down the left edge marks when each line was written
/// and is the scrubber; after Stop each transcript turn sits level with the ink
/// written at that moment.
struct ConceptC: View {
    let state: ConceptState
    @State private var agreed = false
    @State private var keep: KeepPeriod?
    @State private var showFullTranscript = false

    var body: some View {
        switch state {
        case .before: consentStep
        case .firstRun: meetingList(empty: true)
        case .list: meetingList(empty: false)
        case .during, .after, .attention: meetingView
        }
    }

    // MARK: Meeting list, the home screen

    private func meetingList(empty: Bool) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline) {
                Text("Meetings").font(.largeTitle.weight(.bold))
                Spacer()
                Button { } label: { Label("New meeting", systemImage: "record.circle").font(.headline) }
                    .buttonStyle(.borderedProminent).tint(.red).controlSize(.large)
            }
            .padding(.horizontal, 32)
            .padding(.top, 24)
            .padding(.bottom, 12)
            if empty {
                VStack(spacing: 12) {
                    Image(systemName: "timeline.selection").font(.system(size: 44)).foregroundStyle(.secondary)
                    Text("No meetings yet").font(.title2.weight(.semibold))
                    Text("Start a meeting, and write on the page while it records. Everything stays on this iPad.")
                        .font(.body).foregroundStyle(.secondary).multilineTextAlignment(.center)
                    Text("Write with a finger. Scroll with two fingers.")
                        .font(.subheadline).foregroundStyle(.secondary)
                }
                .frame(maxWidth: 460)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 22) {
                        ForEach([("Today", [Mock.today]), ("This week", Mock.pastMeetings.filter { $0.group == "This week" }), ("Last week", Mock.pastMeetings.filter { $0.group == "Last week" })], id: \.0) { title, rows in
                            VStack(alignment: .leading, spacing: 0) {
                                Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(.secondary).padding(.bottom, 8)
                                ForEach(rows) { m in
                                    listRow(m)
                                    Divider()
                                }
                            }
                        }
                    }
                    .padding(.horizontal, 32)
                    .padding(.bottom, 32)
                }
            }
        }
        .background(Color(.systemBackground))
    }

    private func listRow(_ m: PastMeeting) -> some View {
        HStack(spacing: 20) {
            VStack(alignment: .leading, spacing: 3) {
                Text(m.day).font(.headline)
                Text(meetingSubtitle(m)).font(.subheadline).foregroundStyle(.secondary)
            }
            .frame(width: 150, alignment: .leading)
            MiniRail(minutes: m.minutes, seed: m.id)
                .frame(height: 30)
            Text(m.firstLines.joined(separator: "  ·  "))
                .font(inkFont(22))
                .lineLimit(1)
                .frame(width: 280, alignment: .leading)
        }
        .padding(.vertical, 14)
    }

    // MARK: Consent, a step of its own

    private var consentStep: some View {
        VStack(alignment: .leading, spacing: 26) {
            Text("Before you record").font(.largeTitle.weight(.bold))
            Text("Say out loud that you're recording, and check that everyone in the room is okay with it.")
                .font(.body).foregroundStyle(.secondary)
            ConsentFields(agreed: $agreed, keep: $keep)
            RecordButton(title: "Start recording", large: true, enabled: agreed && keep != nil)
            Button("Not now") { }.frame(maxWidth: .infinity)
        }
        .padding(36)
        .frame(maxWidth: 560)
        .background(RoundedRectangle(cornerRadius: 24).fill(Color(.systemBackground)))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.secondarySystemBackground))
    }

    // MARK: The meeting

    private var meetingView: some View {
        VStack(spacing: 0) {
            topBar
            Divider()
            GeometryReader { geo in
                let column: CGFloat = state == .after ? min(360, geo.size.width * 0.36) : 0
                HStack(spacing: 0) {
                    InkPage(layer: layer(column: column), contentHeight: height, initialOffset: offset)
                    if state == .during {
                        Divider()
                        ToolCluster(vertical: true)
                            .padding(.top, 24)
                            .frame(width: 64)
                            .frame(maxHeight: .infinity, alignment: .top)
                            .background(.bar)
                    }
                }
            }
        }
        .background(Color(.systemBackground))
        .sheet(isPresented: $showFullTranscript) {
            NavigationStack {
                TranscriptList(playhead: Mock.pausedSeconds)
                    .navigationTitle("Transcript")
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { showFullTranscript = false } } }
            }
        }
    }

    private var topBar: some View {
        ZStack {
            HStack(spacing: 18) {
                if state != .during {
                    Button { } label: { Label("Meetings", systemImage: "chevron.backward").font(.subheadline.weight(.medium)) }
                }
                if state == .after {
                    HStack(spacing: 8) {
                        Button { } label: { Image(systemName: "play.fill") }.accessibilityLabel("Play")
                        Text("\(Mock.clock(Mock.pausedSeconds)) / \(Mock.clock(Mock.meetingLength))")
                            .font(.subheadline.monospacedDigit())
                    }
                    .padding(.horizontal, 12).padding(.vertical, 6)
                    .background(Capsule().fill(Color(.tertiarySystemFill)))
                }
                Spacer()
                switch state {
                case .during: StopButton()
                case .after:
                    ToolCluster()
                    Button { showFullTranscript = true } label: { Image(systemName: "text.quote") }.font(.title3).accessibilityLabel("Full transcript")
                    Button { } label: { Image(systemName: "magnifyingglass") }.font(.title3).accessibilityLabel("Search the transcript")
                    Button { } label: { Image(systemName: "square.and.arrow.up") }.font(.title3).accessibilityLabel("Export to Files")
                default:
                    ToolCluster()
                }
            }
            switch state {
            case .during: RecordingBadge(seconds: Mock.duringSeconds)
            case .after:
                VStack(spacing: 1) {
                    Text(Mock.meetingTitle).font(.headline)
                    Text("23 min · Transcript ready").font(.caption).foregroundStyle(.secondary)
                }
            default:
                VStack(spacing: 1) {
                    Text(Mock.meetingTitle).font(.headline)
                    Text("Needs attention").font(.caption.weight(.semibold)).foregroundStyle(.orange)
                }
            }
        }
        .padding(.horizontal, 18)
        .frame(height: 60)
        .background(.bar)
    }

    private var shownLines: [NoteLine] {
        switch state {
        case .during: lines(through: Mock.duringSeconds)
        case .attention: lines(through: Mock.interruptedSeconds)
        default: Mock.lines
        }
    }

    private var height: CGFloat {
        let base = InkLayer.height(lineCount: shownLines.count, hasAfterStop: state == .after)
        return state == .attention ? base + 160 : base
    }

    private var offset: CGFloat {
        switch state {
        case .during: -1
        case .after: 380
        default: 0
        }
    }

    private func layer(column: CGFloat) -> InkLayer {
        let rail: RailSpec = switch state {
        case .during: RailSpec(endSeconds: Mock.duringSeconds, live: true)
        case .attention: RailSpec(endSeconds: Mock.interruptedSeconds, interruptedAt: Mock.interruptedSeconds)
        default: RailSpec(endSeconds: Mock.meetingLength, playhead: Mock.pausedSeconds)
        }
        return InkLayer(
            lines: shownLines,
            dimAfter: state == .after ? Mock.pausedSeconds : nil,
            selectedID: state == .after ? Mock.tappedLine : nil,
            afterStop: state == .after ? Mock.addedAfterStop : nil,
            afterStopStyle: .belowRail,
            leadingInset: 96,
            rail: rail,
            alignedTranscript: state == .after ? Mock.transcript : nil,
            transcriptColumn: column
        )
    }
}

/// A meeting's length as a line, with a mark wherever notes were written.
struct MiniRail: View {
    var minutes: Int
    var seed: Int
    var body: some View {
        GeometryReader { geo in
            let marks = (0..<7).map { i in CGFloat((i * 37 + seed * 13) % 100) / 100 }
            ZStack(alignment: .leading) {
                Capsule().fill(Color.secondary.opacity(0.25)).frame(height: 4)
                ForEach(Array(marks.enumerated()), id: \.offset) { _, f in
                    Circle().fill(Color.primary).frame(width: 8, height: 8)
                        .offset(x: f * (geo.size.width - 8))
                }
            }
            .frame(maxHeight: .infinity)
        }
        .accessibilityLabel("\(minutes) minutes, with notes along the way")
    }
}
