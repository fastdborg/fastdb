// swift-tools-version: 5.9
import PackageDescription
let package = Package(
    name: "FastDB",
    products: [.library(name: "FastDB", targets: ["FastDB"])],
    targets: [
        .systemLibrary(name: "CFastDB"),
        .target(name: "FastDB", dependencies: ["CFastDB"]),
        .testTarget(name: "FastDBTests", dependencies: ["FastDB"])
    ]
)
