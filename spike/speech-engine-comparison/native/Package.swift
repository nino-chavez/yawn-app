// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "native-probe",
    platforms: [.macOS(.v26)],
    dependencies: [
        // Pin the supported release line. Package.resolved records the exact revision used.
        .package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.15.6")
    ],
    targets: [
        .executableTarget(
            name: "native-probe",
            dependencies: [.product(name: "FluidAudio", package: "FluidAudio")],
            path: "Sources/native-probe",
            swiftSettings: [.swiftLanguageMode(.v6)]
        )
    ]
)
