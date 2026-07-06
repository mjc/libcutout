// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "CutoutSourceKitWorkspace",
    platforms: [
        .iOS("27.0"),
        .macOS("27.0"),
    ],
    products: [
        .library(name: "CutoutMobile", targets: ["CutoutMobile"]),
    ],
    targets: [
        .systemLibrary(
            name: "cutout_mobile_ffiFFI",
            path: "swift/CutoutMobile/Sources/cutout_mobile_ffiFFI"
        ),
        .target(
            name: "CutoutMobile",
            dependencies: ["cutout_mobile_ffiFFI"],
            path: "swift/CutoutMobile/Sources/CutoutMobile"
        ),
        .testTarget(
            name: "CutoutMobileTests",
            dependencies: ["CutoutMobile"],
            path: "swift/CutoutMobile/Tests/CutoutMobileTests"
        ),
        .executableTarget(
            name: "CutoutApp",
            dependencies: ["CutoutMobile"],
            path: "swift/CutoutMobile/Apps/CutoutApp"
        ),
        .executableTarget(
            name: "CutoutLiveActivityExtension",
            dependencies: ["CutoutMobile"],
            path: "swift/CutoutMobile/Apps/CutoutLiveActivityExtension",
            resources: [.process("Assets.xcassets")]
        ),
        .executableTarget(
            name: "CutoutMobileSmoke",
            dependencies: ["CutoutMobile"],
            path: "swift/CutoutMobile/Tests/CutoutMobileSmoke"
        ),
        .executableTarget(
            name: "CutoutMobileLiveValidator",
            dependencies: ["CutoutMobile"],
            path: "swift/CutoutMobile/Tests/CutoutMobileLiveValidator"
        ),
    ]
)
