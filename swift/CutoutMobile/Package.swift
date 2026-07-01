// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "CutoutMobile",
    platforms: [
        .iOS(.v15),
        .macOS(.v13),
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
            dependencies: ["cutout_mobile_ffiFFI"]
        ),
        .testTarget(
            name: "CutoutMobileTests",
            dependencies: ["CutoutMobile"],
            path: "Tests/CutoutMobileTests"
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
        .executableTarget(
            name: "CutoutApp",
            dependencies: ["CutoutMobile"],
            path: "Apps/CutoutApp"
        ),
    ]
)
