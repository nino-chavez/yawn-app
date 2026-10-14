import SwiftUI

/// B · Pad. The app opens to a blank page. Record is one button at the top that
/// opens into the consent card; past meetings are a grid of pages behind a
/// Meetings button; after Stop the transcript is a ribbon under the page.
struct ConceptB: View {
    let state: ConceptState
    @State private var agreed = false
    @State private var keep: KeepPeriod?

    var body: some View {
        VStack(spacing: 0) {
            topBar
            Divider()
            content
        }
        .background(Color(.systemBackground))
    }

    // MARK: Top bar

    private var topBar: some View {
        ZStack {
            HStack(spacing: 16) {
                meetingsButton
                Spacer()
                trailing
            }
            center
        }
        .padding(.horizontal, 18)
        .frame(height: 60)
        .background(.bar)
    }

    private var meetingsButton: some View {
        Button { } label: {
            HStack(spacing: 6) {
                Image(systemName: state == .list ? "chevron.backward" : "square.grid.2x2")
                Text(state == .list ? "Page" : "Meetings")
                if state != .firstRun && state != .list {
                    Text(state == .before ? "3" : "4")
                        .font(.caption2.weight(.bold))
                        .padding(.horizontal, 6).padding(.vertical, 1)
                        .background(Capsule().fill(Color(.tertiarySystemFill)))
                }
            }
            .font(.subheadline.weight(.medium))
        }
        .opacity(state == .during ? 0.35 : 1)
        .disabled(state == .during)
    }

