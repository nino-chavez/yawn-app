import SwiftUI

/// Stop: the Record capsule at the right of the nav bar while recording.
/// Prepared for the recorder slice.
struct StopCapsule: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label("Stop", systemImage: "stop.fill")
                .labelStyle(.titleAndIcon)
        }
        .buttonStyle(PaperCapsuleStyle(kind: .record))
    }
}
