// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "CutoutMobile",
    platforms: [
        .iOS("27.0"),
        .macOS("27.0"),
    ],
    products: [
        .library(name: "CutoutMobile", targets: ["CutoutMobile"]),
    ],
    dependencies: [
        .package(path: "../../target/swift-ffi/CutoutMobileFFI"),
    ],
    targets: [
        .target(
            name: "CutoutMobile",
            dependencies: [
                .product(name: "CutoutMobileFFI", package: "CutoutMobileFFI"),
            ],
            resources: [.process("Localizable.xcstrings")]
        ),
        .testTarget(
            name: "CutoutMobileTests",
            dependencies: [
                "CutoutMobile",
                .product(name: "CutoutMobileFFI", package: "CutoutMobileFFI"),
            ],
            path: "Tests/CutoutMobileTests"
        ),
        .testTarget(
            name: "CutoutAppTests",
            dependencies: [
                "CutoutApp",
                .product(name: "CutoutMobileFFI", package: "CutoutMobileFFI"),
            ],
            path: "Tests/CutoutAppTests"
        ),
        .executableTarget(
            name: "CutoutApp",
            dependencies: ["CutoutMobile"],
            path: "Apps/CutoutApp",
            resources: [.process("Localizable.xcstrings")]
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
