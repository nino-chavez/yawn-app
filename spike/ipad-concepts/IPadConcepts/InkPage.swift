import PencilKit
import SwiftUI

// One page used by every concept. The invented notes sit underneath a live
// PencilKit canvas, so a finger can write on any concept and two fingers scroll,
// as in the spike (PR #116, #117).

enum PageMetrics {
    static let top: CGFloat = 56
    static let lineHeight: CGFloat = 88
    static let inkSize: CGFloat = 46

    static func lineY(_ index: Int) -> CGFloat { top + CGFloat(index) * lineHeight }

    /// Where on the page a moment in the meeting sits, between the lines written around it.
    static func y(forSeconds t: Int, lines: [NoteLine]) -> CGFloat {
        var points: [(Int, CGFloat)] = [(0, top - 24)]
        for (i, line) in lines.enumerated() {
            if let s = line.seconds { points.append((s, lineY(i) + 20)) }
        }
        points.append((Mock.meetingLength, lineY(lines.count) + 20))
        for (a, b) in zip(points, points.dropFirst()) where t <= b.0 {
            let span = max(1, b.0 - a.0)
            return a.1 + (b.1 - a.1) * CGFloat(t - a.0) / CGFloat(span)
        }
        return points.last!.1
    }
}

/// The ink size that keeps a line inside the room it has, so a narrow page
/// shrinks the writing instead of cutting it off.
func fittedInkSize(_ text: String, room: CGFloat) -> CGFloat {
    let size = PageMetrics.inkSize
    let font = ["BradleyHandITCTT-Bold", "Noteworthy-Bold", "MarkerFelt-Wide"].lazy.compactMap { UIFont(name: $0, size: size) }.first
        ?? .systemFont(ofSize: size)
    let width = (text as NSString).size(withAttributes: [.font: font]).width
    return width > room ? max(size * 0.5, size * room / width) : size
}

enum AfterStopStyle { case blueInk, marginMark, belowRail }

struct RailSpec {
    var endSeconds: Int
    var live = false
    var playhead: Int?
    var interruptedAt: Int?
}

struct InkLayer: View {
    var lines: [NoteLine]
    var dimAfter: Int? = nil
    var selectedID: Int? = nil
    var afterStop: NoteLine? = nil
    var afterStopStyle: AfterStopStyle = .blueInk
    var leadingInset: CGFloat = 0
    var rail: RailSpec? = nil
    var alignedTranscript: [TranscriptTurn]? = nil
    var transcriptColumn: CGFloat = 0

