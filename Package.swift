// swift-tools-version: 6.0

import Foundation
import PackageDescription

private let hostRustLibraryDirectory = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent()
    .appending(path: "target/debug")
    .path

#if os(Linux)
private let hostRustLinkerSettings: [LinkerSetting] = [
    .unsafeFlags([
        "-L\(hostRustLibraryDirectory)",
        "-lcutout_mobile_ffi",
        "-Xlinker", "-rpath",
        "-Xlinker", hostRustLibraryDirectory,
    ]),
]
#else
private let hostRustLinkerSettings: [LinkerSetting] = [
    .unsafeFlags([
        "-L\(hostRustLibraryDirectory)",
        "-lcutout_mobile_ffi",
        "-Xlinker", "-rpath",
        "-Xlinker", hostRustLibraryDirectory,
    ], .when(platforms: [.macOS])),
]
#endif

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
            path: "swift/CutoutMobile/Sources/CutoutMobile",
            linkerSettings: hostRustLinkerSettings
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
