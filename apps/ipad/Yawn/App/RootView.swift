import SwiftUI

/// The page is the screen. Past meetings live in a sidebar beside it
/// (concept selection, change 2), collapsed while there are none to show.
/// Graphite is the chrome's tint: the sidebar toggle and the back chevron
/// are chrome, not brand.
struct RootView: View {
    @State private var columns: NavigationSplitViewVisibility = .detailOnly

    var body: some View {
        NavigationSplitView(columnVisibility: $columns) {
            MeetingSidebar()
                .navigationSplitViewColumnWidth(PaperMetrics.sidebarWidth)
        } detail: {
            PageScreen()
        }
        .tint(Paper.graphite)
    }
}