    static func height(lineCount: Int, hasAfterStop: Bool) -> CGFloat {
        PageMetrics.lineY(lineCount) + (hasAfterStop ? 160 : 40) + 240
    }

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .topLeading) {
                Color(.systemBackground)
                if let rail { railView(rail) }
                ForEach(Array(lines.enumerated()), id: \.element.id) { index, line in
                    inkLine(line, y: PageMetrics.lineY(index), dimmed: isDimmed(line), selected: line.id == selectedID, blue: false, room: room(geo.size.width, line))
                }
                if let afterStop { afterStopView(afterStop, room: room(geo.size.width, afterStop)) }
                if let alignedTranscript { aligned(alignedTranscript, width: geo.size.width) }
            }
        }
    }

    /// Width left for a line before the page edge or the transcript column.
    private func room(_ width: CGFloat, _ line: NoteLine) -> CGFloat {
        max(120, width - transcriptColumn - leadingInset - line.indent - 40)
    }

    private func isDimmed(_ line: NoteLine) -> Bool {
        guard let dimAfter, let s = line.seconds else { return false }
        return s > dimAfter
    }

    private func inkLine(_ line: NoteLine, y: CGFloat, dimmed: Bool, selected: Bool, blue: Bool, room: CGFloat) -> some View {
        Text(line.text)
            .font(inkFont(fittedInkSize(line.text, room: room - 20)))
            .foregroundStyle(blue ? Color(.systemBlue) : Color.primary)
            .opacity(dimmed ? 0.2 : 1)
            .fixedSize()
            .padding(.horizontal, 10)
            .padding(.vertical, 2)
            .background {
                if selected {
                    RoundedRectangle(cornerRadius: 10)
                        .fill(Color.accentColor.opacity(0.12))
                        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.accentColor, lineWidth: 2))
                }
            }
            .rotationEffect(.degrees(line.tilt))
            .offset(x: leadingInset + line.indent, y: y)
    }

    @ViewBuilder
    private func afterStopView(_ line: NoteLine, room: CGFloat) -> some View {
        let y = PageMetrics.lineY(lines.count) + (afterStopStyle == .belowRail ? 64 : 12)
        switch afterStopStyle {
        case .blueInk:
            inkLine(line, y: y, dimmed: false, selected: false, blue: true, room: room)
        case .marginMark:
            ZStack(alignment: .topLeading) {
                Capsule()
                    .stroke(style: StrokeStyle(lineWidth: 2, dash: [4, 4]))
                    .foregroundStyle(.secondary)
                    .frame(width: 4, height: 56)
                    .offset(x: 14, y: y + 6)
                inkLine(line, y: y, dimmed: false, selected: false, blue: false, room: room)
            }
        case .belowRail:
            ZStack(alignment: .topLeading) {
                HStack(spacing: 10) {
                    Text("After the meeting")
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Rectangle()
                        .fill(Color.secondary.opacity(0.4))
                        .frame(height: 1)
                }
                .padding(.trailing, transcriptColumn + 24)
                .offset(x: leadingInset + 20, y: y - 40)
                inkLine(line, y: y, dimmed: false, selected: false, blue: false, room: room)
            }
        }
    }

    private func railView(_ rail: RailSpec) -> some View {
        let x: CGFloat = 64
        let topY = PageMetrics.top - 24
        let endY = PageMetrics.y(forSeconds: rail.interruptedAt ?? rail.endSeconds, lines: lines)
        return ZStack(alignment: .topLeading) {
            Rectangle()
                .fill(Color.secondary.opacity(0.35))
                .frame(width: 3, height: max(0, endY - topY))
                .offset(x: x - 1.5, y: topY)
            ForEach(Array(lines.enumerated()), id: \.element.id) { index, line in
                if let s = line.seconds {
                    let y = PageMetrics.lineY(index) + 20
                    Circle()
                        .fill(isDimmed(line) ? Color.secondary.opacity(0.4) : Color.primary)
                        .frame(width: 9, height: 9)
                        .offset(x: x - 4.5, y: y - 4.5)
                    Text(Mock.clock(s))
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .frame(width: 46, alignment: .trailing)
                        .offset(x: x - 56, y: y - 8)
                }
            }
            if rail.live {
                HStack(spacing: 6) {
                    Circle().fill(Color.red).frame(width: 14, height: 14)
                    Text(Mock.clock(rail.endSeconds))
                        .font(.caption.monospacedDigit().weight(.semibold))
                        .foregroundStyle(.red)
                }
                .offset(x: x - 7, y: endY - 7)
            }
            if rail.interruptedAt != nil {
                Image(systemName: "exclamationmark.circle.fill")
                    .foregroundStyle(.orange)
                    .font(.title3)
                    .offset(x: x - 10, y: endY - 10)
                HStack(alignment: .center, spacing: 16) {
                    InterruptionMessage()
                    ResumeButton()
                }
                .padding(14)
                .frame(maxWidth: 560, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 14).fill(Color.orange.opacity(0.12)))
                .offset(x: x + 24, y: endY + 18)
            }
            if let playhead = rail.playhead {
                let y = PageMetrics.y(forSeconds: playhead, lines: lines)
                HStack(spacing: 0) {
                    Text(Mock.clock(playhead))
                        .font(.caption.monospacedDigit().weight(.semibold))
                        .foregroundStyle(.white)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Capsule().fill(Color.accentColor))
                    Rectangle().fill(Color.accentColor).frame(width: 24, height: 2)
                }
                .offset(x: 4, y: y - 10)
            }
        }
    }

    private func aligned(_ turns: [TranscriptTurn], width: CGFloat) -> some View {
        let x = width - transcriptColumn
        var lastBottom: CGFloat = 0
        let placed: [(TranscriptTurn, CGFloat)] = turns.map { turn in
            let target = PageMetrics.y(forSeconds: turn.seconds, lines: lines) - 10
            let y = max(target, lastBottom + 8)
            lastBottom = y + 58
            return (turn, y)
        }
        return ZStack(alignment: .topLeading) {
            Rectangle()
                .fill(Color.secondary.opacity(0.2))
                .frame(width: 1)
                .frame(maxHeight: .infinity)
                .offset(x: x - 16)
            ForEach(placed, id: \.0.id) { turn, y in
                VStack(alignment: .leading, spacing: 2) {
                    Text(Mock.clock(turn.seconds))
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                    Text(turn.text)
                        .font(.subheadline)
                        .foregroundStyle(isTurnDimmed(turn) ? .secondary : .primary)
                        .lineLimit(2)
                }
                .frame(width: transcriptColumn - 24, alignment: .leading)
                .padding(4)
                .background {
                    if isTurnCurrent(turn) {
                        RoundedRectangle(cornerRadius: 8).fill(Color.accentColor.opacity(0.12))
                    }
                }
                .offset(x: x - 4, y: y)
            }
        }
    }

    private func isTurnDimmed(_ turn: TranscriptTurn) -> Bool {
        guard let dimAfter else { return false }
        return turn.seconds > dimAfter
    }

    private func isTurnCurrent(_ turn: TranscriptTurn) -> Bool {
        guard let dimAfter else { return false }
        let current = Mock.transcript.last { $0.seconds <= dimAfter }
        return current?.id == turn.id
    }
}

