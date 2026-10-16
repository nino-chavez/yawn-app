import SwiftUI

@main
struct YawnApp: App {
    var body: some Scene {
        WindowGroup {
            // In a debug build, `-paperSpecimen` opens the prepared components
            // for captures (engineers only); the app itself opens to the page.
            #if DEBUG
            if CommandLine.arguments.contains("-paperSpecimen") {
                PaperSpecimen()
            } else {
                RootView()
            }
            #else
            RootView()
            #endif
        }
    }
}
