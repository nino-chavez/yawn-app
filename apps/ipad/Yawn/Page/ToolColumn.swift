import SwiftUI

/// The tool column at the page's right edge while recording (DESIGN.md,
/// Composition, iPad): pen, eraser, undo, 56pt wide, 18pt apart, the active
/// tool in a Moment soft capsule. Prepared for the page slice; nothing shows
/// it yet.
struct ToolColumn: View {
    enum Tool {
        case pen
        case eraser
    }

    @Binding var tool: Tool
    let undo: () -> Void

    var body: some View {
        VStack(spacing: PaperMetrics.toolSpacing) {
            ToolCapsule(name: "Pen", symbol: "pencil.tip", active: tool == .pen) { tool = .pen }
            ToolCapsule(name: "Eraser", symbol: "eraser", active: tool == .eraser) { tool = .eraser }
            ToolCapsule(name: "Undo", symbol: "arrow.uturn.backward", active: false, action: undo)
        }
        .frame(width: PaperMetrics.toolColumnWidth)
    }
}
