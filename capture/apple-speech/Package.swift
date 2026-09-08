// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "AppleSpeech",
    platforms: [.macOS(.v14)],
    products: [.executable(name: "apple-speech", targets: ["apple-speech"])],
    targets: [.executableTarget(name: "apple-speech")]
)
