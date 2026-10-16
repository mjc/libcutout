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
            case .rideTracking:
                try await seedActiveDenverRide(database: database)
            case .chargingMap:
                try await seedFictionalChargingSpots(database: database)
            case .eucMain, .rgb, .tune, .none:
                break
            }
        }

        private static func seedActiveDenverRide(database: RideDatabaseHandle) async throws {
            try await Task.detached(priority: .utility) {
                let nowWall = UInt64(Date().timeIntervalSince1970 * 1_000)
                let nowMonotonic = UInt64(ProcessInfo.processInfo.systemUptime * 1_000)
                let startOffset: UInt64 = 300_000
                guard nowWall > startOffset, nowMonotonic > startOffset else {
                    throw CocoaError(.coderInvalidValue)
                }
                let startedWall = nowWall - startOffset
                let startedMonotonic = nowMonotonic - startOffset
                let rideID = try database.createRideWithMonotonicStart(
                    source: .live,
                    createdAtMilliseconds: startedWall,
                    monotonicCreatedAtMilliseconds: startedMonotonic
                )
                _ = try database.transition(id: rideID, event: .start, monotonicAtMilliseconds: startedMonotonic)
                // A synthetic route on the public Platte River Trail, smoothed into evenly
                // spaced samples for a five-minute ride.
                let anchors: [(latitude: Double, longitude: Double)] = [
                    (39.72860, -105.01659),
                    (39.72942, -105.01714),
                    (39.73162, -105.01741),
                    (39.73504, -105.01738),
                    (39.73768, -105.01824),
                    (39.73898, -105.01826),
                    (39.73944, -105.01795),
                    (39.74299, -105.01669),
                    (39.74285, -105.01612),
                    (39.74471, -105.01532),
                    (39.74694, -105.01523),
                    (39.74949, -105.01485),
                    (39.74988, -105.01456),
                    (39.75022, -105.01365),
                    (39.75135, -105.01201),
                    (39.75393, -105.00885),
                    (39.75436, -105.00937),
                    (39.75490, -105.00875),
                ]
                var cumulativeDistanceMeters = [0.0]
                for index in 1..<anchors.count {
                    let distance = Self.distanceMeters(from: anchors[index - 1], to: anchors[index])
                    cumulativeDistanceMeters.append(cumulativeDistanceMeters[index - 1] + distance)
                }
                let totalDistanceMeters = cumulativeDistanceMeters.last ?? 0
                guard totalDistanceMeters > 0 else { throw CocoaError(.coderInvalidValue) }
                for index in 0...50 {
                    let traveledDistanceMeters = totalDistanceMeters * Double(index) / 50
                    let endIndex = min(
                        cumulativeDistanceMeters.firstIndex(where: { $0 >= traveledDistanceMeters }) ?? anchors.count
                            - 1,
                        anchors.count - 1
                    )
                    let startIndex = max(0, endIndex - 1)
                    let segmentDistanceMeters =
                        cumulativeDistanceMeters[endIndex] - cumulativeDistanceMeters[startIndex]
                    let fraction =
                        segmentDistanceMeters > 0
                        ? (traveledDistanceMeters - cumulativeDistanceMeters[startIndex]) / segmentDistanceMeters
                        : 0
                    let start = anchors[startIndex]
                    let end = anchors[endIndex]
                    let latitude = start.latitude + (end.latitude - start.latitude) * fraction
                    let longitude = start.longitude + (end.longitude - start.longitude) * fraction
                    let elapsed = UInt64(Double(startOffset) * traveledDistanceMeters / totalDistanceMeters)
                    let admitted = try database.appendLocation(
                        id: rideID,
                        location: MobileRideLocationDto(
                            latitudeDegrees: latitude,
                            longitudeDegrees: longitude,
                            monotonicMilliseconds: startedMonotonic + elapsed,
                            wallClockUnixMilliseconds: startedWall + elapsed,
                            horizontalAccuracyMillimetres: 4_000,
                            source: .live
                        )
                    )
                    guard admitted == .accepted else { throw CocoaError(.coderInvalidValue) }
                }
                guard let record = try database.findRide(rideId: rideID),
                    record.state == .active, record.summary.pointCount == 51
                else { throw CocoaError(.coderInvalidValue) }
            }.value
        }

        private static func distanceMeters(
            from first: (latitude: Double, longitude: Double),
            to second: (latitude: Double, longitude: Double)
        ) -> Double {
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
