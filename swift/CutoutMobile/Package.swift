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
        .package(path: "../../crates/cutout-mobile-ffi/CutoutMobileFFI"),
    ],
    targets: [
        .target(
            name: "CutoutMobile",
            dependencies: [
                .product(name: "CutoutMobileFFI", package: "CutoutMobileFFI"),
            ],
            exclude: ["Generated"],
            linkerSettings: [
                .linkedLibrary("iconv", .when(platforms: [.iOS, .macOS])),
            ]
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
