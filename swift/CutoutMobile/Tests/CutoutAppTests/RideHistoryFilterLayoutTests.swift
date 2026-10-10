import SwiftUI
import XCTest

@testable import CutoutApp

#if os(macOS)
    import AppKit

    @MainActor
    final class RideHistoryFilterLayoutTests: XCTestCase {
        func testChangingAndClearingFiltersKeepsControlStripHeight() throws {
            for width: CGFloat in [288, 358, 448] {
                for textSize in [DynamicTypeSize.large, .accessibility3] {
                    let baseline = try fittedSize(
                        width: width, textSize: textSize,
                        date: "Last 30 Days", vehicle: "All vehicles", filtered: false,
                        includeShortRides: false
                    )
                    for (date, vehicle) in [
                        ("All time", "All vehicles"),
                        ("Last 30 Days", "NOSFET Aero"),
                        ("Last 30 Days", "A wheel with a much longer saved name"),
                    ] {
                        let filtered = try fittedSize(
                            width: width, textSize: textSize,
                            date: date, vehicle: vehicle, filtered: true,
                            includeShortRides: false
                        )
                        XCTAssertEqual(filtered.width, baseline.width, accuracy: 0.5)
                        XCTAssertEqual(
                            filtered.height, baseline.height, accuracy: 0.5,
                            "Filter values and clear availability must not move the route below at width \(width), \(textSize)"
                        )
                    }
                    let shortRides = try fittedSize(
                        width: width, textSize: textSize,
                        date: "Last 30 Days", vehicle: "All vehicles", filtered: true,
                        includeShortRides: true
                    )
                    XCTAssertEqual(shortRides.width, baseline.width, accuracy: 0.5)
                    XCTAssertEqual(shortRides.height, baseline.height, accuracy: 0.5)
                }
            }
        }

        private func fittedSize(
            width: CGFloat, textSize: DynamicTypeSize,
            date: String, vehicle: String, filtered: Bool, includeShortRides: Bool
        ) throws -> NSSize {
            let view = RideMapHistoryFilterBar(
                dateTitle: date,
                vehicleTitle: vehicle,
                vehicleOptions: [],
                hasActiveFilters: filtered,
                includeShortRides: includeShortRides,
                setDateFilter: { _ in },
                setVehicleFilter: { _ in },
                setIncludeShortRides: { _ in },
                clearFilters: {}
            )
            .environment(\.dynamicTypeSize, textSize)
            .environment(\.colorScheme, .dark)
            .frame(width: width)
            let host = NSHostingView(rootView: view)
            let size = host.fittingSize
            XCTAssertGreaterThanOrEqual(size.height, 44)
            try writeReviewPreview(host: host, size: size, textSize: textSize, date: date, vehicle: vehicle)
            return size
        }

        // Optional artifacts render the actual SwiftUI controls, without phone or UI automation.
        private func writeReviewPreview(
            host: NSView, size: NSSize, textSize: DynamicTypeSize, date: String, vehicle: String
        ) throws {
            guard size.width == 358,
                let directory = ProcessInfo.processInfo.environment["CUTOUT_HISTORY_FILTER_PREVIEW_DIR"]
            else { return }
            host.appearance = NSAppearance(named: .darkAqua)
            host.frame = NSRect(origin: .zero, size: size)
            host.layoutSubtreeIfNeeded()
            let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
            host.cacheDisplay(in: host.bounds, to: bitmap)
            let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
            let output = URL(fileURLWithPath: directory, isDirectory: true)
            try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
            try png.write(to: output.appendingPathComponent("\(textSize)-\(date)-\(vehicle).png"))
        }
    }
#endif
