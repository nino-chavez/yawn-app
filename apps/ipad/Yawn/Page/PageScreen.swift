import InkCore
import SwiftUI

/// What the app opens to: a page, with Record in the bar above it. While
/// recording, the bar holds the recording state and Stop, and nothing else.
/// Writing on the page arrives in slice 3.
struct PageScreen: View {
    @ObservedObject var recorder: MeetingRecorder
    @State private var showingBefore = false
    @State private var showingTranscript = false

    var body: some View {
        PageSurface(showHint: recorder.meetings.isEmpty && recorder.phase == .idle)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar(removing: recorder.isRecording ? .sidebarToggle : nil)
            .toolbar {
                ToolbarItem(placement: .principal) { barCenter }
                if recorder.phase == .transcriptReady {
                    ToolbarItem(placement: .primaryAction) {
                        Button("Transcript") { showingTranscript = true }
                    }
                }
            }
            .safeAreaInset(edge: .top, spacing: 0) { statusLine }
            .fullScreenCover(isPresented: $showingBefore) {
                BeforeRecording(
                    speechModel: recorder.speechModel,
                    canStart: recorder.canStart,
                    onInstallSpeechModel: recorder.installSpeechModel,
                    onStart: { keepDays in
                        showingBefore = false
                        Task { await recorder.start(keepDays: keepDays) }
                    },
                    onCancel: { showingBefore = false }
                )
            }
            .sheet(isPresented: $showingTranscript) {
                TranscriptSheet(segments: recorder.transcript)
            }
    }

    @ViewBuilder
    private var barCenter: some View {
        switch recorder.phase {
        case .preparing:
            BarStatus(text: "Preparing")
        case .recording:
            if let startedAt = recorder.startedAt {
                RecordingBadge(startedAt: startedAt, onStop: recorder.stop)
            }
        case .finishing:
            BarStatus(text: "Finishing")
        case .idle, .transcriptReady, .needsSpeechModel, .transcriptionFailed, .failed:
            RecordButton { showingBefore = true }
        }
    }

    /// One status near the bar. A full-width warning is only for a recording
    /// that stopped early.
    @ViewBuilder
    private var statusLine: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let note = recorder.interruptionNote, !recorder.isRecording {
                Label(note, systemImage: "exclamationmark.triangle.fill")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.orange)
            }
            switch recorder.phase {
            case .transcriptReady:
                Text("Transcript ready · \(MeetingRecorder.clock(recorder.lastDuration ?? 0))")
                    .foregroundStyle(.secondary)
            case .needsSpeechModel:
                HStack {
                    Text("Needs attention: the meeting is saved, but it has no transcript yet. Install Apple’s on-device speech model to transcribe it.")
                    if recorder.speechModel == .installable {
                        Button("Install", action: recorder.installSpeechModel)
                    }
                }
            case .transcriptionFailed:
                HStack {
                    Text("Needs attention: the meeting is saved, but its transcript could not be made.")
                    Button("Transcribe again") { Task { await recorder.transcribeAgain() } }
                }
            case .failed(let message):
                Text(message).foregroundStyle(.red)
            case .idle, .preparing, .recording, .finishing:
                EmptyView()
            }
        }
        .font(.subheadline)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, TimeRail.inset + 16)
        .padding(.vertical, hasStatus ? 8 : 0)
    }

    private var hasStatus: Bool {
        switch recorder.phase {
        case .transcriptReady, .needsSpeechModel, .transcriptionFailed, .failed: true
        case .idle, .preparing, .recording, .finishing: recorder.interruptionNote != nil && !recorder.isRecording
        }
    }
}

/// The page: the time rail down its left margin and, on first run only, one
/// line of what to do.
struct PageSurface: View {
    let showHint: Bool

    var body: some View {
        ZStack(alignment: .leading) {
            Color(.systemBackground)
            TimeRail()
            if showHint {
                VStack(spacing: 10) {
                    // One line where it fits; otherwise two even lines.
                    ViewThatFits(in: .horizontal) {
                        Text("Record a meeting, and write on this page while it records.")
                            .fixedSize()
                        Text("Record a meeting, and write\non this page while it records.")
                    }
                    .font(.title3)
                    Text("Write with a finger. Scroll with two fingers.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                .multilineTextAlignment(.center)
                .padding(32)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }
}

/// The rail down the page's left margin (Concept C). Marks arrive in slice 3.
struct TimeRail: View {
    static let inset: CGFloat = 44

    var body: some View {
        Rectangle()
            .fill(Color(.separator))
            .frame(width: 2)
            .padding(.vertical, 24)
            .padding(.leading, Self.inset)
            .accessibilityHidden(true)
    }
}
