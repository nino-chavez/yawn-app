import SwiftUI

/// A meeting row's small rail: the recording as a line, a dot where each
/// note was written. Decorative beside the row's text, so hidden from
/// assistive technology.
struct MiniRail: View {
    let marks: [Double]

    var body: some View {
        Canvas { context, size in
            let dot = 5.0
            let middle = size.height / 2
            var line = Path()
            line.move(to: CGPoint(x: 0, y: middle))
            line.addLine(to: CGPoint(x: size.width, y: middle))
            context.stroke(line, with: .color(Paper.rail), lineWidth: 1)
            for mark in marks {
                let x = min(max(mark, 0), 1) * (size.width - dot)
                let rect = CGRect(x: x, y: middle - dot / 2, width: dot, height: dot)
                context.fill(Path(ellipseIn: rect), with: .color(Paper.railMark))
            }
        }
        .accessibilityHidden(true)
    }
}
