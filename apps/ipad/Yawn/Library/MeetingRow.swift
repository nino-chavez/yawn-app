import SwiftUI

/// A row of the meeting list (DESIGN.md, Composition, iPad): the day, time
/// and length, a mini rail of when notes were written, and the first ink
/// line. Selected is the Selection surface with a 3pt Graphite locator; the
/// text never inverts. Prepared for the meeting-list slice, which brings the
/// storage these fields come from.
struct MeetingRow: View {
    /// "Wednesday", or the date once the week has passed.
    let day: String
    /// "2:10 PM · 23:05".
    let detail: String
    /// Where in the recording the notes were written, 0 to 1.
    let marks: [Double]
    /// The first line of the operator's ink, as recognized text.
    let firstLine: String
    let selected: Bool

    var body: some View {
        HStack(spacing: 0) {
            Rectangle()
                .fill(selected ? Paper.graphite : .clear)
                .frame(width: PaperMetrics.locatorWidth)
            VStack(alignment: .leading, spacing: 4) {
                Text(day)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Paper.label)
                Text(detail)
                    .font(.caption)
                    .monospacedDigit()
                    .foregroundStyle(Paper.label3)
                MiniRail(marks: marks)
                    .frame(height: PaperMetrics.markDiameter)
                    .padding(.vertical, 2)
                Text(firstLine)
                    .font(.callout)
                    .foregroundStyle(Paper.label2)
                    .lineLimit(1)
            }
            .padding(.vertical, 10)
            .padding(.horizontal, 12)
            Spacer(minLength: 0)
        }
        .background(selected ? Paper.selection : .clear, in: .rect(cornerRadius: PaperMetrics.rowRadius))
        .clipShape(.rect(cornerRadius: PaperMetrics.rowRadius))
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
