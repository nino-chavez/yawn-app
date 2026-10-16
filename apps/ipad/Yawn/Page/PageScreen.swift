import SwiftUI

/// What the app opens to: a page, with Record in the bar above it. In this
/// build the page is blank; writing arrives in slice 3. The nav bar sits on
/// Window chrome with a hairline below it (DESIGN.md, Composition, iPad); the
/// hairline is drawn here, at the top of the page's safe area, because
/// SwiftUI exposes no separator of the bar's own.
struct PageScreen: View {
    var body: some View {
        FirstRunPage()
            .safeAreaInset(edge: .top, spacing: 0) {
                Paper.hairline.frame(height: PaperMetrics.hairline)
            }
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    // Recording arrives in slice 2; RecordButton replaces this.
                    RecordUnavailable()
                }
            }
            .toolbarBackground(Paper.windowChrome, for: .navigationBar)
            .toolbarBackgroundVisibility(.visible, for: .navigationBar)
    }
}

/// First run: the page, with the time rail down its left margin, and one line
/// of what to do.
struct FirstRunPage: View {
    var body: some View {
        ZStack(alignment: .leading) {
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
                .foregroundStyle(Paper.label)
                Text("Write with a finger. Scroll with two fingers.")
                    .font(.subheadline)
                    .foregroundStyle(Paper.label3)
            }
            .multilineTextAlignment(.center)
            .padding(32)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(Paper.page)
    }
}

/// The rail down the page's left margin (Concept C): a 2px Rail line at 64pt
/// from the page's left edge. Empty until notes are written against a
/// recording; marks arrive in slice 3.
struct TimeRail: View {
    var body: some View {
        Rectangle()
            .fill(Paper.rail)
            .frame(width: PaperMetrics.railWidth)
            .padding(.vertical, 24)
            .padding(.leading, PaperMetrics.railInset)
            .accessibilityHidden(true)
    }
}
