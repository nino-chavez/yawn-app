import SwiftUI

/// The meeting list. Rows (date, length, a small rail, the top of the page)
/// and search arrive in slice 5 of docs/ipad-build-plan-2026-10-15.md.
struct MeetingSidebar: View {
    var body: some View {
        ContentUnavailableView(
            "No meetings yet",
            systemImage: "list.bullet.rectangle",
            description: Text("Each meeting you record appears here, newest first.")
        )
        .navigationTitle("Meetings")
    }
}
