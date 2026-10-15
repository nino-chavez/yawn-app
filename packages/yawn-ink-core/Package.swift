// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "InkCore",
    platforms: [.macOS(.v14), .iOS(.v26)],
    products: [
        .library(name: "InkCore", targets: ["InkCore"])
    ],
    targets: [
        .target(name: "InkCore"),
        .testTarget(name: "InkCoreTests", dependencies: ["InkCore"])
    ]
)
