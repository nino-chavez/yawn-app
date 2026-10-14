import SwiftUI

@main
struct IPadConceptsApp: App {
    var body: some Scene {
        WindowGroup { RootView() }
    }
}

enum Concept: String, CaseIterable, Identifiable {
    case a = "A", b = "B", c = "C"
    var id: String { rawValue }
    var name: String {
        switch self {
        case .a: "Notebook"
        case .b: "Pad"
        case .c: "Time rail"
        }
    }
}

enum ConceptState: Int, CaseIterable, Identifiable {
    case before = 1, during, after, attention, firstRun, list
    var id: Int { rawValue }
    var name: String {
        switch self {
        case .before: "Before"
        case .during: "During"
        case .after: "After"
        case .attention: "Needs attention"
        case .firstRun: "First run"
        case .list: "Meeting list"
        }
    }
}

/// Launch arguments `-concept A|B|C`, `-state 1…6` and `-appearance light|dark`
/// pick the starting frame; the menu in the corner switches live.
struct RootView: View {
    @State private var concept: Concept
    @State private var state: ConceptState
    private let scheme: ColorScheme?

    init() {
        let defaults = UserDefaults.standard
        _concept = State(initialValue: Concept(rawValue: defaults.string(forKey: "concept") ?? "A") ?? .a)
        _state = State(initialValue: ConceptState(rawValue: defaults.integer(forKey: "state")) ?? .before)
        switch defaults.string(forKey: "appearance") {
        case "dark": scheme = .dark
        case "light": scheme = .light
        default: scheme = nil
        }
    }

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            Group {
                switch concept {
                case .a: ConceptA(state: state)
                case .b: ConceptB(state: state)
                case .c: ConceptC(state: state)
                }
            }
            .id("\(concept.rawValue)-\(state.rawValue)")
            switcher
        }
        .preferredColorScheme(scheme)
    }

    private var switcher: some View {
        Menu {
            Picker("Concept", selection: $concept) {
                ForEach(Concept.allCases) { Text("\($0.rawValue) · \($0.name)").tag($0) }
            }
            Picker("State", selection: $state) {
                ForEach(ConceptState.allCases) { Text("\($0.rawValue) · \($0.name)").tag($0) }
            }
        } label: {
            Text("Concept \(concept.rawValue) · \(state.rawValue) \(state.name) · invented content")
                .font(.caption2.weight(.medium))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(.thinMaterial, in: Capsule())
        }
        .padding(.trailing, 10)
        .padding(.bottom, 6)
    }
}
