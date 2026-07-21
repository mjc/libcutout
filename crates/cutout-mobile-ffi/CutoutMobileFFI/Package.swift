// swift-tools-version:6.0
// The swift-tools-version declares the minimum version of Swift required to build this package.
// Swift Package: CutoutMobileFFI

import PackageDescription;

let package = Package(
    name: "CutoutMobileFFI",
    platforms: [
        .iOS(.v18), .macOS(.v15)
    ],
    products: [
        .library(
            name: "CutoutMobileFFI",
            targets: ["CutoutMobileFFI"]
        )
    ],
    dependencies: [ ],
    targets: [
        .binaryTarget(name: "cutout_mobile_ffiFFI", path: "./cutout_mobile_ffiFFI.xcframework"),
        .target(
            name: "CutoutMobileFFI",
            dependencies: [
                .target(name: "cutout_mobile_ffiFFI")
            ]
        ),
    ]
)