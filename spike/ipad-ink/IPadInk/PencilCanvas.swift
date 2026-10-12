import PencilKit
import SwiftUI

struct PencilCanvas: UIViewRepresentable {
    let drawing: PKDrawing
    let isEditable: Bool
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
        context.coordinator.apply(drawing, to: canvas)
        canvas.drawingGestureRecognizer.isEnabled = isEditable

        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.didTapCanvas(_:)))
        tap.cancelsTouchesInView = false
        canvas.addGestureRecognizer(tap)
        return canvas
    }

    func updateUIView(_ canvas: PKCanvasView, context: Context) {
        canvas.drawingGestureRecognizer.isEnabled = isEditable
        guard canvas.drawing.dataRepresentation() != drawing.dataRepresentation() else { return }
        context.coordinator.apply(drawing, to: canvas)
    }

    final class Coordinator: NSObject, PKCanvasViewDelegate {
        private var knownStrokeCount = 0
        private var applyingDrawing = false
        private let onStrokesAdded: ([PKStroke], PKDrawing) -> Void
        private let onStrokeTapped: (Int) -> Void

        init(onStrokesAdded: @escaping ([PKStroke], PKDrawing) -> Void, onStrokeTapped: @escaping (Int) -> Void) {
            self.onStrokesAdded = onStrokesAdded
            self.onStrokeTapped = onStrokeTapped
        }

        /// Sets a drawing from the store. The canvas reports that change through
        /// its delegate, and it must not be read back as newly drawn strokes.
        func apply(_ drawing: PKDrawing, to canvas: PKCanvasView) {
            applyingDrawing = true
            canvas.drawing = drawing
            knownStrokeCount = drawing.strokes.count
            applyingDrawing = false
        }

        func canvasViewDrawingDidChange(_ canvasView: PKCanvasView) {
            guard !applyingDrawing else { return }
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
