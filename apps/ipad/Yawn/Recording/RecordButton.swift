import SwiftUI

/// The one Record control. It and the recording state are the only things that
/// use the recording color (experience brief, Hierarchy).
struct RecordButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label("Record", systemImage: "record.circle")
                .font(.headline)
                // Toolbars show icons only by default; Record keeps its word.
                .labelStyle(.titleAndIcon)
        }
        .buttonStyle(.bordered)
        .buttonBorderShape(.capsule)
        .tint(.red)
    }
}
