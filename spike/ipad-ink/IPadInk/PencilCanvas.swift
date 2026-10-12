import PencilKit
import SwiftUI

struct PencilCanvas: UIViewRepresentable {
    let drawing: PKDrawing
    let onStrokesAdded: ([PKStroke], PKDrawing) -> Void
    let onStrokeTapped: (Int) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(onStrokesAdded: onStrokesAdded, onStrokeTapped: onStrokeTapped)
    }

    func makeUIView(context: Context) -> PKCanvasView {
        let canvas = PKCanvasView()
        canvas.drawingPolicy = .anyInput
        canvas.backgroundColor = .secondarySystemBackground
        canvas.isOpaque = true
        canvas.delegate = context.coordinator
        canvas.drawing = drawing
        context.coordinator.knownStrokeCount = drawing.strokes.count

        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.didTapCanvas(_:)))
        tap.cancelsTouchesInView = false
        canvas.addGestureRecognizer(tap)
        return canvas
    }

    func updateUIView(_ canvas: PKCanvasView, context: Context) {
        guard canvas.drawing.dataRepresentation() != drawing.dataRepresentation() else { return }
        canvas.drawing = drawing
        context.coordinator.knownStrokeCount = drawing.strokes.count
    }

    final class Coordinator: NSObject, PKCanvasViewDelegate {
        var knownStrokeCount = 0
        private let onStrokesAdded: ([PKStroke], PKDrawing) -> Void
        private let onStrokeTapped: (Int) -> Void

        init(onStrokesAdded: @escaping ([PKStroke], PKDrawing) -> Void, onStrokeTapped: @escaping (Int) -> Void) {
            self.onStrokesAdded = onStrokesAdded
            self.onStrokeTapped = onStrokeTapped
        }

        func canvasViewDrawingDidChange(_ canvasView: PKCanvasView) {
            let strokes = canvasView.drawing.strokes
            let additions = Array(strokes.dropFirst(knownStrokeCount))
            knownStrokeCount = strokes.count
            guard !additions.isEmpty else { return }
            onStrokesAdded(additions, canvasView.drawing)
        }

        @objc func didTapCanvas(_ recognizer: UITapGestureRecognizer) {
            guard let canvas = recognizer.view as? PKCanvasView else { return }
            let point = recognizer.location(in: canvas)
            guard let index = canvas.drawing.strokes.enumerated().reversed().first(where: { $0.element.renderBounds.contains(point) })?.offset else {
                return
            }
            onStrokeTapped(index)
        }
    }
}
