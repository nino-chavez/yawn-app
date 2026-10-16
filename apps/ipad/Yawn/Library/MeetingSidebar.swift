import SwiftUI

/// The meeting list, on Sidebar. Rows (`MeetingRow`) and search arrive with
/// the storage in slice 5 of docs/ipad-build-plan-2026-10-15.md; until then
/// the list says what will appear here.
struct MeetingSidebar: View {
    var body: some View {
        ContentUnavailableView {
            Label("No meetings yet", systemImage: "list.bullet.rectangle")
                .foregroundStyle(Paper.label)
        } description: {
            Text("Each meeting you record appears here, newest first.")
                .foregroundStyle(Paper.label3)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Paper.sidebar)
        .navigationTitle("Meetings")
        .toolbarBackground(Paper.windowChrome, for: .navigationBar)
        .toolbarBackgroundVisibility(.visible, for: .navigationBar)
    }
}
