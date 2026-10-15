import SwiftUI

/// What the app opens to: a page, with Record in the bar above it. In this
/// build the page is blank; writing arrives in slice 3.
struct PageScreen: View {
    var body: some View {
        FirstRunPage()
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    // Recording arrives in slice 2; RecordButton replaces this.
                    RecordUnavailable()
                }
            }
    }
}

/// First run: the page, with the time rail down its left margin, and one line
/// of what to do.
struct FirstRunPage: View {
    var body: some View {
        ZStack(alignment: .leading) {
            Color(.systemBackground)
            TimeRail()
            VStack(spacing: 10) {
                // One line where it fits; otherwise two even lines, never one
                // stray word.
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

/// The rail down the page's left margin (Concept C). Empty until notes are
/// written against a recording; marks arrive in slice 3.
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
