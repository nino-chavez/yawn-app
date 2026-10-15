import SwiftUI

/// The page is the screen. Past meetings live in a sidebar beside it
/// (concept selection, change 2), hidden while recording.
struct RootView: View {
    @StateObject private var recorder = MeetingRecorder()
    @State private var columns: NavigationSplitViewVisibility = .detailOnly
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            MeetingSidebar(meetings: recorder.meetings)
        } detail: {
            PageScreen(recorder: recorder)
        }
        .onChange(of: recorder.isRecording) { _, recording in
            if recording { columns = .detailOnly }
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { recorder.sweepExpiredAudio() }
        }
    }
}
