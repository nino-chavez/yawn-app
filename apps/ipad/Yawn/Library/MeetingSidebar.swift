import SwiftUI

/// The meeting list, on Sidebar. Rows (`MeetingRow`) and search arrive with
/// the storage in slice 5 of docs/ipad-build-plan-2026-10-15.md; until then
/// the list says what will appear here, lightly: a title and one line, no
/// symbol, so the empty list does not outrank the page beside it. The bar
/// keeps the system's floating-sidebar treatment; forcing a background on it
/// painted a band over the panel's top corners.
struct MeetingSidebar: View {
    var body: some View {
        ContentUnavailableView {
            Text("No meetings yet")
                .foregroundStyle(Paper.label)
        } description: {
            Text("Each meeting you record appears here, newest first.")
                .foregroundStyle(Paper.label3)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Paper.sidebar)
        .navigationTitle("Meetings")
    }
}
