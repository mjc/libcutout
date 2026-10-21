#if os(macOS)
    import AppKit
    import CutoutMobileFFI
    import SwiftUI
    import XCTest

    @testable import CutoutApp

    @MainActor
    final class CaptureSendProgressLayoutTests: XCTestCase {
        func testProgressAndReceiptWaitFitNarrowAndAccessibleLayouts() throws {
            for width in [320.0, 430.0] {
                for textSize in [DynamicTypeSize.large, .accessibility3] {
                    for phase in [MobileCaptureSubmissionProgressPhaseDto.uploading, .verifyingReceipt] {
                        let progress = MobileCaptureSubmissionProgressDto(
                            operationId: "render-operation", attemptId: 1,
                            bytesSent: phase == .uploading ? 25_000_000 : 100_000_000,
                            totalBytes: 100_000_000, phase: phase)
                        let host = NSHostingView(
                            rootView: CaptureSendProgressView(progress: progress)
                                .padding(16).frame(width: width)
                                .environment(\.dynamicTypeSize, textSize)
                                .environment(\.colorScheme, .dark)
                                .background(.black))
                        let size = host.fittingSize
                        XCTAssertLessThanOrEqual(size.width, width + 0.5)
                        XCTAssertGreaterThan(size.height, 50)
                        guard let directory = ProcessInfo.processInfo.environment["CUTOUT_SEND_PROGRESS_PREVIEW_DIR"]
                        else {
                            continue
                        }
                        host.appearance = NSAppearance(named: .darkAqua)
                        host.frame = NSRect(origin: .zero, size: size)
                        host.layoutSubtreeIfNeeded()
                        let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
                        host.cacheDisplay(in: host.bounds, to: bitmap)
                        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
                        let output = URL(fileURLWithPath: directory, isDirectory: true)
                        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
                        try png.write(to: output.appendingPathComponent("send-\(phase)-\(Int(width))-\(textSize).png"))
                    }
                }
            }
        }
    }
#endif
