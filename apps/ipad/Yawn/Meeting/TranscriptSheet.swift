import InkCore
import SwiftUI

/// The full transcript of the last meeting. No speaker names until a table
/// recording proves them (product brief). Search, copy and the aligned view
/// beside the page arrive in slice 4.
struct TranscriptSheet: View {
    let segments: [TranscriptSegment]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if segments.isEmpty {
                    ContentUnavailableView("No speech was transcribed", systemImage: "text.bubble",
                                           description: Text("The recording is saved. Nothing in it was recognized as speech."))
                } else {
                    List(segments) { segment in
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Text(MeetingRecorder.clock(segment.start))
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                                .frame(minWidth: 44, alignment: .trailing)
                            Text(segment.text)
                                .textSelection(.enabled)
                        }
                    }
                    .listStyle(.plain)
                }
            }
            .navigationTitle("Transcript")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }
}
