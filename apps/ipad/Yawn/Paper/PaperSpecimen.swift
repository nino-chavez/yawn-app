#if DEBUG
import SwiftUI

/// Engineers only, debug builds only: the prepared Rail components on one
/// page, reached with the `-paperSpecimen` launch argument, so a cold review
/// can see them in the installed build before the screens that use them
/// exist. Not a route in the app.
struct PaperSpecimen: View {
    @State private var tool: ToolColumn.Tool = .pen

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 36) {
                    SpecimenSection("Recording badge and Stop") {
                        HStack(spacing: 16) {
                            RecordingBadge(elapsed: "12:04")
                            RecordingBadge(elapsed: "12:04", paused: true)
                            StopCapsule {}
                        }
                    }
                    SpecimenSection("Record") {
                        HStack(alignment: .top, spacing: 16) {
                            RecordButton {}
                            RecordUnavailable()
                        }
                    }
                    SpecimenSection("Tool column") {
                        ToolColumn(tool: $tool, undo: {})
                    }
                    SpecimenSection("Meeting rows") {
                        VStack(spacing: 2) {
                            MeetingRow(
                                day: "Wednesday",
                                detail: "2:10 PM · 23:05",
                                marks: [0.03, 0.09, 0.24, 0.39, 0.55, 0.75, 0.92],
                                firstLine: "Checklist review opens; Them owns the rendered comparison.",
                                selected: true
                            )
                            MeetingRow(
                                day: "Tuesday",
                                detail: "9:30 AM · 41:12",
                                marks: [0.1, 0.2, 0.6],
                                firstLine: "Pricing follow-up",
                                selected: false
                            )
                        }
                        .padding(PaperMetrics.sidebarInset)
                        .frame(width: PaperMetrics.sidebarWidth)
                        .background(Paper.sidebar, in: .rect(cornerRadius: PaperMetrics.sidebarRadius))
                    }
                    SpecimenSection("Consent") {
                        ConsentCard(start: { _ in }, notNow: {})
                            .frame(height: 560)
                            .clipShape(.rect(cornerRadius: 12))
                    }
                }
                .padding(32)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .background(Paper.page)
            .navigationTitle("Paper specimen")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(Paper.windowChrome, for: .navigationBar)
            .toolbarBackgroundVisibility(.visible, for: .navigationBar)
        }
        .tint(Paper.graphite)
    }
}

/// One labeled group on the specimen page.
struct SpecimenSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Paper.label3)
            content
        }
    }
}
#endif
