import SwiftUI

/// One tool of the tool column: a paper capsule around its symbol, Moment
/// soft when it is the active tool. The name is the button's label; only the
/// symbol shows.
struct ToolCapsule: View {
    let name: String
    let symbol: String
    let active: Bool
    let action: () -> Void

    var body: some View {
        Button(name, systemImage: symbol, action: action)
            .labelStyle(.iconOnly)
            // The active tool is heavier as well as tinted and ringed.
            .fontWeight(active ? .bold : .regular)
            .buttonStyle(PaperCapsuleStyle(kind: active ? .active : .plain, square: true))
            .accessibilityAddTraits(active ? .isSelected : [])
    }
}
