import SwiftUI

/// The iPad capsule (DESIGN.md, Components and states): 36pt, a full radius,
/// Control with its one border. `.record` fills Record, for Stop; `.active`
/// sits the active tool on Moment soft in Moment. The height follows Dynamic
/// Type; the touch target is the platform's 44pt whatever the capsule's size.
struct PaperCapsuleStyle: ButtonStyle {
    enum Kind {
        case plain
        case record
        case active
    }

    var kind: Kind = .plain
    /// A label color other than the kind's own; Record uses it to keep its
    /// word red on an otherwise plain capsule.
    var labelColor: Color?

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .paperCapsule(kind, labelColor: labelColor, pressed: configuration.isPressed)
            .frame(minHeight: PaperMetrics.touchTarget)
            .contentShape(.rect)
    }
}

/// The capsule's look, usable on a plain view as well as through the button
/// style, so a label that is not yet tappable can still read as the control
/// it will become.
struct PaperCapsuleModifier: ViewModifier {
    let kind: PaperCapsuleStyle.Kind
    let labelColor: Color?
    let pressed: Bool

    @ScaledMetric(relativeTo: .subheadline) private var height = PaperMetrics.capsuleHeight

    func body(content: Content) -> some View {
        content
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(labelColor ?? foreground)
            .padding(.horizontal, PaperMetrics.capsulePadding)
            .frame(minHeight: height)
            .background(fill, in: .capsule)
            .overlay { Capsule().strokeBorder(border, lineWidth: 1) }
            .opacity(pressed ? 0.8 : 1)
    }

    private var foreground: Color {
        switch kind {
        case .plain: Paper.label
        case .record: Paper.onRecord
        case .active: Paper.moment
        }
    }

    private var fill: Color {
        switch kind {
        case .plain: Paper.control
        case .record: Paper.record
        case .active: Paper.momentSoft
        }
    }

    private var border: Color {
        switch kind {
        case .plain: Paper.controlBorder
        case .record, .active: .clear
        }
    }
}

extension View {
    func paperCapsule(
        _ kind: PaperCapsuleStyle.Kind = .plain,
        labelColor: Color? = nil,
        pressed: Bool = false
    ) -> some View {
        modifier(PaperCapsuleModifier(kind: kind, labelColor: labelColor, pressed: pressed))
    }
}
