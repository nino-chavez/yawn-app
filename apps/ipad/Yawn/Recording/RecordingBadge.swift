import SwiftUI

/// The recording badge (DESIGN.md, Composition, iPad): a 32pt capsule on
/// Record soft in Record, with a dot, the word and the elapsed time. Prepared
/// for the recorder slice; nothing shows it yet.
struct RecordingBadge: View {
    /// The elapsed time as the recorder formats it, "12:04".
    let elapsed: String
    /// Paused keeps the surface and drops the dot's weight.
    var paused: Bool = false

    @ScaledMetric(relativeTo: .subheadline) private var height = PaperMetrics.badgeHeight

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(Paper.record)
                .frame(width: 8, height: 8)
                .opacity(paused ? 0.5 : 1)
            Text(paused ? "Paused" : "Recording")
            Text(elapsed)
                .monospacedDigit()
        }
        .font(.subheadline.weight(.semibold))
        .foregroundStyle(Paper.record)
        .padding(.horizontal, 12)
        .frame(minHeight: height)
        .background(Paper.recordSoft, in: .capsule)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(paused ? "Paused, \(elapsed)" : "Recording, \(elapsed)")
        .accessibilityAddTraits(.updatesFrequently)
    }
}
