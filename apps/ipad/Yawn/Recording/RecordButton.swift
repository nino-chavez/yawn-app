import SwiftUI

/// The one Record control. It and the recording state are the only things that
/// use the recording color (experience brief, Hierarchy): a plain paper
/// capsule whose word and symbol are Record.
struct RecordButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label("Record", systemImage: "record.circle")
                // Toolbars show icons only by default; Record keeps its word.
                .labelStyle(.titleAndIcon)
        }
        .buttonStyle(PaperCapsuleStyle(kind: .plain, labelColor: Paper.record))
    }
}

/// Record before recording exists. A disabled toolbar button fades almost out of
/// sight on dark bars, so this is the same capsule in Label 2 with one line
/// that says why it does nothing.
struct RecordUnavailable: View {
    var body: some View {
        VStack(spacing: 4) {
            Label("Record", systemImage: "record.circle")
                .labelStyle(.titleAndIcon)
                .paperCapsule(.plain, labelColor: Paper.label2)
            Text("Not available in this build yet")
                .font(.caption)
                .foregroundStyle(Paper.label3)
        }
        .accessibilityElement(children: .combine)
    }
}
