#if DEBUG && os(iOS) && targetEnvironment(simulator)
    import CutoutMobile
    import CutoutMobileFFI
    import Foundation

    /// Deterministic, local-only presentation data for the five website screenshots.
    /// The opt-in is deliberately inert on devices, release builds, and ordinary UI tests.
    enum CutoutWebsiteScreenshotFixture {
        enum Screen: String {
            case eucMain = "euc-main"
            case rgb
            case tune
            case rideTracking = "ride-tracking"
            case chargingMap = "charging-map"
        }

        static var screen: Screen? {
            guard let index = CommandLine.arguments.firstIndex(of: "-CUTOUT_WEBSITE_SCREENSHOT"),
                CommandLine.arguments.indices.contains(index + 1),
                let screen = Screen(rawValue: CommandLine.arguments[index + 1]),
                let fixture = CutoutUITestSessionFixture(arguments: CommandLine.arguments),
                ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"].flatMap(UUID.init(uuidString:))
                    != nil
            else { return nil }
            switch screen {
            case .eucMain, .rgb, .tune, .rideTracking, .chargingMap:
                guard fixture.isEuc else { return nil }
            }
            return screen
        }

        static var isChargingMapScreenshot: Bool { screen == .chargingMap }

        static var databaseIdentifier: UUID? {
            guard screen != nil,
                let rawID = ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"]
            else { return nil }
            return UUID(uuidString: rawID)
        }

        static func eucRideTelemetry(at now: MonotonicMilliseconds? = nil) -> TelemetrySnapshot? {
            guard screen == .eucMain else { return nil }
            return TelemetrySnapshot(
                at: now,
                speed: Speed(value: 10_700), speedSource: .reported, speedQuality: .known,
                operatingState: .riding, voltage: Voltage(value: 120_000),
                batteryCurrent: BatteryCurrent.currentOut(milliamps: 7_000),
                power: Power(value: 840_000), powerFlow: .discharge,
                controllerTemperature: Temperature(value: 38_000),
                pwm: DutyCycle(permille: 420), batteryLevelReported: BatteryLevel(value: 78)
            )
        }

        static func phoneLocationReadback(at now: MonotonicMilliseconds) -> PhoneLocationReadback? {
            guard screen == .eucMain else { return nil }
            let sourceTime = Date().timeIntervalSince1970
            let state = MobilePhoneLocationState()
            _ = state.ingest(
                sample: MobilePhoneLocationSampleDto(
                    wallClockUnixMs: UInt64(sourceTime * 1_000),
                    sourceTimestampUnixSeconds: sourceTime,
                    latitudeDegrees: 39.75490,
                    longitudeDegrees: -105.00875,
                    altitudeMeters: 1_600,
                    horizontalAccuracyMeters: 4,
                    verticalAccuracyMeters: 6,
                    speedMetersPerSecond: 10.7,
                    speedAccuracyMetersPerSecond: 0.2,
                    courseDegrees: 40,
                    courseAccuracyDegrees: 3
                )
            )
            return PhoneLocationReadback(snapshot: state.currentSnapshot(), receivedAt: now)
        }

        static func runIfRequested(database: RideDatabaseHandle) async throws {
            switch screen {
            case .chargingMap:
                try await seedFictionalChargingSpots(database: database)
            case .eucMain, .rgb, .tune, .rideTracking, .none:
                break
            }
        }

        static func prepareRideTrackingIfRequested(state: MobileRideMapState) async throws -> Task<Void, Never>? {
            guard screen == .rideTracking else { return nil }
            return try await Task.detached(priority: .utility) {
                try prepareRideTracking(state: state)
            }.value
        }

        private static func prepareRideTracking(state: MobileRideMapState) throws -> Task<Void, Never> {
            let nowWall = UInt64(Date().timeIntervalSince1970 * 1_000)
            let nowMonotonic = UInt64(ProcessInfo.processInfo.systemUptime * 1_000)
            let duration: UInt64 = 300_000
            guard nowWall > duration, nowMonotonic > duration else {
                throw CocoaError(.coderInvalidValue)
            }
            let startedWall = nowWall - duration
            let startedMonotonic = nowMonotonic - duration
            let route = try DenverRoute()
            let started = try state.startGpsOnly(atMs: startedMonotonic)
            guard let recording = started.recordingToken else { throw CocoaError(.coderInvalidValue) }
            let speedMetersPerSecond = route.totalDistanceMeters / (Double(duration) / 1_000)

            var lastMonotonic = startedMonotonic
            for elapsed in stride(from: UInt64(0), through: duration, by: 1_000) {
                let monotonic = startedMonotonic + elapsed
                let wall = startedWall + elapsed
                try record(
                    route: route,
                    elapsed: elapsed,
                    startedWall: startedWall,
                    receiptMonotonic: monotonic,
                    receiptWall: wall,
                    speedMetersPerSecond: speedMetersPerSecond,
                    state: state,
                    recording: recording
                )
                lastMonotonic = monotonic
            }

            // Setup and navigation consume real time. Fill that synthetic interval with
            // consecutive route evidence before publishing the model to the UI.
            let catchUpDeadline = Date().addingTimeInterval(10)
            while true {
                let current = monotonicMillisecondsNow()
                guard current >= lastMonotonic, current - lastMonotonic <= 10_000,
                    Date() < catchUpDeadline
                else { throw CocoaError(.coderInvalidValue) }
                guard current - lastMonotonic > 1_000 else { break }
                let next = lastMonotonic + 1_000
                try record(
                    route: route,
                    elapsed: next - startedMonotonic,
                    startedWall: startedWall,
                    receiptMonotonic: next,
                    receiptWall: startedWall + (next - startedMonotonic),
                    speedMetersPerSecond: speedMetersPerSecond,
                    state: state,
                    recording: recording
                )
                lastMonotonic = next
            }

            let snapshot = state.currentSnapshot(atMs: monotonicMillisecondsNow())
            guard let snapshot, snapshot.rideID == recording.rideId, snapshot.state == .active,
                snapshot.summary.pointCount >= 301,
                snapshot.summary.distanceMeters > 3_000,
                snapshot.summary.durationMilliseconds >= duration,
                case .complete = snapshot.summary.elapsedTime,
                case .complete = snapshot.summary.movingTime
            else { throw CocoaError(.coderInvalidValue) }

            let feedStartMonotonic = lastMonotonic
            return Task.detached(priority: .utility) {
                var cursor = feedStartMonotonic
                while !Task.isCancelled {
                    do { try await Task.sleep(for: .seconds(1)) } catch { return }
                    guard !Task.isCancelled else { return }
                    let current = monotonicMillisecondsNow()
                    guard current >= cursor, current - cursor <= 10_000 else { return }
                    while current - cursor >= 1_000 {
                        guard !Task.isCancelled else { return }
                        let next = cursor + 1_000
                        do {
                            try record(
                                route: route,
                                elapsed: next - startedMonotonic,
                                startedWall: startedWall,
                                receiptMonotonic: next,
                                receiptWall: startedWall + (next - startedMonotonic),
                                speedMetersPerSecond: speedMetersPerSecond,
                                state: state,
                                recording: recording
                            )
                        } catch { return }
                        cursor = next
                    }
                }
            }
        }

        private struct Coordinate: Sendable {
            let latitude: Double
            let longitude: Double
        }

        private struct DenverRoute: Sendable {
            let anchors: [Coordinate] = [
                Coordinate(latitude: 39.72860, longitude: -105.01659),
                Coordinate(latitude: 39.72942, longitude: -105.01714),
                Coordinate(latitude: 39.73162, longitude: -105.01741),
                Coordinate(latitude: 39.73504, longitude: -105.01738),
                Coordinate(latitude: 39.73768, longitude: -105.01824),
                Coordinate(latitude: 39.73898, longitude: -105.01826),
                Coordinate(latitude: 39.73944, longitude: -105.01795),
                Coordinate(latitude: 39.74299, longitude: -105.01669),
                Coordinate(latitude: 39.74285, longitude: -105.01612),
                Coordinate(latitude: 39.74471, longitude: -105.01532),
                Coordinate(latitude: 39.74694, longitude: -105.01523),
                Coordinate(latitude: 39.74949, longitude: -105.01485),
                Coordinate(latitude: 39.74988, longitude: -105.01456),
                Coordinate(latitude: 39.75022, longitude: -105.01365),
                Coordinate(latitude: 39.75135, longitude: -105.01201),
                Coordinate(latitude: 39.75393, longitude: -105.00885),
                Coordinate(latitude: 39.75436, longitude: -105.00937),
                Coordinate(latitude: 39.75490, longitude: -105.00875),
            ]
            let cumulativeDistanceMeters: [Double]
            let totalDistanceMeters: Double

            init() throws {
                var cumulative = [0.0]
                for index in 1..<anchors.count {
                    cumulative.append(
                        cumulative[index - 1] + Self.distanceMeters(from: anchors[index - 1], to: anchors[index]))
                }
                guard let total = cumulative.last, total > 0 else { throw CocoaError(.coderInvalidValue) }
                cumulativeDistanceMeters = cumulative
                totalDistanceMeters = total
            }

            func coordinate(at elapsed: UInt64) -> Coordinate {
                let distance = totalDistanceMeters * (Double(elapsed % 600_000) / 300_000)
                let traveled = distance <= totalDistanceMeters ? distance : 2 * totalDistanceMeters - distance
                let endIndex = min(
                    cumulativeDistanceMeters.firstIndex(where: { $0 >= traveled }) ?? anchors.count - 1,
                    anchors.count - 1
                )
                let startIndex = max(0, endIndex - 1)
                let segmentDistance = cumulativeDistanceMeters[endIndex] - cumulativeDistanceMeters[startIndex]
                let fraction =
                    segmentDistance > 0 ? (traveled - cumulativeDistanceMeters[startIndex]) / segmentDistance : 0
                let start = anchors[startIndex]
                let end = anchors[endIndex]
                return Coordinate(
                    latitude: start.latitude + (end.latitude - start.latitude) * fraction,
                    longitude: start.longitude + (end.longitude - start.longitude) * fraction
                )
            }

            private static func distanceMeters(from first: Coordinate, to second: Coordinate) -> Double {
                let radiansPerDegree = Double.pi / 180
                let latitudeDelta = (second.latitude - first.latitude) * radiansPerDegree
                let longitudeDelta = (second.longitude - first.longitude) * radiansPerDegree
                let firstLatitude = first.latitude * radiansPerDegree
                let secondLatitude = second.latitude * radiansPerDegree
                let haversine =
                    sin(latitudeDelta / 2) * sin(latitudeDelta / 2)
                    + cos(firstLatitude) * cos(secondLatitude) * sin(longitudeDelta / 2) * sin(longitudeDelta / 2)
                return 6_371_000 * 2 * atan2(sqrt(haversine), sqrt(1 - haversine))
            }
        }

        private static func record(
            route: DenverRoute,
            elapsed: UInt64,
            startedWall: UInt64,
            receiptMonotonic: UInt64,
            receiptWall: UInt64,
            speedMetersPerSecond: Double,
            state: MobileRideMapState,
            recording: MobileRideMapRecordingTokenDto
        ) throws {
            let coordinate = route.coordinate(at: elapsed)
            let sample = MobilePhoneLocationSampleDto(
                wallClockUnixMs: startedWall + elapsed,
                sourceTimestampUnixSeconds: nil,
                latitudeDegrees: coordinate.latitude,
                longitudeDegrees: coordinate.longitude,
                altitudeMeters: 1_600,
                horizontalAccuracyMeters: 4,
                verticalAccuracyMeters: 6,
                speedMetersPerSecond: speedMetersPerSecond,
                speedAccuracyMetersPerSecond: 0.2,
                courseDegrees: nil,
                courseAccuracyDegrees: nil
            )
            let outcomes = try state.ingestLocationCallbackOutcomes(
                recordingToken: recording,
                receiptMonotonicMs: receiptMonotonic,
                receiptWallClockUnixMs: receiptWall,
                samples: [sample]
            )
            let accepted = outcomes.filter {
                if case .accepted = $0.decision { return true }
                return false
            }
            guard accepted.count == 1,
                let acceptedRequest = accepted[0].requestID,
                accepted[0].rideID == recording.rideId,
                outcomes.allSatisfy({ outcome in
                    guard outcome.requestID == acceptedRequest else { return false }
                    if case .pending = outcome.decision { return true }
                    if case .accepted = outcome.decision { return outcome.requestID == acceptedRequest }
                    return false
                })
            else { throw CocoaError(.coderInvalidValue) }
        }

        private static func monotonicMillisecondsNow() -> UInt64 {
            UInt64(ProcessInfo.processInfo.systemUptime * 1_000)
        }

        static func seedFictionalChargingSpots(database: RideDatabaseHandle) async throws {
            let bounds = MobileGeoBoundsDto(
                minimumLatitudeDegrees: 39.72,
                maximumLatitudeDegrees: 39.76,
                minimumLongitudeDegrees: -105.01,
                maximumLongitudeDegrees: -104.97
            )
            let existing = try await database.chargingSpotsInBounds(
                bounds: bounds,
                selection: MobileChargingFilterDto(cost: nil, connector: nil, voltageVolts: nil, venue: nil),
                cursor: nil,
                limit: 1
            )
            guard existing.spots.isEmpty else { return }

            let inputs = [
                MobileChargingInputDto(
                    coordinate: MobileMapCoordinateDto(latitudeDegrees: 39.7392, longitudeDegrees: -104.9903),
                    accessNote: "Fictional screenshot data. No real charging access is offered.",
                    cost: .free,
                    name: "Juniper Corner Library",
                    venue: .other,
                    hours: "09:00–18:00",
                    terms: nil,
                    price: nil,
                    supplies: [MobileChargingSupplyDto(voltageVolts: 120, connectors: [.nema515], outletCount: 2)]
                ),
                MobileChargingInputDto(
                    coordinate: MobileMapCoordinateDto(latitudeDegrees: 39.744, longitudeDegrees: -104.986),
                    accessNote: "Fictional screenshot data. This venue does not exist.",
                    cost: .paid,
                    name: "Bluebird Coffee Room",
                    venue: .cafe,
                    hours: nil,
                    terms: .purchaseRequired,
                    price: nil,
                    supplies: [MobileChargingSupplyDto(voltageVolts: nil, connectors: [], outletCount: nil)]
                ),
                MobileChargingInputDto(
                    coordinate: MobileMapCoordinateDto(latitudeDegrees: 39.736, longitudeDegrees: -104.995),
                    accessNote: "Fictional screenshot data. No real charging access is offered.",
                    cost: .paid,
                    name: "Riverglass Charge Stop",
                    venue: .evChargingStation,
                    hours: "Always open",
                    terms: .directFee,
                    price: nil,
                    supplies: [MobileChargingSupplyDto(voltageVolts: 240, connectors: [.j1772], outletCount: 4)]
                ),
            ]
            for input in inputs { _ = try await database.saveChargingSpot(fields: input) }
        }
    }
#endif