    @ViewBuilder
    private var center: some View {
        switch state {
        case .before, .firstRun:
            Button { } label: {
                Label("Record", systemImage: "record.circle").font(.headline)
                    .padding(.horizontal, 18).padding(.vertical, 8)
                    .background(Capsule().fill(Color.red.opacity(state == .before ? 0.18 : 0.12)))
                    .foregroundStyle(.red)
            }
        case .during:
            HStack(spacing: 14) {
                RecordingBadge(seconds: Mock.duringSeconds)
                StopButton().controlSize(.small)
            }
            .padding(.leading, 16).padding(.trailing, 6).padding(.vertical, 6)
            .background(Capsule().fill(Color.red.opacity(0.1)))
        case .after:
            VStack(spacing: 1) {
                Text(Mock.meetingTitle).font(.headline)
                Text("23 min · Transcript ready").font(.caption).foregroundStyle(.secondary)
            }
        case .attention:
            HStack(spacing: 12) {
                Label("Recording stopped at \(Mock.clock(Mock.interruptedSeconds))", systemImage: "exclamationmark.triangle.fill")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.orange)
                ResumeButton().controlSize(.small)
            }
            .padding(.leading, 16).padding(.trailing, 6).padding(.vertical, 6)
            .background(Capsule().fill(Color.orange.opacity(0.14)))
        case .list:
            Text("Meetings").font(.headline)
        }
    }

    @ViewBuilder
    private var trailing: some View {
        switch state {
        case .during, .attention:
            ToolCluster()
        case .after:
            HStack(spacing: 18) {
                ToolCluster()
                Button { } label: { Image(systemName: "square.and.arrow.up") }.font(.title3).accessibilityLabel("Export to Files")
            }
        case .list:
            Button { } label: { Label("New meeting", systemImage: "record.circle") }
                .buttonStyle(.borderedProminent).tint(.red)
        case .before, .firstRun:
            EmptyView()
        }
    }

    // MARK: Content

    @ViewBuilder
    private var content: some View {
        switch state {
        case .before:
            ZStack(alignment: .top) {
                InkPage(layer: InkLayer(lines: []), contentHeight: 0)
                consentCard.padding(.top, 14)
            }
        case .during:
            page(lines(through: Mock.duringSeconds), offset: -1)
        case .after:
            VStack(spacing: 0) {
                page(Mock.lines, offset: 380, dimAfter: Mock.pausedSeconds, selected: Mock.tappedLine, afterStop: true)
                transcriptRibbon
            }
        case .attention:
            VStack(spacing: 0) {
                Text("The audio up to then is saved. Resume to keep recording this meeting.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .padding(.vertical, 8)
                page(lines(through: Mock.interruptedSeconds), offset: 0)
            }
        case .firstRun:
            ZStack {
                InkPage(layer: InkLayer(lines: []), contentHeight: 0)
                VStack(spacing: 10) {
                    Text("Record a meeting, and write on this page while it records.")
                        .font(.title3)
                    Text("Write with a finger. Scroll with two fingers. Meetings stay on this iPad.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                .multilineTextAlignment(.center)
                .allowsHitTesting(false)
            }
        case .list:
            meetingGrid
        }
    }

    private func page(_ lines: [NoteLine], offset: CGFloat, dimAfter: Int? = nil, selected: Int? = nil, afterStop: Bool = false) -> some View {
        InkPage(
            layer: InkLayer(lines: lines, dimAfter: dimAfter, selectedID: selected, afterStop: afterStop ? Mock.addedAfterStop : nil, afterStopStyle: .marginMark),
            contentHeight: InkLayer.height(lineCount: lines.count, hasAfterStop: afterStop),
            initialOffset: offset
        )
    }

    private var consentCard: some View {
        VStack(alignment: .leading, spacing: 20) {
            ConsentFields(agreed: $agreed, keep: $keep)
            HStack {
                Button("Cancel") { }
                Spacer()
                RecordButton(title: "Start recording", enabled: agreed && keep != nil)
            }
        }
        .padding(24)
        .frame(maxWidth: 520)
        .background(RoundedRectangle(cornerRadius: 20).fill(Color(.secondarySystemBackground)))
        .shadow(color: .black.opacity(0.12), radius: 18, y: 6)
    }

    /// The transcript as a ribbon of turns under the page, following the playhead.
    private var transcriptRibbon: some View {
        let current = Mock.transcript.last { $0.seconds <= Mock.pausedSeconds }?.id
        return VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 16) {
                ReplayControls()
                Spacer()
                Label("Search", systemImage: "magnifyingglass").font(.subheadline).foregroundStyle(.secondary)
                Label("Full transcript", systemImage: "arrow.up.left.and.arrow.down.right").font(.subheadline)
            }
            ScrollViewReader { proxy in
                ScrollView(.horizontal, showsIndicators: false) {
                    LazyHStack(alignment: .top, spacing: 10) {
                        ForEach(Mock.transcript) { turn in
                            VStack(alignment: .leading, spacing: 4) {
                                Text(Mock.clock(turn.seconds)).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                                Text(turn.text).font(.subheadline)
                                    .foregroundStyle(turn.seconds > Mock.pausedSeconds ? .secondary : .primary)
                                    .lineLimit(4)
                                Spacer(minLength: 0)
                            }
                            .padding(10)
                            .frame(width: 210, height: 116, alignment: .topLeading)
                            .background(RoundedRectangle(cornerRadius: 12).fill(turn.id == current ? Color.accentColor.opacity(0.14) : Color(.secondarySystemBackground)))
                            .id(turn.id)
                        }
                    }
                }
                .onAppear { if let current { proxy.scrollTo(current, anchor: .center) } }
            }
            .frame(height: 116)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }

    private var meetingGrid: some View {
        let groups: [(String, [PastMeeting])] = [
            ("Today", [Mock.today]),
            ("This week", Mock.pastMeetings.filter { $0.group == "This week" }),
            ("Last week", Mock.pastMeetings.filter { $0.group == "Last week" }),
        ]
        return ScrollView {
            VStack(alignment: .leading, spacing: 26) {
                ForEach(groups, id: \.0) { title, meetings in
                    VStack(alignment: .leading, spacing: 12) {
                        Text(title).font(.title3.weight(.semibold))
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 220, maximum: 260), spacing: 22, alignment: .top)], alignment: .leading, spacing: 22) {
                            ForEach(meetings) { m in
                                VStack(alignment: .leading, spacing: 8) {
                                    PageThumb(lines: m.firstLines, width: 230)
                                        .shadow(color: .black.opacity(0.08), radius: 6, y: 2)
                                    Text(m.day).font(.headline)
                                    Text(meetingSubtitle(m)).font(.subheadline).foregroundStyle(.secondary)
                                }
                            }
                        }
                    }
                }
            }
            .padding(28)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Color(.secondarySystemBackground))
    }
}
