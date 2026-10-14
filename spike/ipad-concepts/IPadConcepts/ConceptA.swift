import SwiftUI

/// A · Notebook. The meeting list is a sidebar beside the page, consent is a sheet,
/// the transcript is an inspector column, and replay lives in the toolbar.
struct ConceptA: View {
    let state: ConceptState
    @State private var columns: NavigationSplitViewVisibility
    @State private var showConsent: Bool
    @State private var showTranscript: Bool

    init(state: ConceptState) {
        self.state = state
        _columns = State(initialValue: (state == .during || state == .after) ? .detailOnly : .all)
        _showConsent = State(initialValue: state == .before)
        _showTranscript = State(initialValue: state == .after)
    }

    private var meetings: [PastMeeting] {
        switch state {
        case .firstRun: []
        case .before: Mock.pastMeetings
        default: [Mock.today] + Mock.pastMeetings
        }
    }

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            sidebar
        } detail: {
            NavigationStack { detail }
                .inspector(isPresented: $showTranscript) { transcriptInspector }
        }
        .sheet(isPresented: $showConsent) { consentSheet }
    }

    // MARK: Sidebar

    @ViewBuilder
    private var sidebar: some View {
        Group {
            if meetings.isEmpty {
                ContentUnavailableView("No meetings yet", systemImage: "pencil.and.scribble", description: Text("Meetings you record appear here."))
            } else {
                List {
                    ForEach(["Today", "This week", "Last week"], id: \.self) { group in
                        let rows = meetings.filter { $0.group == group }
                        if !rows.isEmpty {
                            Section(group) {
                                ForEach(rows) { meeting in
                                    row(meeting)
                                        .listRowBackground(meeting.id == 0 && state != .before ? Color.accentColor.opacity(0.15) : nil)
                                }
                            }
                        }
                    }
                }
            }
        }
        .navigationTitle("Meetings")
    }

    private func row(_ m: PastMeeting) -> some View {
        HStack(spacing: 12) {
            PageThumb(lines: m.firstLines, width: 76)
            VStack(alignment: .leading, spacing: 2) {
                Text(m.day).font(.headline)
                Text(meetingSubtitle(m)).font(.subheadline).foregroundStyle(.secondary)
                if m.id == 0 && state == .attention {
                    Text("Needs attention").font(.caption.weight(.semibold)).foregroundStyle(.orange)
                }
            }
        }
        .padding(.vertical, 4)
    }

    // MARK: Detail

    @ViewBuilder
    private var detail: some View {
        switch state {
        case .before:
            blankPage.navigationTitle("New meeting").navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .primaryAction) { RecordButton() } }
        case .during:
            page(lines: lines(through: Mock.duringSeconds), offset: -1)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .principal) { RecordingBadge(seconds: Mock.duringSeconds) }
                    ToolbarItem(placement: .primaryAction) { ToolCluster() }
                    ToolbarItem(placement: .primaryAction) { StopButton() }
                }
        case .after:
            page(lines: Mock.lines, offset: 380, dimAfter: Mock.pausedSeconds, selected: Mock.tappedLine, afterStop: true)
                .navigationTitle(Mock.meetingTitle).navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .principal) { ReplayControls(compact: true) }
                    ToolbarItem(placement: .primaryAction) { ToolCluster() }
                    ToolbarItem(placement: .primaryAction) {
                        Button { showTranscript.toggle() } label: { Image(systemName: "text.quote") }
                            .accessibilityLabel("Transcript")
                    }
                    ToolbarItem(placement: .primaryAction) {
                        Button { } label: { Image(systemName: "square.and.arrow.up") }.accessibilityLabel("Export to Files")
                    }
                }
        case .attention:
            page(lines: lines(through: Mock.interruptedSeconds), offset: 0)
                .safeAreaInset(edge: .top, spacing: 0) {
                    HStack(alignment: .center, spacing: 16) {
                        InterruptionMessage()
                        Spacer()
                        ResumeButton()
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 12)
                    .background(Color.orange.opacity(0.12))
                }
                .navigationTitle(Mock.meetingTitle).navigationBarTitleDisplayMode(.inline)
        case .firstRun:
            VStack(spacing: 18) {
                Image(systemName: "pencil.and.scribble").font(.system(size: 44)).foregroundStyle(.secondary)
                Text("Record a meeting, and write on the page while it records.")
                    .font(.title3).multilineTextAlignment(.center)
                Text("Write with a finger. Scroll with two fingers.")
                    .font(.subheadline).foregroundStyle(.secondary)
                RecordButton()
            }
            .padding(40)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(.systemBackground))
        case .list:
            page(lines: Mock.lines, offset: 0, afterStop: true)
                .navigationTitle(Mock.meetingTitle).navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .status) {
                        Text("Transcript ready").font(.subheadline).foregroundStyle(.secondary)
                    }
                    ToolbarItem(placement: .primaryAction) {
                        Button { showTranscript.toggle() } label: { Image(systemName: "text.quote") }.accessibilityLabel("Transcript")
                    }
                    ToolbarItem(placement: .primaryAction) { RecordButton() }
                }
        }
    }

    private var blankPage: some View {
        InkPage(layer: InkLayer(lines: []), contentHeight: 0)
    }

    private func page(lines: [NoteLine], offset: CGFloat, dimAfter: Int? = nil, selected: Int? = nil, afterStop: Bool = false) -> some View {
        InkPage(
            layer: InkLayer(lines: lines, dimAfter: dimAfter, selectedID: selected, afterStop: afterStop ? Mock.addedAfterStop : nil, afterStopStyle: .blueInk),
            contentHeight: InkLayer.height(lineCount: lines.count, hasAfterStop: afterStop),
            initialOffset: offset
        )
    }

    private var transcriptInspector: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Transcript").font(.headline)
                Spacer()
                Text("Transcript ready").font(.caption).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 16)
            .padding(.top, 12)
            Label("Search the transcript", systemImage: "magnifyingglass")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .padding(8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(.tertiarySystemFill), in: RoundedRectangle(cornerRadius: 8))
                .padding(.horizontal, 12)
            TranscriptList(playhead: state == .after ? Mock.pausedSeconds : nil)
        }
        .inspectorColumnWidth(min: 300, ideal: 340, max: 420)
    }

    private var consentSheet: some View {
        NavigationStack {
            ConsentFields()
                .padding(28)
                .frame(maxHeight: .infinity, alignment: .top)
                .navigationTitle("Before you record")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { showConsent = false } }
                    ToolbarItem(placement: .confirmationAction) { RecordButton() }
                }
        }
        .presentationDetents([.medium])
    }
}
