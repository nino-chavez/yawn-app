import SwiftUI

/// What the app opens to: a page, with Record in the bar above it. In this
/// build the page is blank; writing arrives in slice 3.
struct PageScreen: View {
    var body: some View {
        FirstRunPage()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    // Recording arrives in slice 2.
                    RecordButton(action: {})
                        .disabled(true)
                }
            }
    }
}

/// First run: a blank page and one line of what to do, nothing else.
struct FirstRunPage: View {
    var body: some View {
        VStack(spacing: 10) {
            Text("Record a meeting, and write on this page while it records.")
                .font(.title3)
            Text("Write with a finger. Scroll with two fingers.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Text("Recording is not in this build yet.")
                .font(.footnote)
                .foregroundStyle(.tertiary)
                .padding(.top, 12)
        }
        .multilineTextAlignment(.center)
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemBackground))
    }
}
