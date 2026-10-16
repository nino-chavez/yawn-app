import SwiftUI

/// The consent step before a recording (product brief, iPad in-person mode;
/// DESIGN.md, Composition, iPad): a 420pt card on Page over Window chrome.
/// Recording can start only after the operator confirms that everyone who
/// can be heard agreed, and chooses how long to keep the audio; there is no
/// default choice. Prepared for the recorder slice; nothing presents it yet.
struct ConsentCard: View {
    let start: (_ keepDays: Int) -> Void
    let notNow: () -> Void

    @State private var everyoneAgreed = false
    @State private var keepDays: Int?

    private var canStart: Bool { everyoneAgreed && keepDays != nil }

    var body: some View {
        ZStack {
            Paper.windowChrome.ignoresSafeArea()
            // The card sits centered where it fits; where it does not (a
            // narrow window, the largest text sizes) it scrolls instead.
            ViewThatFits(in: .vertical) {
                card
                ScrollView {
                    card
                        .padding(.vertical, 32)
                }
                .scrollBounceBehavior(.basedOnSize)
            }
        }
    }

    private var card: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Before recording")
                .font(.title2.weight(.semibold))
                .foregroundStyle(Paper.label)
            Text("Yawn records the room and writes its transcript on this iPad.")
                .font(.body)
                .foregroundStyle(Paper.label2)
            Toggle("Everyone who can be heard has agreed to the recording", isOn: $everyoneAgreed)
                .font(.body)
                .foregroundStyle(Paper.label)
                .tint(Paper.graphite)
            VStack(alignment: .leading, spacing: 8) {
                Text("Keep the audio")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Paper.label)
                    // The picker carries the same name for assistive technology.
                    .accessibilityHidden(true)
                Picker("Keep the audio", selection: $keepDays) {
                    ForEach([1, 7, 30], id: \.self) { days in
                        Text("^[\(days) day](inflect: true)")
                            .tag(Optional(days))
                    }
                }
                .pickerStyle(.segmented)
            }
            Text("Stays on this iPad.")
                .font(.footnote)
                .foregroundStyle(Paper.label3)
            Button("Start recording", action: begin)
                .buttonStyle(PaperStartButtonStyle())
                .disabled(!canStart)
            Button(action: notNow) {
                Text("Not now")
                    .font(.body)
                    .foregroundStyle(Paper.label2)
                    .frame(maxWidth: .infinity, minHeight: PaperMetrics.touchTarget)
            }
            .buttonStyle(.plain)
        }
        .padding(28)
        .frame(maxWidth: PaperMetrics.consentCardWidth)
        .background(Paper.page, in: .rect(cornerRadius: PaperMetrics.consentCardRadius))
        .overlay {
            RoundedRectangle(cornerRadius: PaperMetrics.consentCardRadius)
                .strokeBorder(Paper.hairline, lineWidth: 1)
        }
        .padding(.horizontal, 16)
    }

    private func begin() {
        if let keepDays { start(keepDays) }
    }
}
