import SwiftUI

/// The page is the screen. Past meetings live in a sidebar beside it
/// (concept selection, change 2), collapsed while there are none to show.
struct RootView: View {
    @State private var columns: NavigationSplitViewVisibility = .detailOnly

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            MeetingSidebar()
        } detail: {
            PageScreen()
        }
    }
}
