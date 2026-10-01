// swift-tools-version: 6.1

import PackageDescription

let package = Package(
    name: "CutoutMobile",
    platforms: [
        .iOS("27.0"),
        .macOS("27.0"),
    ],
    products: [
        .library(name: "CutoutMobile", targets: ["CutoutMobile"])
    ],
    dependencies: [
        .package(name: "CutoutMobileFFI", path: "../../target/swift-ffi"),
        .package(url: "https://github.com/spotify/ios-sdk.git", from: "5.0.1"),
    ],
    targets: [
        .target(
            name: "CutoutMobile",
            dependencies: [
                .product(name: "CutoutMobileFFI", package: "CutoutMobileFFI"),
                .product(name: "SpotifyiOS", package: "ios-sdk", condition: .when(platforms: [.iOS])),
            ],
            resources: [.process("Localizable.xcstrings")],
            plugins: [.plugin(name: "VerifyRustArtifact")]
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
                "CutoutMobile",
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
            name: "CutoutMobileLiveValidator",
            dependencies: ["CutoutMobile"],
            path: "Tests/CutoutMobileLiveValidator"
        ),
        .testTarget(
            name: "CutoutMobileLiveValidatorTests",
            dependencies: ["CutoutMobileLiveValidator"],
            path: "Tests/CutoutMobileLiveValidatorTests"
        ),
        .executableTarget(
            name: "MelkLightingLiveValidator",
            dependencies: ["CutoutMobile"],
            path: "Tests/MelkLightingLiveValidator"
        ),
        .plugin(name: "VerifyRustArtifact", capability: .buildTool()),
    ]
)
