// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ScChat",
    platforms: [.iOS(.v15)],
    products: [
        .library(name: "ScChat", targets: ["ScChat"]),
    ],
    targets: [
        .binaryTarget(name: "ScChatFFI", path: "ScChatFFI.xcframework"),
        .target(name: "ScChat", dependencies: ["ScChatFFI"], path: "Sources/ScChat"),
    ]
)