/// A PencilKit canvas that scrolls a rendered picture of the invented notes
/// underneath its strokes. The picture is redrawn when the width or the
/// light/dark appearance changes.
final class ConceptCanvasView: PKCanvasView {
    let backing = UIImageView()
    var notes: InkLayer?
    var minimumContentHeight: CGFloat = 0
    var initialOffset: CGFloat = 0
    private var placedInitialOffset = false
    private var renderedKey = ""

    override init(frame: CGRect) {
        super.init(frame: frame)
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: ConceptCanvasView, _: UITraitCollection) in
            view.renderedKey = ""
            view.setNeedsLayout()
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.width > 0, bounds.height > 0 else { return }
        let lowest = drawing.bounds.isNull ? 0 : drawing.bounds.maxY
        let height = max(bounds.height + 120, lowest + 600, minimumContentHeight)
        let size = CGSize(width: bounds.width, height: height)
        if contentSize != size { contentSize = size }
        if backing.superview !== self { insertSubview(backing, at: 0) } else if subviews.first !== backing { sendSubviewToBack(backing) }
        let style = traitCollection.userInterfaceStyle
        let key = "\(Int(size.width))x\(Int(size.height))-\(style.rawValue)"
        if key != renderedKey, let notes {
            renderedKey = key
            let content = notes
                .frame(width: size.width, height: size.height)
                .environment(\.colorScheme, style == .dark ? .dark : .light)
            let renderer = ImageRenderer(content: content)
            renderer.scale = traitCollection.displayScale
            renderer.proposedSize = ProposedViewSize(size)
            backing.image = renderer.uiImage
            if backing.image == nil { print("ink layer did not render at \(key)") }
            backing.frame = CGRect(origin: .zero, size: size)
        }
        if !placedInitialOffset {
            placedInitialOffset = true
            // A negative offset means "show the newest writing": the last line
            // sits near the bottom with room below it to keep writing.
            let wanted = initialOffset < 0 ? minimumContentHeight - 240 + 200 - bounds.height : initialOffset
            contentOffset.y = min(max(0, wanted), max(0, height - bounds.height))
        }
    }
}

struct InkPage: UIViewRepresentable {
    var layer: InkLayer
    var contentHeight: CGFloat
    var initialOffset: CGFloat = 0

    func makeUIView(context: Context) -> ConceptCanvasView {
        let canvas = ConceptCanvasView(frame: .zero)
        canvas.drawingPolicy = UIPencilInteraction.prefersPencilOnlyDrawing ? .pencilOnly : .anyInput
        canvas.tool = PKInkingTool(.monoline, color: .label, width: 5)
        canvas.backgroundColor = .clear
        canvas.isOpaque = false
        canvas.alwaysBounceVertical = true
        canvas.initialOffset = initialOffset
        canvas.minimumContentHeight = contentHeight
        canvas.notes = layer
        return canvas
    }

    func updateUIView(_ canvas: ConceptCanvasView, context: Context) {
        canvas.minimumContentHeight = contentHeight
        canvas.setNeedsLayout()
    }
}
