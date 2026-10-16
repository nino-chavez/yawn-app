import SwiftUI

/// The consent card's one button: full width, 50pt, Record with On record
/// text; faded while the attestations are incomplete, the way every disabled
/// control fades.
struct PaperStartButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    @ScaledMetric(relativeTo: .body) private var height = PaperMetrics.startButtonHeight

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body.weight(.semibold))
            .foregroundStyle(Paper.onRecord)
            .frame(maxWidth: .infinity, minHeight: height)
            .background(Paper.record, in: .capsule)
            .opacity(isEnabled ? (configuration.isPressed ? 0.8 : 1) : 0.45)
    }
}
