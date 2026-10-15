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

/// Record before recording exists. A disabled toolbar button fades almost out of
/// sight on dark bars, so this is a plain label that says why it does nothing.
struct RecordUnavailable: View {
    var body: some View {
        VStack(spacing: 2) {
            Label("Record", systemImage: "record.circle")
                .font(.headline)
                .labelStyle(.titleAndIcon)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 14)
                .padding(.vertical, 5)
                .background(Capsule().fill(Color(.tertiarySystemFill)))
            Text("Not available in this build yet")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}
