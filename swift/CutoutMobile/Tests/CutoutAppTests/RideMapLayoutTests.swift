import CutoutMobile
import CutoutMobileFFI
import MapKit
import SwiftUI
import XCTest

@testable import CutoutApp

#if os(macOS)
    import AppKit

    @MainActor
    final class RideMapLayoutTests: XCTestCase {
        func testEmptyListeningHistoryDoesNotAddASectionButFailuresAndDeletionRemain() throws {
            let absent = try renderedSize(
                historySummary(state: nil), width: 358, textSize: .large, name: "history-music-absent"
            )
            for state in [MobileMusicHistoryStateDto.missing, .disabled, .deleted, .humanReadable, .redacted] {
                let empty = try renderedSize(
                    historySummary(state: state), width: 358, textSize: .large, name: "history-music-empty"
                )
                XCTAssertEqual(empty.height, absent.height, accuracy: 0.5, "Empty \(state) history needs no section")
            }
            let failure = try renderedSize(
                historySummary(state: .disabled, error: .storageError("fixture failure")),
                width: 358, textSize: .large, name: "history-music-failure"
            )
            XCTAssertGreaterThan(failure.height, absent.height)
            let deletion = try renderedSize(
                historySummary(state: .redacted, canForget: true),
                width: 358, textSize: .large, name: "history-music-deletion"
            )
            XCTAssertGreaterThan(deletion.height, absent.height)
        }

        private func historySummary(
            state: MobileMusicHistoryStateDto?, error: MobileRideMapError? = nil, canForget: Bool = false
        ) -> some View {
            RideMapHistoryDetailSummary(
                distance: "75 mi", duration: "5 hr", averageSpeed: "15 mph",
                recordedAt: "Oct 4", vehicle: "NOSFET Aero", telemetryState: .associatedNoTelemetry,
                displayPointCount: 2, recordedPointCount: 2, pointsTruncated: false,
                segmentCount: 1, segments: [], segmentsOmittedByBudget: false, canonicalBackgroundGapCount: 0,
                musicTimeline: [], musicTimelineUnavailable: error != nil, musicHistoryState: state,
                musicHistoryError: error, musicHistoryCanForget: canForget, forgetMusicHistory: { true },
                state: .ready, loadRoutePreview: {}, shareText: "Ride",
                mapPosition: .constant(.automatic), isApplyingCamera: .constant(false)
            )
        }

        func testRideRowSelectionDoesNotChangeLayoutAtNarrowAndAccessibleSizes() throws {
            for width: CGFloat in [288, 358, 448] {
                for textSize in [DynamicTypeSize.large, .accessibility3] {
                    let baseline = try renderedSize(
                        row(selected: false), width: width, textSize: textSize,
                        name: "row-default"
                    )
                    let selected = try renderedSize(
                        row(selected: true), width: width, textSize: textSize,
                        name: "row-selected"
                    )
                    XCTAssertEqual(baseline.width, width, accuracy: 0.5)
                    XCTAssertEqual(selected.height, baseline.height, accuracy: 0.5)
                    XCTAssertGreaterThanOrEqual(baseline.height, 44)
                }
            }
        }

        func testDetailHeaderAndMetricsUseMoreVerticalSpaceForAccessibilityText() throws {
            for width: CGFloat in [288, 358] {
                let header = RideMapHistoryDetailHeader(close: {})
                let metrics = RideMapDetailMetrics(
                    distance: "75.0 mi", duration: "5 hr, 36 min", averageSpeed: "13.4 mph"
                )
                let normalHeader = try renderedSize(header, width: width, textSize: .large, name: "detail-header")
                let accessibleHeader = try renderedSize(
                    header, width: width, textSize: .accessibility3, name: "detail-header"
                )
                let normalMetrics = try renderedSize(metrics, width: width, textSize: .large, name: "detail-metrics")
                let accessibleMetrics = try renderedSize(
                    metrics, width: width, textSize: .accessibility3, name: "detail-metrics"
                )
                XCTAssertGreaterThan(accessibleHeader.height, normalHeader.height)
                XCTAssertGreaterThan(accessibleMetrics.height, normalMetrics.height)
            }
        }

        func testMapHeaderFitsInsideItsCompactSlot() throws {
            let shell = PevAppShell(sectionTitle: "Map") { Color.clear.frame(height: 1) }
            _ = try renderedSize(shell.frame(height: 64), width: 390, textSize: .large, name: "map-shell")
        }

        func testHistorySearchKeepsItsSizeWhenQueryIsCleared() throws {
            for textSize in [DynamicTypeSize.large, .accessibility3] {
                let empty = try renderedSize(
                    RideMapHistorySearchField(searchText: .constant("")),
                    width: 288, textSize: textSize, name: "search-empty"
                )
                let searched = try renderedSize(
                    RideMapHistorySearchField(searchText: .constant("NOSFET Aero October 4")),
                    width: 288, textSize: textSize, name: "search-query"
                )
                XCTAssertEqual(empty.height, searched.height, accuracy: 0.5)
                XCTAssertGreaterThanOrEqual(empty.height, 44)
            }
        }

        func testRouteNoticesOnlyOccupySpaceForPersistentRouteConditions() throws {
            for textSize in [DynamicTypeSize.large, .accessibility3] {
                let noNotice = try renderedSize(
                    routeNotices(recordedPointCount: 75, backgroundGaps: 0, omittedSegments: false),
                    width: 288, textSize: textSize, name: "route-no-notice"
                )
                let emptyRide = try renderedSize(
                    routeNotices(recordedPointCount: 0, backgroundGaps: 1, omittedSegments: false),
                    width: 288, textSize: textSize, name: "route-empty-no-notice"
                )
                let interrupted = try renderedSize(
                    routeNotices(recordedPointCount: 75, backgroundGaps: 1, omittedSegments: false),
                    width: 288, textSize: textSize, name: "route-interrupted"
                )
                let omitted = try renderedSize(
                    routeNotices(recordedPointCount: 75, backgroundGaps: 0, omittedSegments: true),
                    width: 288, textSize: textSize, name: "route-preview-omitted"
                )
                XCTAssertEqual(noNotice.height, 0, accuracy: 0.5)
                XCTAssertEqual(emptyRide.height, 0, accuracy: 0.5)
                XCTAssertGreaterThan(interrupted.height, noNotice.height)
                XCTAssertGreaterThan(omitted.height, noNotice.height)
            }
        }

        private func routeNotices(
            recordedPointCount: UInt64, backgroundGaps: UInt64, omittedSegments: Bool
        ) -> some View {
            RideMapRouteTruthView(
                displayedPointCount: 0, recordedPointCount: recordedPointCount,
                rustSegmentCount: 1, showsRecordedBounds: true,
                segmentsOmittedByBudget: omittedSegments,
                canonicalBackgroundGapCount: backgroundGaps
            )
        }

        private func row(selected: Bool) -> some View {
            RideMapHistoryRow(
                rideID: "preview-ride", isSelected: selected,
                title: "Oct 4, 2026 at 1:08 PM",
                vehicle: "NOSFET Aero with a longer saved name",
                distance: "75.0 mi", duration: "5 hr, 36 min", select: {}
            )
        }

        private func renderedSize<Content: View>(
            _ content: Content, width: CGFloat, textSize: DynamicTypeSize, name: String
        ) throws -> NSSize {
            let view =
                content
                .environment(\.dynamicTypeSize, textSize)
                .environment(\.colorScheme, .dark)
                .frame(width: width)
                .background(PevColors.pageBackground)
            let host = NSHostingView(rootView: view)
            let size = host.fittingSize
            XCTAssertLessThanOrEqual(size.width, width + 0.5)
            guard let directory = ProcessInfo.processInfo.environment["CUTOUT_MAP_LAYOUT_PREVIEW_DIR"] else {
                return size
            }
            host.appearance = NSAppearance(named: .darkAqua)
            host.frame = NSRect(origin: .zero, size: size)
            host.layoutSubtreeIfNeeded()
            let bitmap = try XCTUnwrap(host.bitmapImageRepForCachingDisplay(in: host.bounds))
            host.cacheDisplay(in: host.bounds, to: bitmap)
            let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
            let output = URL(fileURLWithPath: directory, isDirectory: true)
            try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
            try png.write(to: output.appendingPathComponent("\(name)-\(width)-\(textSize).png"))
            return size
        }
    }
#endif
