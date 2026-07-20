// swift-tools-version: 6.0

import Foundation
import PackageDescription

private func repositoryRoot(from manifestPath: String) -> URL {
    var directory = URL(fileURLWithPath: manifestPath).deletingLastPathComponent()
    while directory.path != "/" {
        if FileManager.default.fileExists(atPath: directory.appending(path: "Cargo.toml").path) {
            return directory
        }
        directory.deleteLastPathComponent()
    }
    preconditionFailure("CutoutMobile must be built from inside the libcutout repository")
}

private let hostRustLibraryDirectory = repositoryRoot(from: #filePath)
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
    name: "CutoutMobile",
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
            path: "Sources/cutout_mobile_ffiFFI"
        ),
        .target(
            name: "CutoutMobile",
            dependencies: ["cutout_mobile_ffiFFI"],
            linkerSettings: hostRustLinkerSettings
        ),
        .testTarget(
            name: "CutoutMobileTests",
            dependencies: ["CutoutMobile"],
            path: "Tests/CutoutMobileTests"
        ),
        .testTarget(
            name: "CutoutAppTests",
            dependencies: ["CutoutApp"],
            path: "Tests/CutoutAppTests"
        ),
        .executableTarget(
            name: "CutoutApp",
            dependencies: ["CutoutMobile"],
            path: "Apps/CutoutApp"
        ),
        .executableTarget(
            name: "CutoutLiveActivityExtension",
            dependencies: ["CutoutMobile"],
            path: "Apps/CutoutLiveActivityExtension",
            resources: [.process("Assets.xcassets")]
        ),
        .executableTarget(
            name: "CutoutMobileSmoke",
            dependencies: ["CutoutMobile"],
            path: "Tests/CutoutMobileSmoke"
        ),
        .executableTarget(
            name: "CutoutMobileLiveValidator",
            dependencies: ["CutoutMobile"],
            path: "Tests/CutoutMobileLiveValidator"
        ),
    ]
)
