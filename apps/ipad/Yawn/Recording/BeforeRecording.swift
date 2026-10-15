import SwiftUI

/// The step before every recording, full screen so the room can see it
/// (Concept C). Consent and a keep period are both unmade until chosen.
struct BeforeRecording: View {
    let speechModel: MeetingRecorder.SpeechModel
    let onInstallSpeechModel: () -> Void
    let onStart: (Int) -> Void
    let onCancel: () -> Void

    @State private var agreed = false
    @State private var keepDays: Int?

    var body: some View {
        GeometryReader { geometry in
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("Before you record")
                    .font(.largeTitle.weight(.bold))
                Text("Say out loud that you’re recording, and check that everyone in the room is okay with it.")
                    .font(.body)
                    .foregroundStyle(.secondary)
                Toggle(isOn: $agreed) {
                    // Wraps rather than truncating: the room must read all of it.
                    Text("Everyone who can be heard agreed to this recording")
                        .fixedSize(horizontal: false, vertical: true)
                }
                .font(.body.weight(.medium))
                VStack(alignment: .leading, spacing: 8) {
                    Text("Keep the audio for")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    Picker("Keep the audio for", selection: $keepDays) {
                        Text("1 day").tag(1 as Int?)
                        Text("7 days").tag(7 as Int?)
                        Text("30 days").tag(30 as Int?)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                }
                Label("Stays on this iPad", systemImage: "ipad.landscape")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                speechModelLine
                Button {
                    if let keepDays { onStart(keepDays) }
                } label: {
                    Label("Start recording", systemImage: "record.circle")
                        .font(.title3.weight(.semibold))
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 6)
                }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.capsule)
                .tint(.red)
                .disabled(!agreed || keepDays == nil)
                Button("Not now", action: onCancel)
                    .frame(maxWidth: .infinity)
            }
            .padding(32)
            .frame(maxWidth: 520)
            .background(RoundedRectangle(cornerRadius: 24).fill(Color(.systemBackground)))
            .padding(24)
            // Centered on screen; scrolls only when it no longer fits.
            .frame(maxWidth: .infinity, minHeight: geometry.size.height)
        }
        .scrollBounceBehavior(.basedOnSize)
        }
        .background(Color(.secondarySystemBackground))
    }

    @ViewBuilder
    private var speechModelLine: some View {
        switch speechModel {
        case .installed, .checking:
            EmptyView()
        case .installable:
            HStack {
                Text("Transcripts need Apple’s on-device speech model. You can record now and transcribe once it’s installed.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Button("Install", action: onInstallSpeechModel)
                    .font(.footnote.weight(.semibold))
            }
        case .installing:
            Label("Installing Apple’s on-device speech model…", systemImage: "arrow.down.circle")
                .font(.footnote)
                .foregroundStyle(.secondary)
        case .unavailable:
            Text("This iPad’s language has no on-device speech model, so this meeting won’t have a transcript.")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }
}
