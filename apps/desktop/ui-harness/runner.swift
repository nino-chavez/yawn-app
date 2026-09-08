// WKWebView harness runner: loads the real Yawn UI (stubbed Tauri bridge)
// over localhost and executes scenario.js in the page, printing its JSON
// result. Same engine as the packaged Tauri app.
import AppKit
import WebKit

final class Delegate: NSObject, NSApplicationDelegate, WKNavigationDelegate {
    let pageURL: URL
    let scenarioPath: String
    let mode: String
    let capturePath: String?
    var webView: WKWebView!
    var window: NSWindow!

    init(pageURL: URL, scenarioPath: String, mode: String, capturePath: String?) {
        self.pageURL = pageURL
        self.scenarioPath = scenarioPath
        self.mode = mode
        self.capturePath = capturePath
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let configuration = WKWebViewConfiguration()
        let components = URLComponents(url: pageURL, resolvingAgainstBaseURL: false)
        let width = CGFloat(Int(components?.queryItems?.first(where: { $0.name == "width" })?.value ?? "960") ?? 960)
        let height = CGFloat(Int(components?.queryItems?.first(where: { $0.name == "height" })?.value ?? "760") ?? 760)
        webView = WKWebView(frame: NSRect(x: 0, y: 0, width: width, height: height), configuration: configuration)
        webView.navigationDelegate = self
        window = NSWindow(
            contentRect: webView.frame,
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.title = capturePath == nil ? "Yawn undo harness" : "Yawn layout review — synthetic"
        if let appearance = components?.queryItems?.first(where: { $0.name == "appearance" })?.value {
            switch appearance.lowercased() {
            case "light": window.appearance = NSAppearance(named: .aqua)
            case "dark": window.appearance = NSAppearance(named: .darkAqua)
            default: break
            }
        }
        window.contentView = webView
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        webView.load(URLRequest(url: pageURL))
        DispatchQueue.main.asyncAfter(deadline: .now() + 45) {
            FileHandle.standardError.write(Data("TIMEOUT\n".utf8))
            exit(3)
        }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        guard let body = try? String(contentsOfFile: scenarioPath, encoding: .utf8) else {
            FileHandle.standardError.write(Data("cannot read scenario\n".utf8))
            exit(2)
        }
        webView.callAsyncJavaScript(body, arguments: ["mode": mode], in: nil, in: .page) { outcome in
            switch outcome {
            case .success(let value):
                if JSONSerialization.isValidJSONObject(value),
                   let data = try? JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys]) {
                    print(String(data: data, encoding: .utf8) ?? "\(value)")
                } else {
                    print("\(String(describing: value))")
                }
                // Every scenario is a gate. A JavaScript exception, explicit
                // error, captured page error, failed named step, or failed
                // top-level contract must make the command fail. Printing a
                // red result while returning zero hid stale selectors before.
                if let result = value as? [String: Any] {
                    let explicitFailure = (result["pass"] as? Bool) == false
                    let returnedError = result["error"] != nil
                    let capturedErrors = (result["errors"] as? [Any])?.isEmpty == false
                    let failedStep = (result["steps"] as? [[String: Any]])?.contains {
                        ($0["ok"] as? Bool) == false
                    } == true
                    if explicitFailure || returnedError || capturedErrors || failedStep {
                        self.finish(exitCode: 1)
                        return
                    }
                }
                self.finish(exitCode: 0)
            case .failure(let error):
                FileHandle.standardError.write(Data("JS ERROR: \(error)\n".utf8))
                self.finish(exitCode: 1)
            }
        }
    }

    private func finish(exitCode: Int32) {
        guard exitCode == 0, let capturePath else {
            exit(exitCode)
        }
        // Let WebKit commit the scenario's final DOM/layout work before its
        // native snapshot. This stays entirely inside the synthetic harness.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [self] in
            let configuration = WKSnapshotConfiguration()
            configuration.rect = webView.bounds
            webView.takeSnapshot(with: configuration) { image, error in
                guard let image else {
                    FileHandle.standardError.write(Data("SNAPSHOT ERROR: \(error?.localizedDescription ?? "unknown error")\n".utf8))
                    exit(1)
                }
                guard let tiff = image.tiffRepresentation,
                      let bitmap = NSBitmapImageRep(data: tiff),
                      let png = bitmap.representation(using: .png, properties: [:]) else {
                    FileHandle.standardError.write(Data("SNAPSHOT ERROR: could not encode PNG\n".utf8))
                    exit(1)
                }
                do {
                    try png.write(to: URL(fileURLWithPath: capturePath), options: .atomic)
                    exit(0)
                } catch {
                    FileHandle.standardError.write(Data("SNAPSHOT ERROR: \(error.localizedDescription)\n".utf8))
                    exit(1)
                }
            }
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        FileHandle.standardError.write(Data("NAV ERROR: \(error)\n".utf8))
        exit(2)
    }
}

let arguments = CommandLine.arguments
guard arguments.count >= 3, let pageURL = URL(string: arguments[1]) else {
    FileHandle.standardError.write(Data("usage: runner <url> <scenario.js path>\n".utf8))
    exit(2)
}
let components = URLComponents(url: pageURL, resolvingAgainstBaseURL: false)
let mode = components?.queryItems?.first(where: { $0.name == "mode" })?.value ?? "capture"
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let capturePath = ProcessInfo.processInfo.environment["HARNESS_CAPTURE_PATH"]
let delegate = Delegate(pageURL: pageURL, scenarioPath: arguments[2], mode: mode, capturePath: capturePath)
app.delegate = delegate
app.run()
