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
    /// A symbol-only capsule: one fixed round size whatever the symbol's
    /// width, so the tools in a column match at every text size.
    var square = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .paperCapsule(kind, labelColor: labelColor, square: square, pressed: configuration.isPressed)
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
    let square: Bool
    let pressed: Bool

    @ScaledMetric(relativeTo: .subheadline) private var height = PaperMetrics.capsuleHeight

    func body(content: Content) -> some View {
        content
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(labelColor ?? foreground)
            .padding(.horizontal, square ? 0 : PaperMetrics.capsulePadding)
            .frame(width: square ? height : nil, height: square ? height : nil)
            .frame(minHeight: height)
            .background(fill, in: .capsule)
            .overlay { Capsule().strokeBorder(border, lineWidth: kind == .active ? 1.5 : 1) }
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
        case .record: .clear
        // The active tool carries a ring as well as its tint, so selection
        // is not color alone.
        case .active: Paper.moment
        }
    }
}

extension View {
    func paperCapsule(
        _ kind: PaperCapsuleStyle.Kind = .plain,
        labelColor: Color? = nil,
        square: Bool = false,
        pressed: Bool = false
    ) -> some View {
        modifier(PaperCapsuleModifier(kind: kind, labelColor: labelColor, square: square, pressed: pressed))
    }
}
