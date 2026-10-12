import InkCore
import SwiftUI

struct ContentView: View {
    @StateObject private var store = MeetingStore()

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Text("iPad ink research")
                        .font(.largeTitle.bold())
                    Text("A local, in-person recording and Pencil note-taking experiment. This is not the Yawn product.")
                        .foregroundStyle(.secondary)

                    startGate
                    canvas
                    replay
                    transcript
                    exportNote
                }
                .padding()
            }
            .navigationTitle("iPad Ink")
            .task { await store.refreshSpeechModelStatus() }
            .onReceive(Timer.publish(every: 0.2, on: .main, in: .common).autoconnect()) { _ in
                store.refreshPlaybackTime()
            }
        }
    }

    private var startGate: some View {
        GroupBox("Before recording") {
            VStack(alignment: .leading, spacing: 12) {
                Toggle("Everyone who can be heard agreed to this recording", isOn: $store.consentConfirmed)
                Picker("Keep audio for", selection: $store.keepDays) {
                    Text("Choose a period").tag(nil as Int?)
                    Text("1 day").tag(1 as Int?)
                    Text("7 days").tag(7 as Int?)
                    Text("30 days").tag(30 as Int?)
                }
                .pickerStyle(.menu)

                HStack {
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
                        Button("Transcribe", action: store.transcribeSavedAudio)
                            .buttonStyle(.bordered)
                    }
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
        case .failed(let message): message
        }
    }

    private var canvas: some View {
        GroupBox("Ink") {
            PencilCanvas(
                drawing: store.displayedDrawing,
                isEditable: !store.isReplaying,
                onStrokesAdded: store.addStrokes,
                onStrokeTapped: store.seekToStroke(at:)
            )
            .frame(minHeight: 380)
            .clipShape(RoundedRectangle(cornerRadius: 12))
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

    private var transcript: some View {
        GroupBox("Transcript") {
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

    private var exportNote: some View {
        Text("Exports stay in Files › On My iPad › iPad Ink › meetings. Each meeting contains audio.wav, strokes.json, transcript.json, meeting.json, and drawing.data.")
            .font(.footnote)
            .foregroundStyle(.secondary)
    }
}
