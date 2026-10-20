import CutoutMobile
import CutoutMobileFFI
import SwiftUI
import XCTest

@testable import CutoutApp

#if os(macOS)
    import AppKit
#endif

@MainActor
final class EucRideInstrumentTests: XCTestCase {
    func testOrdinaryRideFixturesDoNotCreateSystemLiveActivities() {
        for fixture in [
            CutoutUITestSessionFixture.euc, .dynamicEuc, .staleEuc, .vesc, .unknownDevice, .bluetoothUnavailable,
        ] {
            XCTAssertTrue(fixture.makeLiveActivityManager() is CutoutUITestLiveActivityManager)
        }
        for fixture in [
            CutoutUITestSessionFixture.autoVescLiveActivity, .autoDynamicVescLiveActivity,
            .autoCriticalVescLiveActivity, .autoUnavailableVescLiveActivity, .autoStaleVescLiveActivity,
        ] {
            XCTAssertTrue(fixture.makeLiveActivityManager() is LiveActivityRideActivityKitManager)
        }
    }

    func testIsolatedLiveActivityPreservesLifecycleReceiptsWithoutSystemEffects() async throws {
        let manager = CutoutUITestSessionFixture.euc.makeLiveActivityManager()
        let snapshot = LiveActivityRideSnapshot(
            identity: .model(.aero),
            rideState: EucRideScreenState(phase: .live, displayState: RideDisplayState())
        )
        let identity = LiveActivityRideSessionIdentity(platformIdentifier: "fixture", sessionID: "ride")
        let started = try await manager.start(
            snapshot: snapshot, rideSessionIdentity: identity, staleAfterMilliseconds: 1_000,
            freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(started, .started(activityID: "ui-test-ride"))
        let adopted = try await manager.start(
            snapshot: snapshot, rideSessionIdentity: identity, staleAfterMilliseconds: 1_000,
            freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(adopted, .adopted(activityID: "ui-test-ride"))
        let updated = try await manager.update(
            snapshot: snapshot, staleAfterMilliseconds: 1_000, freshnessExpiresAt: .distantFuture
        )
        XCTAssertEqual(updated.activityID, "ui-test-ride")
        let ended = try await manager.end(reason: .disconnected)
        XCTAssertEqual(ended.activityIDs, ["ui-test-ride"])
        let endedAgain = try await manager.end(reason: .disconnected)
        XCTAssertTrue(endedAgain.activityIDs.isEmpty)
        do {
            _ = try await manager.update(
                snapshot: snapshot, staleAfterMilliseconds: 1_000, freshnessExpiresAt: .distantFuture
            )
            XCTFail("An ended fixture must not acknowledge another activity update")
        } catch {
            XCTAssertEqual(error as? LiveActivityRideLifecycleError, .activityUnavailable)
        }
    }

    func testHeadroomClearsWhenTelemetryAgesWithoutAnotherSample() throws {
        let state = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000), operatingState: .riding, pwm: DutyCycle(permille: 680)
                )
            )
        )
        let fresh = view(state: state, now: 1_000)
        XCTAssertEqual(try XCTUnwrap(fresh.safetyBars.first?.progress), 0.32, accuracy: 0.001)
        XCTAssertEqual(fresh.safetyBars.first?.metricValue.displayText, "32%")
        XCTAssertTrue(view(state: state, now: 100_000).safetyBars.isEmpty)
        XCTAssertTrue(view(state: nil, now: 1_000).safetyBars.isEmpty)
    }

    func testDisconnectedTelemetryDoesNotFillTheGauge() {
        let state = EucRideScreenState(
            phase: .scanning,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000), operatingState: .riding, pwm: DutyCycle(permille: 680)
                )
            )
        )
        XCTAssertTrue(view(state: state, now: 1_000).safetyBars.isEmpty)
    }

    func testUnavailableGaugeNeverFillsAndZeroRemainsAvailable() {
        for value in [PevDashboardMetricValue.unavailable, .status(display: "N/A", accessibility: "Not applicable")] {
            XCTAssertNil(gauge(value: value, progress: 0.5).displayedProgress)
        }
        XCTAssertEqual(gauge(value: .available(display: "0%", accessibility: "0%"), progress: 0).displayedProgress, 0)
        XCTAssertEqual(
            gauge(value: .available(display: "100%", accessibility: "100%"), progress: 1).displayedProgress, 1)
    }

    #if os(macOS)
        func testPopulatedDashboardFitsA600PointViewport() throws {
            for height: CGFloat in [600, 700] {
                let image = try XCTUnwrap(renderedDashboard(height: height, playerFrame: nil))
                let fullImage = CGRect(x: 0, y: 0, width: image.width, height: image.height)
                let pixels = try normalizedPixels(in: image, region: fullImage)
                let lastForegroundRow = try XCTUnwrap(
                    lastForegroundRow(in: pixels, width: image.width, height: image.height)
                )
                XCTAssertLessThan(lastForegroundRow, 1_000, "Ride content should end before the 500pt mark at \(height)pt")
            }
        }

        func testDashboardLeavesCompactPlayerFrameUncovered() throws {
            let playerFrame = CGRect(x: 0, y: 510, width: 393, height: 84)
            let image = try XCTUnwrap(renderedDashboard(height: 700, playerFrame: playerFrame))
            let fullImage = CGRect(x: 0, y: 0, width: image.width, height: image.height)
            let pixels = try normalizedPixels(in: image, region: fullImage)
            let lastForegroundRow = try XCTUnwrap(
                lastForegroundRow(in: pixels, width: image.width, height: image.height)
            )
            XCTAssertLessThan(
                lastForegroundRow, Int(playerFrame.minY * 2),
                "The dashboard must end before the compact player begins at \(playerFrame.minY)pt"
            )
        }

        func testDashboardInstrumentLayoutIsStableAcrossPlayerFramesAndViewportHeights() throws {
            let viewportHeights: [CGFloat] = [600, 650, 700]
            let playerFrames: [CGRect?] = [
                nil,
                CGRect(x: 0, y: 510, width: 393, height: 74),
                CGRect(x: 0, y: 560, width: 393, height: 92),
            ]

            let baseline = try XCTUnwrap(renderedDashboard(height: 600, playerFrame: nil))
            let commonDashboardRegion = CGRect(x: 0, y: 0, width: baseline.width, height: 1_180)
            let baselinePixels = try normalizedPixels(in: baseline, region: commonDashboardRegion)
            let baselinePath = pngArtifactPath(for: baseline, name: "cutout-ride-layout-baseline.png")

            for height in viewportHeights {
                for (frameIndex, playerFrame) in playerFrames.enumerated() {
                    let image = try XCTUnwrap(renderedDashboard(height: height, playerFrame: playerFrame))
                    let pixels = try normalizedPixels(in: image, region: commonDashboardRegion)
                    guard pixels != baselinePixels else { continue }
                    let imagePath = pngArtifactPath(
                        for: image, name: "cutout-ride-layout-\(Int(height))-frame-\(frameIndex).png"
                    )
                    XCTFail(
                        "Ride layout pixels differ at \(height)pt with player frame \(String(describing: playerFrame)): "
                            + pixelDifference(baselinePixels, pixels, width: Int(commonDashboardRegion.width))
                            + "; baseline=\(baselinePath ?? "unavailable"), actual=\(imagePath ?? "unavailable")"
                    )
                }
            }
        }

        func testBatteryStripKeepsItsSizeWhenTelemetryBecomesUnavailable() {
            func size(progress: Double?) -> CGSize {
                let host = NSHostingView(
                    rootView: EucInstrumentReading(
                        label: "Battery", value: .available(display: "78", accessibility: "78"),
                        unit: "%", pointSize: 52, identifier: "battery", showsProgress: true, progress: progress
                    ).environment(\.dynamicTypeSize, .large).frame(width: 180))
                return host.fittingSize
            }
            XCTAssertEqual(size(progress: 0.78), size(progress: nil))
        }

        private func renderedDashboard(height: CGFloat, playerFrame: CGRect?) -> CGImage? {
            let dashboard = EucRideInstruments(
                title: "NOSFET Aero", subtitle: "Connected", statusTone: .eucRide,
                captureStatusText: nil,
                speed: .available(value: "0.0", unit: "mph", freshness: .fresh, severity: .nominal),
                headroom: PevSafetyBar(
                    id: .pwmHeadroom, label: "PWM headroom",
                    metricValue: .available(display: "32%", accessibility: "32%"), progress: 0.32,
                    accent: .yellow
                ),
                headroomNumber: "32", batteryProgress: 0.94,
                tiles: [
                    PevDashboardTile(
                        kind: .batteryLevel, label: "Battery",
                        metricValue: .available(display: "94", accessibility: "94"),
                        unit: "%", detail: "", accent: .yellow
                    ),
                    PevDashboardTile(
                        kind: .packVoltage, label: "Voltage",
                        metricValue: .available(display: "123.6", accessibility: "123.6"),
                        unit: "V", detail: "", accent: .cyan
                    ),
                    PevDashboardTile(
                        kind: .power, label: "Power",
                        metricValue: .available(display: "0.00", accessibility: "0.00"),
                        unit: "kW", detail: "", accent: .cyan
                    ),
                    PevDashboardTile(
                        kind: .thermal, label: "Temperature",
                        metricValue: .available(display: "33", accessibility: "33"),
                        unit: "°C", detail: "", accent: .yellow
                    ),
                ],
                gpsSpeed: PevDashboardTile(
                    kind: .gpsSpeed, label: "GPS speed",
                    metricValue: .available(display: "0.0", accessibility: "0.0"),
                    unit: "mph", detail: "", accent: .cyan
                ),
                disconnect: {}
            )
            let renderer = ImageRenderer(
                content: dashboard
                    .environment(\.musicCompactPlayerFrame, playerFrame)
                    .environment(\.dynamicTypeSize, .large)
                    .environment(\.colorScheme, .dark)
                    .frame(width: 393, height: height)
            )
            renderer.scale = 2
            return renderer.cgImage
        }

        private func normalizedPixels(in image: CGImage, region: CGRect) throws -> Data {
            let cropped = try XCTUnwrap(image.cropping(to: region))
            let width = Int(region.width)
            let height = Int(region.height)
            let bytesPerRow = width * 4
            let context = try XCTUnwrap(
                CGContext(
                    data: nil, width: width, height: height, bitsPerComponent: 8,
                    bytesPerRow: bytesPerRow, space: CGColorSpaceCreateDeviceRGB(),
                    bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue
                        | CGImageAlphaInfo.premultipliedLast.rawValue
                )
            )
            context.draw(cropped, in: CGRect(x: 0, y: 0, width: width, height: height))
            return Data(bytes: try XCTUnwrap(context.data), count: bytesPerRow * height)
        }

        private func lastForegroundRow(in pixels: Data, width: Int, height: Int) -> Int? {
            pixels.withUnsafeBytes { buffer in
                guard let bytes = buffer.bindMemory(to: UInt8.self).baseAddress else { return nil }
                let backgroundOffset = 0
                let background = (bytes[backgroundOffset], bytes[backgroundOffset + 1], bytes[backgroundOffset + 2])
                for y in stride(from: height - 1, through: 0, by: -1) {
                    for x in 0 ..< width {
                        let offset = (y * width + x) * 4
                        if abs(Int(bytes[offset]) - Int(background.0)) > 10
                            || abs(Int(bytes[offset + 1]) - Int(background.1)) > 10
                            || abs(Int(bytes[offset + 2]) - Int(background.2)) > 10
                        {
                            return y
                        }
                    }
                }
                return nil
            }
        }

        private func pixelDifference(_ lhs: Data, _ rhs: Data, width: Int) -> String {
            let left = [UInt8](lhs)
            let right = [UInt8](rhs)
            guard left.count == right.count, width > 0 else {
                return "different byte lengths baseline=\(left.count) actual=\(right.count)"
            }
            var changedPixels = 0
            var minX = width
            var maxX = -1
            var minY = left.count / (width * 4)
            var maxY = -1
            var firstPixel: (x: Int, y: Int, baseline: [UInt8], actual: [UInt8])?
            for offset in stride(from: 0, to: left.count, by: 4) {
                guard left[offset] != right[offset] || left[offset + 1] != right[offset + 1]
                    || left[offset + 2] != right[offset + 2] || left[offset + 3] != right[offset + 3]
                else { continue }
                let pixel = offset / 4
                let x = pixel % width
                let y = pixel / width
                changedPixels += 1
                minX = min(minX, x)
                maxX = max(maxX, x)
                minY = min(minY, y)
                maxY = max(maxY, y)
                if firstPixel == nil {
                    firstPixel = (x, y, Array(left[offset ..< offset + 4]), Array(right[offset ..< offset + 4]))
                }
            }
            guard let firstPixel else { return "no differing RGBA pixels" }
            return "changedPixels=\(changedPixels), bounds=(\(minX),\(minY))-(\(maxX),\(maxY)), "
                + "first=(\(firstPixel.x),\(firstPixel.y)) baselineRGBA=\(firstPixel.baseline) actualRGBA=\(firstPixel.actual)"
        }

        private func pngArtifactPath(for image: CGImage, name: String) -> String? {
            let bitmap = NSBitmapImageRep(cgImage: image)
            guard let png = bitmap.representation(using: .png, properties: [:]) else { return nil }
            let url = FileManager.default.temporaryDirectory.appendingPathComponent(name)
            do {
                try png.write(to: url)
                return url.path
            } catch {
                return nil
            }
        }
    #endif

    private func gauge(value: PevDashboardMetricValue, progress: Double) -> EucHeadroomGauge {
        EucHeadroomGauge(
            headroom: PevSafetyBar(
                id: .pwmHeadroom, label: "PWM headroom", metricValue: value, progress: progress, accent: .yellow)
        )
    }

    private func view(state: EucRideScreenState?, now: UInt64) -> EucRideScreenView {
        EucRideScreenView(
            rideState: state, rideTitle: "NOSFET Aero", now: MonotonicMilliseconds(now),
            captureStatusText: nil, connectionStatusText: "Connected",
            phoneLocationReadback: PhoneLocationReadback(
                snapshot: MobilePhoneLocationSnapshotDto(latestSample: nil, gpsSpeed: nil)
            )
        )
    }
}
