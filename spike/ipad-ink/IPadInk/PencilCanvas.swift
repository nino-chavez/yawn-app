import PencilKit
import SwiftUI

/// A page that grows as you write. Its scrollable height follows the bottom of
/// the drawing plus room to keep going, the pattern of Apple's "Drawing with
/// PencilKit" sample, so a large note never uses up the page.
final class GrowingCanvasView: PKCanvasView {
    /// Blank room kept below the lowest stroke.
    static let roomBelowDrawing: CGFloat = 600

    override func layoutSubviews() {
        super.layoutSubviews()
        fitContentToDrawing()
    }

    func fitContentToDrawing() {
        guard bounds.width > 0, bounds.height > 0 else { return }
        let drawingBounds = drawing.bounds
        let lowest = drawingBounds.isNull ? 0 : drawingBounds.maxY
        let height = max(bounds.height + Self.roomBelowDrawing, lowest + Self.roomBelowDrawing)
        let size = CGSize(width: bounds.width, height: height)
        if contentSize != size {
            contentSize = size
            // A scroll view keeps its offset when the content shrinks; pull it
            // back so the view never rests past the end of the page.
            let lastOffset = max(0, size.height - bounds.height)
            if contentOffset.y > lastOffset {
                contentOffset.y = lastOffset
            }
        }
    }
}

struct PencilCanvas: UIViewRepresentable {
    let drawing: PKDrawing
    let isEditable: Bool
    let onStrokesAdded: ([PKStroke], PKDrawing) -> Void
    let onStrokeTapped: (Int) -> Void

    /// Who may write, per the iPad's "Only Draw with Apple Pencil" setting. With
    /// it off, a finger or the Pencil writes and two fingers scroll; with it on,
    /// only the Pencil writes and one finger scrolls. Not `.default`: that honors
    /// the setting only while a tool picker shows, and this page has none, so a
    /// finger could never write.
    static var preferredDrawingPolicy: PKCanvasViewDrawingPolicy {
        UIPencilInteraction.prefersPencilOnlyDrawing ? .pencilOnly : .anyInput
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(onStrokesAdded: onStrokesAdded, onStrokeTapped: onStrokeTapped)
    }

    func makeUIView(context: Context) -> GrowingCanvasView {
        let canvas = GrowingCanvasView()
        context.coordinator.followPencilSetting(of: canvas)
        canvas.backgroundColor = .secondarySystemBackground
        canvas.isOpaque = true
        canvas.alwaysBounceVertical = true
        canvas.showsVerticalScrollIndicator = true
        canvas.contentInsetAdjustmentBehavior = .never
        canvas.delegate = context.coordinator
        context.coordinator.apply(drawing, to: canvas)
        canvas.drawingGestureRecognizer.isEnabled = isEditable

        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.didTapCanvas(_:)))
        tap.cancelsTouchesInView = false
        canvas.addGestureRecognizer(tap)
        return canvas
    }

    func updateUIView(_ canvas: GrowingCanvasView, context: Context) {
        // Replay turns drawing off but keeps scrolling, so earlier pages stay reachable.
        canvas.drawingGestureRecognizer.isEnabled = isEditable
        guard canvas.drawing.dataRepresentation() != drawing.dataRepresentation() else { return }
        context.coordinator.apply(drawing, to: canvas)
    }

    final class Coordinator: NSObject, PKCanvasViewDelegate {
        private var knownStrokeCount = 0
        private var applyingDrawing = false
        private weak var canvas: PKCanvasView?
        private let onStrokesAdded: ([PKStroke], PKDrawing) -> Void
        private let onStrokeTapped: (Int) -> Void

        init(onStrokesAdded: @escaping ([PKStroke], PKDrawing) -> Void, onStrokeTapped: @escaping (Int) -> Void) {
            self.onStrokesAdded = onStrokesAdded
            self.onStrokeTapped = onStrokeTapped
        }

        /// Applies the Pencil setting now, and again each time the app comes back
        /// to the front, because the setting is changed in the Settings app.
        func followPencilSetting(of canvas: PKCanvasView) {
            self.canvas = canvas
            canvas.drawingPolicy = PencilCanvas.preferredDrawingPolicy
            NotificationCenter.default.addObserver(
                self, selector: #selector(appDidBecomeActive),
                name: UIApplication.didBecomeActiveNotification, object: nil
            )
        }

        @objc private func appDidBecomeActive() {
            canvas?.drawingPolicy = PencilCanvas.preferredDrawingPolicy
        }

        /// Sets a drawing from the store. The canvas reports that change through
        /// its delegate, and it must not be read back as newly drawn strokes.
        func apply(_ drawing: PKDrawing, to canvas: GrowingCanvasView) {
            // Fewer strokes than the canvas holds means a cleared page (New
            // meeting); start it at the top rather than where the last one ended.
            let cleared = drawing.strokes.count < canvas.drawing.strokes.count
            applyingDrawing = true
            canvas.drawing = drawing
            knownStrokeCount = drawing.strokes.count
            applyingDrawing = false
            canvas.fitContentToDrawing()
            if cleared {
                canvas.setContentOffset(.zero, animated: false)
            }
        }

        func canvasViewDrawingDidChange(_ canvasView: PKCanvasView) {
            (canvasView as? GrowingCanvasView)?.fitContentToDrawing()
            guard !applyingDrawing else { return }
            let strokes = canvasView.drawing.strokes
            let additions = Array(strokes.dropFirst(knownStrokeCount))
            knownStrokeCount = strokes.count
            guard !additions.isEmpty else { return }
            onStrokesAdded(additions, canvasView.drawing)
        }

        @objc func didTapCanvas(_ recognizer: UITapGestureRecognizer) {
            guard let canvas = recognizer.view as? PKCanvasView else { return }
            // A scroll view's own coordinates are content coordinates, the space
            // stroke bounds live in, so this hit test holds at any scroll position.
            let point = recognizer.location(in: canvas)
            guard let index = canvas.drawing.strokes.enumerated().reversed().first(where: { $0.element.renderBounds.contains(point) })?.offset else {
                return
            }
            onStrokeTapped(index)
        }
    }
}
