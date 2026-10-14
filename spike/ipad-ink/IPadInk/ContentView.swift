import InkCore
import SwiftUI

struct ContentView: View {
    @StateObject private var store = MeetingStore()
    @StateObject private var canvasController = PencilCanvasController()
    @State private var selectedTool: PencilCanvas.Tool = .pen

    var body: some View {
        NavigationStack {
            GeometryReader { proxy in
                // The page gets the screen. Replay and the transcript sit beside it
                // in landscape and below it in portrait; nothing around the page
                // scrolls, so the page's own scrolling never competes with it.
                let landscape = proxy.size.width > proxy.size.height
                VStack(alignment: .leading, spacing: 12) {
                    controls
                    if landscape {
                        HStack(alignment: .top, spacing: 12) {
                            page
                            sidePanel.frame(width: 340)
                        }
                    } else {
                        VStack(spacing: 12) {
                            page
                            sidePanel.frame(height: 240)
                        }
                    }
                }
                .padding()
            }
            .navigationTitle("iPad Ink")
            .navigationBarTitleDisplayMode(.inline)
            .task { await store.refreshSpeechModelStatus() }
            .onReceive(Timer.publish(every: 0.2, on: .main, in: .common).autoconnect()) { _ in
                store.refreshPlaybackTime()
            }
        }
    }

    private var controls: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Toggle("Everyone who can be heard agreed to this recording", isOn: $store.consentConfirmed)
                HStack(spacing: 12) {
                    Picker("Keep audio for", selection: $store.keepDays) {
                        Text("Choose a period").tag(nil as Int?)
                        Text("1 day").tag(1 as Int?)
                        Text("7 days").tag(7 as Int?)
                        Text("30 days").tag(30 as Int?)
                    }
                    .pickerStyle(.menu)
                    Button("Start recording", action: store.startRecording)
                        .buttonStyle(.borderedProminent)
                        .disabled(!store.canStart)
                    if store.isRecording {
                        Button("Stop recording", action: store.stopRecording)
                            .buttonStyle(.bordered)
                    }
                    if store.canStartNewMeeting {
                        Button("New meeting", action: store.startNewMeeting)
                            .buttonStyle(.bordered)
                    }
                    if store.recordingState == .needsSpeechModel, !store.speechModelInstallable {
                        Button("Transcribe") { store.transcribeSavedAudio() }
                            .buttonStyle(.bordered)
                    }
                    Spacer(minLength: 0)
                }
                stateText
                HStack(spacing: 8) {
                    Text(store.speechModelStatus)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                    if store.speechModelInstallable {
                        Button("Install", action: store.installSpeechModel)
                            .font(.footnote)
                    }
                    Spacer(minLength: 0)
                    Text("Research spike, not the Yawn product.")
                        .font(.footnote)
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }

    private var stateText: some View {
        Text(stateDescription)
            .font(.footnote)
            .foregroundStyle(isFailed ? AnyShapeStyle(.red) : AnyShapeStyle(.secondary))
    }

    private var isFailed: Bool {
        if case .failed = store.recordingState { return true }
        return false
    }

    private var stateDescription: String {
        switch store.recordingState {
        case .ready: "Choose consent and an audio keep period to begin."
        case .recording: "Recording microphone audio locally. New pencil strokes are timed against this recording."
        case .stopped: "Recording stopped. Audio, strokes, transcript, and drawing are in this meeting’s Files folder."
        case .transcribing: "Transcribing the saved audio on this iPad. Results may still change."
        case .needsSpeechModel: "The recording is saved. Install Apple’s on-device speech model to transcribe it."
        case .interrupted(let message): "Needs attention: \(message)"
        case .failed(let message): message
        }
    }

    private var page: some View {
        PencilCanvas(
            drawing: store.displayedDrawing,
            isEditable: store.canDraw,
            tool: selectedTool,
            controller: canvasController,
            onDrawingChanged: store.drawingChanged,
            onStrokeTapped: store.seekToStroke(at:)
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(alignment: .topLeading) {
            HStack(spacing: 8) {
                Button("Pen", systemImage: "pencil.tip") {
                    selectedTool = .pen
                }
                .frame(minWidth: 44, minHeight: 44)
                .buttonStyle(selectedTool == .pen ? .borderedProminent : .bordered)
                .accessibilityLabel("Pen")

                Button("Eraser", systemImage: "eraser") {
                    selectedTool = .eraser
                }
                .frame(minWidth: 44, minHeight: 44)
                .buttonStyle(selectedTool == .eraser ? .borderedProminent : .bordered)
                .accessibilityLabel("Eraser")

                Button("Undo", systemImage: "arrow.uturn.backward", action: canvasController.undo)
                    .frame(minWidth: 44, minHeight: 44)
                    .buttonStyle(.bordered)
                    .accessibilityLabel("Undo")

                Button("Redo", systemImage: "arrow.uturn.forward", action: canvasController.redo)
                    .frame(minWidth: 44, minHeight: 44)
                    .buttonStyle(.bordered)
                    .accessibilityLabel("Redo")
            }
            .controlSize(.large)
            .padding(8)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
            .padding(12)
            .disabled(!store.canDraw)
        }
        .overlay(alignment: .bottomTrailing) {
            Text("The page grows as you write. Scroll with two fingers.")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .padding(10)
                .allowsHitTesting(false)
        }
    }

    private var sidePanel: some View {
        VStack(alignment: .leading, spacing: 12) {
            replay
            GroupBox("Transcript") {
                ScrollView {
                    transcript
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            .frame(maxHeight: .infinity)
            Text("Exports stay in Files › On My iPad › iPad Ink › meetings.")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var replay: some View {
        if store.playbackDuration > 0 {
            GroupBox("Replay") {
                VStack(alignment: .leading) {
                    HStack {
                        Button("Play or pause", action: store.togglePlayback)
                        Text("\(store.playbackTime, format: .number.precision(.fractionLength(1))) seconds")
                            .monospacedDigit()
                    }
                    Slider(value: Binding(get: { store.playbackTime }, set: store.seek(to:)), in: 0...store.playbackDuration)
                }
            }
        }
    }

    @ViewBuilder
    private var transcript: some View {
        if store.transcript.isEmpty {
            Text("No transcript text is available yet.")
                .foregroundStyle(.secondary)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(store.transcript) { segment in
                    Button {
                        store.seek(to: segment.start)
                    } label: {
                        HStack(alignment: .firstTextBaseline) {
                            Text(segment.isFinal ? "Final" : "In progress")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(segment.isFinal ? AnyShapeStyle(.secondary) : AnyShapeStyle(.orange))
                            Text(segment.text)
                                .multilineTextAlignment(.leading)
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }
}
