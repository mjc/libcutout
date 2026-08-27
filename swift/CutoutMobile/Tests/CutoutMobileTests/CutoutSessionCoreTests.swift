import XCTest
import CutoutMobileFFI
import CoreLocation
#if canImport(CoreBluetooth)
import CoreBluetooth
#endif
@testable import CutoutMobile

final class CutoutSessionCoreTests: XCTestCase {
    #if canImport(CoreBluetooth)
    func testCoreBluetoothRestorationPolicyOptsInAndSelectsOnlySavedDevice() {
        XCTAssertEqual(
            CoreBluetoothRestorationPolicy.centralManagerOptions[CBCentralManagerOptionRestoreIdentifierKey]
                as? String,
            "io.cutout.central"
        )
        XCTAssertEqual(
            CoreBluetoothRestorationPolicy.selectedPlatformIdentifier(
                savedPlatformIdentifier: "wheel-b",
                restoredPlatformIdentifiers: ["wheel-a", "wheel-b"]
            ),
            "wheel-b"
        )
        XCTAssertNil(
            CoreBluetoothRestorationPolicy.selectedPlatformIdentifier(
                savedPlatformIdentifier: "wheel-c",
                restoredPlatformIdentifiers: ["wheel-a", "wheel-b"]
            )
        )
    }
    #endif

    func testBoundedDiagnosticLogRetainsNewestRecordsAndCountsDroppedHistory() {
        var log = BoundedDiagnosticLog(capacity: 3)

        ["one", "two", "three", "four"].forEach { log.append($0) }

        XCTAssertEqual(log.values, ["two", "three", "four"])
        XCTAssertEqual(log.droppedCount, 1)
    }

    func testMonotonicClockUsesItsInjectedUptimeSource() {
        var now = MonotonicMilliseconds(100)
        let clock = MonotonicClock(now: { now })

        XCTAssertEqual(clock.now(), MonotonicMilliseconds(100))

        now = MonotonicMilliseconds(250)
        XCTAssertEqual(clock.now(), MonotonicMilliseconds(250))
    }

    func testMonotonicElapsedSaturatesWhenTheClockMovesBackward() {
        XCTAssertEqual(
            MonotonicMilliseconds(1_333).elapsed(since: MonotonicMilliseconds(1_000)),
            MonotonicMilliseconds(333)
        )
        XCTAssertEqual(
            MonotonicMilliseconds(1_000).elapsed(since: MonotonicMilliseconds(1_333)),
            MonotonicMilliseconds(0)
        )
    }

    func testLocationTimestampAdmissionAdvancesOnlyForAcceptedDecisionsAndResets() {
        let point = MobileRideMapPointDto(
            sequence: 0,
            segmentId: 0,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            wallClockUnixMs: 1_700_000_000_000,
            monotonicMs: 1_000,
            horizontalAccuracyMeters: 4,
            telemetryState: .gpsOnly
        )
        let first = Date(timeIntervalSince1970: 1_700_000_000)
        let second = Date(timeIntervalSince1970: 1_700_000_001)
        var admission = LocationTimestampAdmission()

        admission.record(first, decision: .rejected(reason: .accuracyTooLow))
        admission.record(first, decision: .ignored(reason: .rideNotRecording))
        admission.record(first, decision: .storageError(message: "queue full", retryable: false))
        XCTAssertNil(admission.lastAcceptedTimestamp)

        admission.record(first, decision: .pending(point: point, segmentStarted: true))
        XCTAssertEqual(admission.lastAcceptedTimestamp, first)

        admission.record(second, decision: .accepted(point: point, segmentStarted: false))
        XCTAssertEqual(admission.lastAcceptedTimestamp, second)

        admission.reset()
        XCTAssertNil(admission.lastAcceptedTimestamp)
    }

    func testLocationTimestampAdmissionDoesNotRegressForDuplicateOrOutOfOrderAcceptedDecisions() {
        let point = MobileRideMapPointDto(
            sequence: 0,
            segmentId: 0,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            wallClockUnixMs: 1_700_000_000_000,
            monotonicMs: 1_000,
            horizontalAccuracyMeters: 4,
            telemetryState: .gpsOnly
        )
        let first = Date(timeIntervalSince1970: 1_700_000_000)
        let second = Date(timeIntervalSince1970: 1_700_000_001)
        var admission = LocationTimestampAdmission()

        admission.record(first, decision: .accepted(point: point, segmentStarted: true))
        admission.record(first, decision: .accepted(point: point, segmentStarted: false))
        admission.record(first.addingTimeInterval(-1), decision: .accepted(point: point, segmentStarted: false))

        XCTAssertEqual(admission.lastAcceptedTimestamp, first)

        admission.record(second, decision: .accepted(point: point, segmentStarted: false))
        XCTAssertEqual(admission.lastAcceptedTimestamp, second)
    }

    func testCoreLocationTimestampConversionRejectsInvalidAndOutOfRangeDates() {
        XCTAssertEqual(
            wallClockUnixMilliseconds(for: Date(timeIntervalSince1970: 1_700_000_000.125)),
            1_700_000_000_125
        )
        XCTAssertNil(wallClockUnixMilliseconds(for: Date(timeIntervalSince1970: -1)))
        XCTAssertNil(wallClockUnixMilliseconds(for: Date(timeIntervalSince1970: .nan)))
        XCTAssertNil(
            wallClockUnixMilliseconds(
                for: Date(timeIntervalSince1970: Double(Int64.max) / 1_000)
            )
        )
    }

    func testLocationAvailabilityDistinguishesServicesPermissionAndStorage() {
        XCTAssertEqual(
            locationAvailability(
                servicesEnabled: false,
                authorizationStatus: .authorizedAlways,
                storageAvailable: true
            ),
            .servicesDisabled
        )
        XCTAssertEqual(
            locationAvailability(
                servicesEnabled: true,
                authorizationStatus: .notDetermined,
                storageAvailable: true
            ),
            .permissionRequired
        )
        XCTAssertEqual(
            locationAvailability(
                servicesEnabled: true,
                authorizationStatus: .authorizedAlways,
                storageAvailable: false
            ),
            .storageUnavailable
        )
    }

    func testLocationAuthorizationRequestsAlwaysForBackgroundRecording() throws {
        let authorizedWhenInUse = try XCTUnwrap(CLAuthorizationStatus(rawValue: 4))
        XCTAssertEqual(
            locationAuthorizationAction(for: .notDetermined),
            .requestWhenInUse
        )
        XCTAssertEqual(
            locationAuthorizationAction(for: authorizedWhenInUse),
            .requestAlwaysAndStart
        )
        XCTAssertEqual(
            locationAuthorizationAction(for: .authorizedAlways),
            .start
        )
        XCTAssertEqual(
            locationAuthorizationAction(for: .denied),
            .stop
        )
        XCTAssertEqual(
            locationAuthorizationAction(for: .restricted),
            .stop
        )
    }

    func testBatchedLocationsPreserveTheirRecordedTimeSpacing() {
        let timestamps = [
            Date(timeIntervalSince1970: 1_700_000_000),
            Date(timeIntervalSince1970: 1_700_000_001.5),
            Date(timeIntervalSince1970: 1_700_000_004),
        ]

        XCTAssertEqual(
            monotonicMillisecondsForLocationBatch(
                timestamps: timestamps,
                callbackMonotonicMs: MonotonicMilliseconds(20_000),
                callbackWallClock: Date(timeIntervalSince1970: 1_700_000_004)
            ),
            [
                MonotonicMilliseconds(16_000),
                MonotonicMilliseconds(17_500),
                MonotonicMilliseconds(20_000),
            ]
        )
    }

    func testBatchedLocationsSkipDuplicateAndOutOfOrderSourceTimes() {
        let first = Date(timeIntervalSince1970: 1_700_000_000)
        let second = Date(timeIntervalSince1970: 1_700_000_001)
        let callback = Date(timeIntervalSince1970: 1_700_000_002)

        XCTAssertEqual(
            monotonicMillisecondsForLocationBatch(
                timestamps: [first, first],
                callbackMonotonicMs: MonotonicMilliseconds(20_000),
                callbackWallClock: callback
            ),
            [.some(MonotonicMilliseconds(20_000)), nil]
        )
        XCTAssertEqual(
            monotonicMillisecondsForLocationBatch(
                timestamps: [second, first],
                callbackMonotonicMs: MonotonicMilliseconds(20_000),
                callbackWallClock: callback
            ),
            [.some(MonotonicMilliseconds(20_000)), nil]
        )
    }

    func testBatchedLocationsRejectStaleAndFutureSourceTimesUsingInjectedWallClock() {
        let lastAccepted = Date(timeIntervalSince1970: 1_700_000_010)
        let callback = Date(timeIntervalSince1970: 1_700_000_020)

        XCTAssertEqual(
            monotonicMillisecondsForLocationBatch(
                timestamps: [Date(timeIntervalSince1970: 1_700_000_009)],
                callbackMonotonicMs: MonotonicMilliseconds(20_000),
                callbackWallClock: callback,
                lastAcceptedTimestamp: lastAccepted
            ),
            [nil]
        )
        XCTAssertEqual(
            monotonicMillisecondsForLocationBatch(
                timestamps: [Date(timeIntervalSince1970: 1_700_000_021)],
                callbackMonotonicMs: MonotonicMilliseconds(20_000),
                callbackWallClock: callback,
                lastAcceptedTimestamp: lastAccepted
            ),
            [nil]
        )
    }

    func testBatchedLocationsKeepValidSamplesAroundAnInvalidTimestamp() {
        let first = Date(timeIntervalSince1970: 1_700_000_000)
        let future = Date(timeIntervalSince1970: 1_700_000_010)
        let last = Date(timeIntervalSince1970: 1_700_000_002)

        XCTAssertEqual(
            monotonicMillisecondsForLocationBatch(
                timestamps: [first, future, last],
                callbackMonotonicMs: MonotonicMilliseconds(20_000),
                callbackWallClock: Date(timeIntervalSince1970: 1_700_000_003)
            ),
            [
                .some(MonotonicMilliseconds(18_000)),
                nil,
                .some(MonotonicMilliseconds(20_000)),
            ]
        )
    }

    func testBatchedLocationsDoNotReuseAConstructedMonotonicTimestamp() {
        let base = Date(timeIntervalSince1970: 1_700_000_000)

        let result = monotonicMillisecondsForLocationBatch(
            timestamps: [
                base,
                base.addingTimeInterval(0.0004),
                base.addingTimeInterval(0.002),
            ],
            callbackMonotonicMs: MonotonicMilliseconds(20_000),
            callbackWallClock: base.addingTimeInterval(0.002)
        )

        XCTAssertEqual(result[0], .some(MonotonicMilliseconds(19_998)))
        XCTAssertNil(result[1])
        XCTAssertEqual(result[2], .some(MonotonicMilliseconds(20_000)))
    }

    func testCoreLocationSentinelsBecomeTypedAbsence() {
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(20_000) },
            wallClock: WallClock { Date(timeIntervalSince1970: 1_700_000_001) }
        )
        let location = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: -1,
            verticalAccuracy: -1,
            course: 0,
            courseAccuracy: -1,
            speed: -1,
            speedAccuracy: -1,
            timestamp: Date(timeIntervalSince1970: 1_700_000_000)
        )

        core.locationManager(CLLocationManager(), didUpdateLocations: [location])

        let sample = core.phoneLocationSnapshot.latestSample
        XCTAssertNotNil(sample)
        XCTAssertNil(sample?.horizontalAccuracyMeters)
        XCTAssertNil(sample?.verticalAccuracyMeters)
        XCTAssertNil(sample?.speedMetersPerSecond)
        XCTAssertNil(sample?.speedAccuracyMetersPerSecond)
        XCTAssertEqual(sample?.courseDegrees?.value, 0)
        XCTAssertNil(sample?.courseAccuracyDegrees)
        XCTAssertEqual(sample?.latitudeDegrees.value, 39.7392)
        XCTAssertEqual(sample?.longitudeDegrees.value, -104.9903)
        XCTAssertEqual(sample?.altitudeMeters.value, 1_600)

        let invalidCourseLocation = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: -1,
            verticalAccuracy: -1,
            course: -1,
            courseAccuracy: -1,
            speed: -1,
            speedAccuracy: -1,
            timestamp: Date(timeIntervalSince1970: 1_700_000_001)
        )
        core.locationManager(CLLocationManager(), didUpdateLocations: [invalidCourseLocation])
        XCTAssertNil(core.phoneLocationSnapshot.latestSample?.courseDegrees)

        let nonFiniteLocation = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: .nan,
            verticalAccuracy: .infinity,
            course: .nan,
            courseAccuracy: .nan,
            speed: .nan,
            speedAccuracy: .infinity,
            timestamp: Date(timeIntervalSince1970: 1_700_000_002)
        )
        core.locationManager(CLLocationManager(), didUpdateLocations: [nonFiniteLocation])

        let nonFiniteSample = core.phoneLocationSnapshot.latestSample
        XCTAssertNotNil(nonFiniteSample)
        XCTAssertNil(nonFiniteSample?.horizontalAccuracyMeters)
        XCTAssertNil(nonFiniteSample?.verticalAccuracyMeters)
        XCTAssertNil(nonFiniteSample?.speedMetersPerSecond)
        XCTAssertNil(nonFiniteSample?.speedAccuracyMetersPerSecond)
        XCTAssertNil(nonFiniteSample?.courseDegrees)
        XCTAssertNil(nonFiniteSample?.courseAccuracyDegrees)
        XCTAssertEqual(nonFiniteSample?.latitudeDegrees.value, 39.7392)
        XCTAssertEqual(nonFiniteSample?.longitudeDegrees.value, -104.9903)
    }

    func testConnectionAutoStartResetsLocationTimestampAdmissionForANewRide() throws {
        let timestamp = Date(timeIntervalSince1970: 1_700_000_000)
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(20_000) },
            wallClock: WallClock { timestamp },
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 60_000
            )
        )

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        core.locationManager(
            CLLocationManager(),
            didUpdateLocations: [Self.location(timestamp: timestamp, latitude: 39.7392)]
        )
        XCTAssertEqual(core.phoneLocationSnapshot.latestSample?.latitudeDegrees.value, 39.7392)

        _ = try core.rideMapStateHandle.stop(atMs: 20_000)
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        core.locationManager(
            CLLocationManager(),
            didUpdateLocations: [Self.location(timestamp: timestamp, latitude: 39.7402)]
        )

        XCTAssertEqual(core.phoneLocationSnapshot.latestSample?.latitudeDegrees.value, 39.7402)
    }

    func testPhoneLocationReadbackTracksAValidSampleWithoutAnActiveRide() {
        let core = CutoutSessionCore()
        let location = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: 4,
            verticalAccuracy: 6,
            course: 90,
            courseAccuracy: 3,
            speed: 2,
            speedAccuracy: 0.2,
            timestamp: Date(timeIntervalSince1970: 1_700_000_000)
        )

        core.locationManager(CLLocationManager(), didUpdateLocations: [location])

        XCTAssertEqual(core.phoneLocationSnapshot.latestSample?.latitudeDegrees.value, 39.7392)
        XCTAssertEqual(core.phoneLocationSnapshot.latestSample?.longitudeDegrees.value, -104.9903)
    }

    private static func location(timestamp: Date, latitude: CLLocationDegrees) -> CLLocation {
        CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: latitude, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: 4,
            verticalAccuracy: 6,
            course: 90,
            courseAccuracy: 3,
            speed: 2,
            speedAccuracy: 0.2,
            timestamp: timestamp
        )
    }

    func testRejectedLocationDoesNotPoisonTheNextCallbackTimestamp() async throws {
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(20_000) },
            wallClock: WallClock { Date(timeIntervalSince1970: 1_700_000_020) }
        )
        guard RustPersistenceStore.shared != nil else {
            throw XCTSkip("Rust ride database is unavailable in this test environment")
        }
        defer {
            _ = try? core.rideMapStateHandle.stop(atMs: 20_000)
            _ = try? core.rideMapStateHandle.discard()
        }
        _ = try core.rideMapStateHandle.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)

        let rejected = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: 200,
            verticalAccuracy: 6,
            course: 90,
            courseAccuracy: 3,
            speed: 2,
            speedAccuracy: 0.2,
            timestamp: Date(timeIntervalSince1970: 1_700_000_010)
        )
        let valid = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: 4,
            verticalAccuracy: 6,
            course: 90,
            courseAccuracy: 3,
            speed: 2,
            speedAccuracy: 0.2,
            timestamp: Date(timeIntervalSince1970: 1_700_000_009)
        )

        core.locationManager(CLLocationManager(), didUpdateLocations: [rejected])
        core.locationManager(CLLocationManager(), didUpdateLocations: [valid])

        var snapshot: MobileRideMapSnapshotDto?
        for _ in 0 ..< 10_000 {
            _ = core.rideMapStateHandle.pollLocationWrites()
            snapshot = core.rideMapStateHandle.currentSnapshot(atMs: 20_000)
            if snapshot?.summary.pointCount == 1 { break }
            await Task.yield()
        }
        XCTAssertEqual(snapshot?.summary.pointCount, 1)
    }

    func testPevcapLocationContextUsesOnlyAdmittedMapSamples() {
        let sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_000_000,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            altitudeMeters: 1_600,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: 2,
            speedAccuracyMetersPerSecond: 0.2,
            courseDegrees: 90,
            courseAccuracyDegrees: 3
        )
        let point = MobileRideMapPointDto(
            sequence: 0,
            segmentId: 0,
            latitudeDegrees: sample.latitudeDegrees.value,
            longitudeDegrees: sample.longitudeDegrees.value,
            wallClockUnixMs: sample.wallClockUnixMs,
            monotonicMs: 1_000,
            horizontalAccuracyMeters: sample.horizontalAccuracyMeters?.value ?? 0,
            telemetryState: .gpsOnly
        )

        XCTAssertEqual(
            capturePhoneLocationSample(
                sample: sample,
                decision: .accepted(point: point, segmentStarted: true)
            ),
            sample
        )
        XCTAssertNil(
            capturePhoneLocationSample(
                sample: sample,
                decision: .rejected(reason: .accuracyTooLow)
            )
        )
        XCTAssertNil(
            capturePhoneLocationSample(
                sample: sample,
                decision: .ignored(reason: .rideNotRecording)
            )
        )
        XCTAssertNil(
            capturePhoneLocationSample(
                sample: sample,
                decision: .pending(point: point, segmentStarted: true)
            )
        )
        XCTAssertNil(
            capturePhoneLocationSample(
                sample: sample,
                decision: .storageError(message: "queue full", retryable: false)
            )
        )
    }

    func testPhoneLocationStateClearPreventsCrossCaptureContext() {
        let state = MobilePhoneLocationState()
        let sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_000_000,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            altitudeMeters: 1_600,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: 2,
            speedAccuracyMetersPerSecond: 0.2,
            courseDegrees: 90,
            courseAccuracyDegrees: 3
        )

        _ = state.ingest(sample: sample)
        XCTAssertEqual(state.currentSnapshot().latestSample, sample)

        state.clear()

        XCTAssertNil(state.currentSnapshot().latestSample)
    }

    func testDatabaseBackedRideMapReportsPendingThenDurablyAccepted() async throws {
        guard let database = RustPersistenceStore.shared else {
            throw XCTSkip("Rust ride database is unavailable in this test environment")
        }

        let state = MobileRideMapState(database: database)
        defer {
            _ = try? state.stop(atMs: 200)
            _ = try? state.discard()
        }
        _ = try state.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)
        let decision = try state.ingestLocation(
            monotonicMs: 100,
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            horizontalAccuracyMeters: 4
        )

        guard case let .pending(point, segmentStarted) = decision else {
            return XCTFail("database-backed ingestion should return pending, got \(decision)")
        }

        var accepted: MobileRideMapDecisionDto?
        for _ in 0 ..< 10_000 {
            if let outcome = state.pollLocationWrites().first {
                accepted = outcome
                break
            }
            await Task.yield()
        }

        guard case let .accepted(acceptedPoint, acceptedSegmentStarted) = accepted else {
            return XCTFail("pending location did not produce a durable acceptance")
        }
        XCTAssertEqual(acceptedPoint, point)
        XCTAssertEqual(acceptedSegmentStarted, segmentStarted)
    }

    func testPendingLocationPollUsesInjectedSchedulerWithoutSleeping() async throws {
        guard RustPersistenceStore.shared != nil else {
            throw XCTSkip("Rust ride database is unavailable in this test environment")
        }
        let scheduler = RecordingRideMapPollScheduler()
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(20_000) },
            wallClock: WallClock { Date(timeIntervalSince1970: 1_700_000_020) },
            rideMapPollScheduler: scheduler
        )
        defer {
            _ = try? core.rideMapStateHandle.stop(atMs: 20_000)
            _ = try? core.rideMapStateHandle.discard()
        }
        _ = try core.rideMapStateHandle.startGpsOnly(atMs: 100, lastConnectedVehicle: nil)

        let location = CLLocation(
            coordinate: CLLocationCoordinate2D(latitude: 39.7392, longitude: -104.9903),
            altitude: 1_600,
            horizontalAccuracy: 4,
            verticalAccuracy: 6,
            course: 90,
            courseAccuracy: 3,
            speed: 2,
            speedAccuracy: 0.2,
            timestamp: Date(timeIntervalSince1970: 1_700_000_010)
        )
        core.locationManager(CLLocationManager(), didUpdateLocations: [location])

        XCTAssertEqual(scheduler.scheduledCount, 1)
        for _ in 0 ..< 10_000 {
            guard scheduler.runNext() else {
                await Task.yield()
                continue
            }
            if core.rideMapStateHandle.currentSnapshot(atMs: 20_000)?.summary.pointCount == 1 {
                break
            }
            await Task.yield()
        }
        XCTAssertEqual(
            core.rideMapStateHandle.currentSnapshot(atMs: 20_000)?.summary.pointCount,
            1
        )
        XCTAssertGreaterThan(scheduler.runCount, 0)
    }

    func testTerminalRideMapTransitionClearsPendingSamplesAndStalePollWork() async throws {
        guard RustPersistenceStore.shared != nil else {
            throw XCTSkip("Rust ride database is unavailable in this test environment")
        }
        let scheduler = RecordingRideMapPollScheduler()
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(20_000) },
            wallClock: WallClock { Date(timeIntervalSince1970: 1_700_000_020) },
            rideMapPollScheduler: scheduler
        )
        defer {
            _ = try? core.rideMapStateHandle.stop(atMs: 20_000)
            _ = try? core.rideMapStateHandle.discard()
        }

        _ = try core.startRideMapGpsOnly(atMs: 100, lastConnectedVehicle: nil)
        core.locationManager(
            CLLocationManager(),
            didUpdateLocations: [Self.location(
                timestamp: Date(timeIntervalSince1970: 1_700_000_010),
                latitude: 39.7392
            )]
        )
        XCTAssertEqual(scheduler.scheduledCount, 1)

        _ = try core.stopRideMap(atMs: 200)
        XCTAssertTrue(scheduler.runNext())
        XCTAssertEqual(scheduler.scheduledCount, 0)

        _ = try core.startRideMapGpsOnly(atMs: 300, lastConnectedVehicle: nil)
        core.locationManager(
            CLLocationManager(),
            didUpdateLocations: [Self.location(
                timestamp: Date(timeIntervalSince1970: 1_700_000_011),
                latitude: 39.7402
            )]
        )
        for _ in 0 ..< 10_000 {
            _ = scheduler.runNext()
            if core.rideMapStateHandle.currentSnapshot(atMs: 20_000)?.summary.pointCount == 1 {
                break
            }
            await Task.yield()
        }
        XCTAssertEqual(
            core.rideMapStateHandle.currentSnapshot(atMs: 20_000)?.summary.pointCount,
            1
        )
    }

    func testPendingLocationQueueDoesNotCollideWhenAStoppedRideRestartsAtSequenceZero() {
        let oldSample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_000_100,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            altitudeMeters: 1_600,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: 2,
            speedAccuracyMetersPerSecond: 0.2,
            courseDegrees: 90,
            courseAccuracyDegrees: 3
        )
        let newSample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_100_100,
            latitudeDegrees: 39.7393,
            longitudeDegrees: -104.9904,
            altitudeMeters: 1_601,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: 2,
            speedAccuracyMetersPerSecond: 0.2,
            courseDegrees: 90,
            courseAccuracyDegrees: 3
        )
        let oldPoint = MobileRideMapPointDto(
            sequence: 0,
            segmentId: 0,
            latitudeDegrees: oldSample.latitudeDegrees.value,
            longitudeDegrees: oldSample.longitudeDegrees.value,
            wallClockUnixMs: oldSample.wallClockUnixMs,
            monotonicMs: 100,
            horizontalAccuracyMeters: oldSample.horizontalAccuracyMeters?.value ?? 0,
            telemetryState: .gpsOnly
        )
        let newPoint = MobileRideMapPointDto(
            sequence: 0,
            segmentId: 0,
            latitudeDegrees: newSample.latitudeDegrees.value,
            longitudeDegrees: newSample.longitudeDegrees.value,
            wallClockUnixMs: newSample.wallClockUnixMs,
            monotonicMs: 10_100,
            horizontalAccuracyMeters: newSample.horizontalAccuracyMeters?.value ?? 0,
            telemetryState: .gpsOnly
        )
        let oldRideTerminalDecisions: [MobileRideMapDecisionDto] = [
            .ignored(reason: .rideNotRecording),
            .rejected(reason: .timestampOutOfOrder),
            .storageError(message: "old ride write failed", retryable: false),
        ]
        for terminalDecision in oldRideTerminalDecisions {
            var queue = PendingPhoneLocationQueue()
            queue.append(oldSample, sequence: oldPoint.sequence)
            queue.append(newSample, sequence: newPoint.sequence)

            XCTAssertEqual(queue.take(for: terminalDecision), oldSample)
            XCTAssertEqual(
                queue.take(for: .accepted(point: newPoint, segmentStarted: true)),
                newSample
            )
        }
    }

    func testPendingPhoneLocationQueueDropsContextWhenAcceptedSequenceIsMissing() {
        var queue = PendingPhoneLocationQueue()
        queue.append(
            MobilePhoneLocationSampleDto(
                wallClockUnixMs: 1_700_000_000_000,
                latitudeDegrees: 39.7392,
                longitudeDegrees: -104.9903,
                altitudeMeters: 1_600,
                horizontalAccuracyMeters: 4,
                verticalAccuracyMeters: 6,
                speedMetersPerSecond: 2,
                speedAccuracyMetersPerSecond: 0.2,
                courseDegrees: 90,
                courseAccuracyDegrees: 3
            ),
            sequence: 7
        )

        let missingSequencePoint = MobileRideMapPointDto(
            sequence: 8,
            segmentId: 0,
            latitudeDegrees: 39.7393,
            longitudeDegrees: -104.9904,
            wallClockUnixMs: 1_700_000_001_000,
            monotonicMs: 1_000,
            horizontalAccuracyMeters: 4,
            telemetryState: .gpsOnly
        )

        XCTAssertNil(queue.take(for: .accepted(point: missingSequencePoint, segmentStarted: false)))
        XCTAssertTrue(queue.isEmpty)
    }

    func testPendingPhoneLocationQueueDeduplicatesSequenceAndCanDiscardDirectOutcome() {
        let sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_000_000,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            altitudeMeters: 1_600,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: 2,
            speedAccuracyMetersPerSecond: 0.2,
            courseDegrees: 90,
            courseAccuracyDegrees: 3
        )
        var queue = PendingPhoneLocationQueue()
        queue.append(sample, sequence: 7)
        queue.append(sample, sequence: 7)

        XCTAssertEqual(queue.count, 1)
        queue.remove(sequence: 7, sample: sample)
        XCTAssertTrue(queue.isEmpty)
    }

    func testPevcapLocationStateIsSeparateFromPhoneLocationReadback() {
        let sample = MobilePhoneLocationSampleDto(
            wallClockUnixMs: 1_700_000_000_000,
            latitudeDegrees: 39.7392,
            longitudeDegrees: -104.9903,
            altitudeMeters: 1_600,
            horizontalAccuracyMeters: 4,
            verticalAccuracyMeters: 6,
            speedMetersPerSecond: 2,
            speedAccuracyMetersPerSecond: 0.2,
            courseDegrees: 90,
            courseAccuracyDegrees: 3
        )
        let point = MobileRideMapPointDto(
            sequence: 0,
            segmentId: 0,
            latitudeDegrees: sample.latitudeDegrees.value,
            longitudeDegrees: sample.longitudeDegrees.value,
            wallClockUnixMs: sample.wallClockUnixMs,
            monotonicMs: 1_000,
            horizontalAccuracyMeters: sample.horizontalAccuracyMeters?.value ?? 0,
            telemetryState: .gpsOnly
        )
        let admittedState = MobilePhoneLocationState()

        XCTAssertNil(
            capturePhoneLocationSample(
                sample: sample,
                decision: .ignored(reason: .rideNotRecording),
                state: admittedState
            )
        )
        XCTAssertNil(admittedState.currentSnapshot().latestSample)

        XCTAssertEqual(
            capturePhoneLocationSample(
                sample: sample,
                decision: .accepted(point: point, segmentStarted: true),
                state: admittedState
            ),
            sample
        )
        XCTAssertEqual(admittedState.currentSnapshot().latestSample, sample)
    }

    func testCaptureElapsedTimeUsesTheInjectedMonotonicClockAtExactBoundaries() {
        var now = MonotonicMilliseconds(2_999)
        let core = CutoutSessionCore(clock: MonotonicClock { now })
        let startedAt = MonotonicMilliseconds(1_000)

        XCTAssertEqual(core.captureElapsedMilliseconds(since: startedAt), 1_999)

        now = MonotonicMilliseconds(3_000)
        XCTAssertEqual(core.captureElapsedMilliseconds(since: startedAt), 2_000)

        now = MonotonicMilliseconds(3_001)
        XCTAssertEqual(core.captureElapsedMilliseconds(since: startedAt), 2_001)
    }

    func testConnectionReconnectPolicyBoundsJitteredBackoff() {
        XCTAssertEqual(ConnectionReconnectPolicy.delayMilliseconds(attempt: 1, jitter: 0), 200)
        XCTAssertEqual(ConnectionReconnectPolicy.delayMilliseconds(attempt: 2, jitter: 0.5), 500)
        XCTAssertEqual(ConnectionReconnectPolicy.delayMilliseconds(attempt: 3, jitter: 1), 1_200)
        XCTAssertNil(ConnectionReconnectPolicy.delayMilliseconds(attempt: 4, jitter: 0.5))
    }

    func testRestoredConnectedPeripheralStartsRideMapBeforeTelemetry() {
        XCTAssertTrue(
            RideMapConnectionPolicy.shouldEnsureRecording(
                hasObservedConnection: false,
                hasSelectedRoute: true
            )
        )
        XCTAssertFalse(
            RideMapConnectionPolicy.shouldEnsureRecording(
                hasObservedConnection: true,
                hasSelectedRoute: true
            )
        )
        XCTAssertFalse(
            RideMapConnectionPolicy.shouldEnsureRecording(
                hasObservedConnection: false,
                hasSelectedRoute: false
            )
        )
    }

    func testReconnectSchedulerCancelsSupersededAndExplicitRetries() {
        let scheduler = RecordingReconnectScheduler()
        let reconnects = ConnectionReconnectController(scheduler: scheduler)
        var completed = [String]()

        XCTAssertEqual(
            reconnects.schedule(jitter: 0) { completed.append("first") },
            ConnectionReconnectSchedule(attempt: 1, delayMilliseconds: 200)
        )
        XCTAssertEqual(
            reconnects.schedule(jitter: 0.5) { completed.append("second") },
            ConnectionReconnectSchedule(attempt: 2, delayMilliseconds: 500)
        )

        scheduler.runAll()
        XCTAssertEqual(completed, ["second"])

        XCTAssertEqual(
            reconnects.schedule(jitter: 1) { completed.append("cancelled") },
            ConnectionReconnectSchedule(attempt: 3, delayMilliseconds: 1_200)
        )
        reconnects.cancel()
        scheduler.runAll()

        XCTAssertEqual(completed, ["second"])
        XCTAssertEqual(reconnects.attempt, 0)
    }

    func testReconnectExhaustionCancelsTheLastPendingRetry() {
        let scheduler = RecordingReconnectScheduler()
        let reconnects = ConnectionReconnectController(scheduler: scheduler)
        var completed = [String]()

        XCTAssertNotNil(reconnects.schedule(jitter: 0) { completed.append("first") })
        XCTAssertNotNil(reconnects.schedule(jitter: 0) { completed.append("second") })
        XCTAssertNotNil(reconnects.schedule(jitter: 0) { completed.append("third") })
        XCTAssertNil(reconnects.schedule(jitter: 0) { completed.append("exhausted") })

        scheduler.runAll()

        XCTAssertTrue(completed.isEmpty)
        XCTAssertEqual(reconnects.attempt, ConnectionReconnectPolicy.maximumAttempts + 1)
    }

    func testNordicNotificationUUIDsRemainFullWidthForPevcap() {
        let service = CBUUID(string: "6E400001-B5A3-F393-E0A9-E50E24DCCA9E")
        let notify = CBUUID(string: "6E400003-B5A3-F393-E0A9-E50E24DCCA9E")

        XCTAssertEqual(BluetoothUuid(coreBluetoothUuid: service)?.bytes.count, 16)
        XCTAssertEqual(BluetoothUuid(coreBluetoothUuid: notify)?.bytes.count, 16)
        XCTAssertEqual(
            BluetoothUuid(coreBluetoothUuid: service)?.bytes,
            BluetoothUuid(Data([
                0x6e, 0x40, 0x00, 0x01, 0xb5, 0xa3, 0xf3, 0x93,
                0xe0, 0xa9, 0xe5, 0x0e, 0x24, 0xdc, 0xca, 0x9e,
            ]))?.bytes
        )
    }

    func testObservedAdvertisementsUpdatePickerScanState() {
        let core = CutoutSessionCore()
        var observedStates: [DevicePickerScanState] = []
        core.onScanStateChange = { observedStates.append($0) }

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NOSFET Aero",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-unknown"),
                localName: "Little FOCer",
                advertisedServiceUuids: [.bluetooth16(0xFFF0)]
            )
        )

        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(core.scanState.rows.map(\.title), ["NOSFET Aero", "Little FOCer"])
        XCTAssertEqual(core.scanState.rows.map(\.connectionRoute), [.electricUnicycle, .vescOnewheel])
        XCTAssertEqual(core.scanState.sections.supported.map(\.title), ["NOSFET Aero", "Little FOCer"])
        XCTAssertTrue(core.scanState.sections.unsupported.isEmpty)
        XCTAssertEqual(observedStates.count, 2)
    }

    func testUnnamedNordicUartAdvertisementRoutesAsVesc() {
        let core = CutoutSessionCore()

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc-unnamed"),
                localName: nil,
                advertisedServiceUuids: [.vescNordicUartService]
            )
        )

        XCTAssertEqual(core.scanState.rows.map(\.title), ["VESC device"])
        XCTAssertEqual(core.scanState.rows.first?.connectionRoute, .vescOnewheel)
    }

    func testObservedAdvertisementsHideNonPevRows() {
        let core = CutoutSessionCore()

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-keyboard"),
                localName: "Keyboard",
                advertisedServiceUuids: []
            )
        )

        XCTAssertTrue(core.scanState.rows.isEmpty)
    }

    func testPairUnknownCandidateReturnsFalse() {
        let core = CutoutSessionCore()

        XCTAssertFalse(core.pair(platformIdentifier: "ios-local-missing"))
    }

    func testScriptedProbeTimeoutFailsWithoutPublishingLive() {
        let failed = expectation(description: "probe timeout is published")
        let live = expectation(description: "probe timeout never publishes live")
        live.isInverted = true
        let core = CutoutSessionCore(testScript: CutoutSessionTestScript(
            candidate: scriptedProbeCandidate,
            telemetry: nil,
            identificationProbeFailure: .timedOut,
            connectionDelayMilliseconds: 0
        ))
        core.onPhaseChange = { phase in
            if phase == .failed(.identificationFailed(.timedOut)) {
                failed.fulfill()
            }
            if phase == .live {
                live.fulfill()
            }
        }

        core.start()
        XCTAssertTrue(core.probe(platformIdentifier: scriptedProbeCandidate.platformIdentifier))

        wait(for: [failed, live], timeout: 0.2)
        XCTAssertEqual(core.phase, .failed(.identificationFailed(.timedOut)))
        XCTAssertNil(core.displayState.speed.millimetersPerSecond)
    }

    func testScriptedSessionUsesTheCorePublicationPath() {
        let live = expectation(description: "scripted session reaches live")
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                connectionDelayMilliseconds: 0
            )
        )
        core.onPhaseChange = { phase in
            if phase == .live {
                live.fulfill()
            }
        }

        core.start()
        XCTAssertEqual(core.scanState.rows, [scriptedVescCandidate.pickerRow])
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))

        wait(for: [live], timeout: 1)
        XCTAssertEqual(core.phase, .live)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 8_000)
    }

    func testScriptedConnectedSessionStartsAndAssociatesTheLiveRideMap() {
        let live = expectation(description: "scripted session reaches live")
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                startsLive: true,
                connectionDelayMilliseconds: 0
            )
        )
        core.onPhaseChange = { phase in
            if phase == .live {
                live.fulfill()
            }
        }

        core.start()

        wait(for: [live], timeout: 1)
        XCTAssertEqual(core.rideMapStateHandle.currentSnapshot()?.state, .recording)
        XCTAssertEqual(
            core.rideMapStateHandle.currentSnapshot()?.associatedVehicle,
            scriptedVescCandidate.platformIdentifier
        )
    }

    func testScriptedBluetoothUnavailableSessionPublishesNoPickerRows() {
        assertScriptedInitialBluetoothState(
            .unavailable,
            phase: .bluetoothUnavailable(rawState: 4),
            scanState: .bluetoothUnavailable
        )
    }

    func testScriptedBluetoothPermissionDeniedSessionPublishesNoPickerRows() {
        assertScriptedInitialBluetoothState(
            .permissionDenied,
            phase: .bluetoothPermissionDenied,
            scanState: .permissionDenied
        )
    }

    private func assertScriptedInitialBluetoothState(
        _ initialBluetoothState: CutoutSessionTestInitialBluetoothState,
        phase expectedPhase: SessionConnectionPhase,
        scanState expectedScanState: DevicePickerScanState
    ) {
        let unavailable = expectation(description: "scripted session becomes unavailable")
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                initialBluetoothState: initialBluetoothState
            )
        )
        core.onPhaseChange = { phase in
            if phase == expectedPhase {
                unavailable.fulfill()
            }
        }

        core.start()

        wait(for: [unavailable], timeout: 1)
        XCTAssertEqual(core.phase, expectedPhase)
        XCTAssertEqual(core.scanState, expectedScanState)
    }

    func testExplicitDisconnectCancelsTheScriptedLateLiveCallback() {
        let live = expectation(description: "late scripted callback is ignored")
        live.isInverted = true
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                connectionDelayMilliseconds: 50
            )
        )
        core.onPhaseChange = { phase in
            if phase == .live {
                live.fulfill()
            }
        }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        core.disconnectAndScan()

        wait(for: [live], timeout: 0.2)
        XCTAssertEqual(core.phase, .scanning)
        XCTAssertEqual(core.scanState.rows, [scriptedVescCandidate.pickerRow])
        XCTAssertNil(core.displayState.speed.millimetersPerSecond)
    }

    func testScriptedSessionPublishesReconnectAndReturnsLive() {
        let retry = expectation(description: "scripted session schedules reconnect")
        let live = expectation(description: "scripted session returns live")
        live.expectedFulfillmentCount = 2
        let core = CutoutSessionCore(
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: TelemetrySnapshot(speed: speedValue(8_000)),
                reconnectsAfterFirstLive: true,
                reconnectDelayMilliseconds: 0,
                connectionDelayMilliseconds: 0
            )
        )
        core.onPhaseChange = { phase in
            if phase == .live {
                live.fulfill()
            }
        }
        core.onReconnectScheduled = { _ in retry.fulfill() }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))

        wait(for: [retry, live], timeout: 3)
        XCTAssertEqual(core.phase, .live)
    }

    func testTransportTerminationUsesTheSharedReconnectTransition() {
        let scheduler = RecordingReconnectScheduler()
        let retry = expectation(description: "transport termination schedules retry")
        var reconnectCount = 0
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(1_000) },
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 60_000
            ),
            reconnectScheduler: scheduler,
            reconnectJitter: { 0 }
        )
        core.onReconnectScheduled = { scheduled in
            XCTAssertEqual(scheduled.platformIdentifier, self.scriptedVescCandidate.platformIdentifier)
            XCTAssertEqual(scheduled.attempt, 1)
            XCTAssertEqual(scheduled.deadline, MonotonicMilliseconds(1_200))
            retry.fulfill()
        }

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))

        core.handleTransportTermination(
            platformIdentifier: scriptedVescCandidate.platformIdentifier,
            error: nil,
            reconnect: { reconnectCount += 1 }
        )

        wait(for: [retry], timeout: 1)
        XCTAssertEqual(core.phase, .discoveringServices)
        XCTAssertEqual(reconnectCount, 0)

        scheduler.runAll()
        XCTAssertEqual(reconnectCount, 1)
    }

    func testBluetoothStateChangesClearPickerCancelReconnectAndRestoreScanning() {
        let scheduler = RecordingReconnectScheduler()
        var reconnectCount = 0
        var scanCount = 0
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(1_000) },
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 60_000
            ),
            reconnectScheduler: scheduler,
            reconnectJitter: { 0 }
        )

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        core.handleTransportTermination(
            platformIdentifier: scriptedVescCandidate.platformIdentifier,
            error: nil,
            reconnect: { reconnectCount += 1 }
        )

        core.handleCentralState(.poweredOff, startScan: {})
        scheduler.runAll()

        XCTAssertEqual(core.phase, .bluetoothUnavailable(rawState: CBManagerState.poweredOff.rawValue))
        XCTAssertEqual(core.scanState, DevicePickerScanState(status: .bluetoothUnavailable, rows: []))
        XCTAssertEqual(reconnectCount, 0)

        core.handleCentralState(.unauthorized, startScan: { scanCount += 1 })
        XCTAssertEqual(core.phase, .bluetoothPermissionDenied)
        XCTAssertEqual(core.scanState, .permissionDenied)
        XCTAssertEqual(scanCount, 0)

        core.handleCentralState(.poweredOn, startScan: { scanCount += 1 })
        XCTAssertEqual(core.phase, .scanning)
        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(scanCount, 1)
    }

    func testTransportTerminationExhaustionCannotRunAnOlderReconnect() {
        let scheduler = RecordingReconnectScheduler()
        var reconnectCount = 0
        let core = CutoutSessionCore(
            clock: MonotonicClock { MonotonicMilliseconds(1_000) },
            testScript: CutoutSessionTestScript(
                candidate: scriptedVescCandidate,
                telemetry: nil,
                connectionDelayMilliseconds: 60_000
            ),
            reconnectScheduler: scheduler,
            reconnectJitter: { 0 }
        )

        core.start()
        XCTAssertTrue(core.pair(platformIdentifier: scriptedVescCandidate.platformIdentifier))
        for _ in 0...ConnectionReconnectPolicy.maximumAttempts {
            core.handleTransportTermination(
                platformIdentifier: scriptedVescCandidate.platformIdentifier,
                error: nil,
                reconnect: { reconnectCount += 1 }
            )
        }

        scheduler.runAll()

        XCTAssertEqual(core.phase, .failed(.connectFailed("unknown error")))
        XCTAssertEqual(core.scanState.rows, [scriptedVescCandidate.pickerRow])
        XCTAssertEqual(reconnectCount, 0)
    }

    func testRecordOnlyMissingCandidateReturnsFalse() {
        let core = CutoutSessionCore()

        XCTAssertFalse(core.recordOnly(platformIdentifier: "ios-local-missing", note: "unknown wheel"))
    }

    func testNotificationCapturePassesThroughWhenCaptureIsInactive() {
        let core = CutoutSessionCore()

        XCTAssertNil(
            core.captureFrame(
                direction: "notify",
                characteristic: CBUUID(string: "FFE1"),
                service: CBUUID(string: "FFE0"),
                bytes: Data([0x01])
            )
        )
        XCTAssertEqual(core.phase, .starting)
    }

    func testSuccessfulScriptedRecordOnlyFlushUsesTheRealWriter() async throws {
        let started = expectation(description: "real capture writer starts")
        var captureURL: URL?
        let core = CutoutSessionCore(testScript: CutoutSessionTestScript(
            candidate: scriptedVescCandidate,
            telemetry: nil,
            connectionDelayMilliseconds: 0
        ))
        core.onCaptureEvent = { event in
            if case let .started(fileURL) = event {
                captureURL = fileURL
                started.fulfill()
            }
        }

        XCTAssertTrue(core.recordOnly(
            platformIdentifier: scriptedVescCandidate.platformIdentifier,
            note: "durability test",
            annotations: ["durability=background"]
        ))
        await fulfillment(of: [started], timeout: 1)
        let url = try XCTUnwrap(captureURL)
        defer { try? FileManager.default.removeItem(at: url) }

        let flushSucceeded = await core.flushCapture()
        XCTAssertTrue(flushSucceeded)
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        XCTAssertGreaterThan((attributes[.size] as? NSNumber)?.uint64Value ?? 0, 0)
        let capture = try String(contentsOf: url, encoding: .utf8)
        XCTAssertTrue(capture.contains("durability=background"))
        XCTAssertTrue(capture.contains("capture_evidence=simulator_fixture"))
        XCTAssertFalse(capture.contains("capture_evidence=hardware_tested"))

        core.disconnectAndScan()
    }

    func testObservedAdvertisementsReplaceDuplicatePeripheralRows() {
        let core = CutoutSessionCore()

        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "Begode Falcon",
                advertisedServiceUuids: []
            )
        )
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "Begode Falcon",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        XCTAssertEqual(core.scanState.rows.map(\.id), ["ios-local-falcon"])
        XCTAssertEqual(core.scanState.sections.supported.map(\.title), ["Begode Falcon"])
    }

    func testApplyNotificationStepMarksLiveAndUpdatesDisplayState() {
        let core = CutoutSessionCore()
        let snapshot = TelemetrySnapshot(
            speed: speedValue(1_234),
            operatingState: .riding,
            voltage: voltageValue(117_000),
            powerFlow: .negativeUnknown,
            batteryLevelEstimated: batteryLevelValue(77)
        )
        let step = CoreBluetoothSessionStep(operations: [], snapshot: snapshot)
        let receivedAt = MonotonicMilliseconds(42)

        core.applyNotificationStep(step, receivedAt: receivedAt)

        XCTAssertEqual(core.phase, .live)
        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(
            EucRideScreenState(phase: core.phase, displayState: core.displayState).operatingState,
            .riding
        )
        XCTAssertEqual(core.displayState.telemetry?.speed, Speed(value: 1_234))
        XCTAssertEqual(core.displayState.telemetry?.powerFlow, .negativeUnknown)
        XCTAssertEqual(core.displayState.notificationCount, 1)
        XCTAssertEqual(core.displayState.lastUpdate, receivedAt)
    }

    func testApplyNotificationStepPublishesDisplayStateOnMainThread() {
        nonisolated(unsafe) let core = CutoutSessionCore()
        let published = expectation(description: "display state published")
        core.onDisplayStateChange = { _ in
            XCTAssertTrue(Thread.isMainThread)
            published.fulfill()
        }

        DispatchQueue.global().async {
            core.applyNotificationStep(
                CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot()),
                receivedAt: MonotonicMilliseconds(42)
            )
        }

        wait(for: [published], timeout: 1.0)
    }

    func testDisplayPublicationThrottleUsesMonotonicTime() {
        let clock = TestMonotonicClock(MonotonicMilliseconds(1_000))
        let core = CutoutSessionCore(clock: MonotonicClock(now: { clock.now }))
        var publicationCount = 0
        core.onDisplayStateChange = { _ in publicationCount += 1 }

        let step = CoreBluetoothSessionStep(operations: [], snapshot: TelemetrySnapshot())
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1_000))

        clock.now = MonotonicMilliseconds(1_200)
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1_200))

        clock.now = MonotonicMilliseconds(1_333)
        core.applyNotificationStep(step, receivedAt: MonotonicMilliseconds(1_333))

        XCTAssertEqual(publicationCount, 2)
    }

    func testDisplayPublicationThrottleDoesNotDelaySafetyWarningTransitions() throws {
        let clock = TestMonotonicClock(MonotonicMilliseconds(1_000))
        let core = CutoutSessionCore(clock: MonotonicClock(now: { clock.now }))
        var publishedStates: [RideDisplayState] = []
        core.onDisplayStateChange = { publishedStates.append($0) }

        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(500))
            ),
            receivedAt: MonotonicMilliseconds(1_000)
        )

        clock.now = MonotonicMilliseconds(1_200)
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(800))
            ),
            receivedAt: MonotonicMilliseconds(1_200)
        )

        XCTAssertEqual(publishedStates.count, 2)
        XCTAssertEqual(
            EucRideScreenState(
                phase: .live,
                displayState: try XCTUnwrap(publishedStates.last)
            ).warningState.severity,
            .reduceAcceleration
        )

        clock.now = MonotonicMilliseconds(1_250)
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(500))
            ),
            receivedAt: MonotonicMilliseconds(1_250)
        )

        XCTAssertEqual(publishedStates.count, 3)
        XCTAssertEqual(
            EucRideScreenState(
                phase: .live,
                displayState: try XCTUnwrap(publishedStates.last)
            ).warningState.severity,
            .normal
        )
    }

    func testVescRideSnapshotKeepsRideCriticalFieldsTyped() {
        let snapshot = VescRideSnapshot(
            title: "Fungineers X7",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            warning: .dutyPushback,
            boardSpeed: speedValue(19_000),
            dutyCycle: dutyCycle(820),
            dutyHeadroom: batteryLevelValue(18),
            batteryCurrent: batteryCurrentValue(38_000),
            powerFlow: .discharge,
            motorCurrent: phaseCurrentValue(71_000),
            boardAngle: angleValue(-18),
            controllerTemperature: temperatureValue(54_000),
            motorTemperature: temperatureValue(49_000)
        )

        XCTAssertEqual(snapshot.vehicleKind, .float)
        XCTAssertEqual(snapshot.subProtocol, .refloat)
        XCTAssertEqual(snapshot.controllerState, .unknown)
        XCTAssertEqual(snapshot.warning, .dutyPushback)
        XCTAssertEqual(snapshot.boardSpeed, speedValue(19_000))
        XCTAssertEqual(snapshot.dutyCycle, dutyCycle(820))
        XCTAssertEqual(snapshot.dutyHeadroom, batteryLevelValue(18))
        XCTAssertEqual(snapshot.batteryCurrent, batteryCurrentValue(38_000))
        XCTAssertEqual(snapshot.powerFlow, .discharge)
        XCTAssertEqual(snapshot.motorCurrent, phaseCurrentValue(71_000))
        XCTAssertEqual(snapshot.boardAngle, angleValue(-18))
        XCTAssertEqual(snapshot.controllerTemperature, temperatureValue(54_000))
        XCTAssertEqual(snapshot.motorTemperature, temperatureValue(49_000))
    }

    func testVescRideSnapshotUsesProtocolDecodedSafetyState() throws {
        let snapshot = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    speed: speedValue(19_000),
                    operatingState: .riding,
                    vescOperatingMode: .handtest,
                    vescWarning: .dutyPushback,
                    vescStopReason: .pitch
                )
            ),
            title: nil
        ))

        XCTAssertEqual(snapshot.warning, .dutyPushback)
        XCTAssertEqual(snapshot.stopReason, .pitch)
        XCTAssertEqual(snapshot.operatingMode, .handtest)
    }

    func testVescRideSnapshotOwnsBatteryReadbackFormatting() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            batteryLevelReported: batteryLevelValue(72),
            batteryCurrent: batteryCurrentValue(38_000)
        )

        XCTAssertEqual(
            snapshot.batteryReadback,
            .reported(level: "72", current: "38.0")
        )
    }

    func testVescRideSnapshotOwnsBoardAngleReadbackFormatting() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            boardAngle: angleValue(-18_000),
            balanceAngle: angleValue(500)
        )

        XCTAssertEqual(
            snapshot.boardAngleReadback,
            .available(orientation: .noseDown, balanceAngle: "0.5")
        )
    }

    func testVescRideSnapshotOwnsMotorTemperatureReadbackFormatting() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            motorTemperature: temperatureValue(49_000)
        )

        XCTAssertEqual(
            snapshot.controllerTemperatureReadback,
            .available(motorTemperature: "49.0")
        )
    }

    func testRideHeroReadoutOwnsVescSpeedFreshnessAndSeverity() {
        let snapshot = VescRideSnapshot(
            title: "VESC",
            vehicleKind: .float,
            subProtocol: .refloat,
            controllerState: .unknown,
            warning: .dutyPushback,
            boardSpeed: speedValue(19_000),
            lastUpdate: MonotonicMilliseconds(1_000)
        )

        XCTAssertEqual(
            RideHeroReadout.vesc(
                snapshot: snapshot,
                now: MonotonicMilliseconds(4_000)
            ),
            .available(
                value: "42.5",
                unit: "mph",
                freshness: .stale,
                severity: .caution
            )
        )
    }

    func testTelemetryThermalReadbackOwnsSensorFormatting() {
        let snapshot = TelemetrySnapshot(
            controllerTemperature: temperatureValue(54_000),
            motorTemperature: temperatureValue(49_000)
        )

        XCTAssertEqual(
            snapshot.thermalReadback,
            .controllerMotor(controller: "54", motor: "49")
        )
    }

    func testVescVehicleKindDoesNotImplySubProtocol() {
        let snapshot = VescRideSnapshot(
            title: "VESC Bike",
            vehicleKind: .bike,
            subProtocol: .generic,
            controllerState: .unknown
        )

        XCTAssertEqual(snapshot.vehicleKind, .bike)
        XCTAssertEqual(snapshot.subProtocol, .generic)

        let bike = VescRideSnapshot(
            title: "VESC Bike",
            vehicleKind: .bike,
            subProtocol: .bike,
            controllerState: .unknown
        )
        XCTAssertEqual(bike.vehicleKind, .bike)
        XCTAssertEqual(bike.subProtocol, .bike)

        let eskate = VescRideSnapshot(
            title: "VESC Skateboard",
            vehicleKind: .skateboard,
            subProtocol: .eskate,
            controllerState: .unknown
        )
        XCTAssertEqual(eskate.vehicleKind, .skateboard)
        XCTAssertEqual(eskate.subProtocol, .eskate)
    }

    func testVescRideSnapshotProjectsLiveDisplayTelemetryWithoutInventingSpeed() throws {
        let telemetry = TelemetrySnapshot(
            voltage: voltageValue(75_400),
            batteryCurrent: batteryCurrentValue(38_000),
            motorCurrent: phaseCurrentValue(71_000),
            powerFlow: .discharge,
            controllerTemperature: temperatureValue(54_000),
            motorTemperature: temperatureValue(49_000)
        )
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: "Little FOCer BT"))

        XCTAssertEqual(snapshot.title, "Little FOCer BT")
        XCTAssertEqual(snapshot.vehicleKind, .float)
        XCTAssertEqual(snapshot.subProtocol, .generic)
        XCTAssertEqual(snapshot.controllerState, .unknown)
        XCTAssertNil(snapshot.boardSpeed)
        XCTAssertEqual(snapshot.batteryVoltage, voltageValue(75_400))
        XCTAssertEqual(snapshot.batteryCurrent, batteryCurrentValue(38_000))
        XCTAssertEqual(snapshot.powerFlow, .discharge)
        XCTAssertEqual(snapshot.motorCurrent, phaseCurrentValue(71_000))
        XCTAssertEqual(snapshot.controllerTemperature, temperatureValue(54_000))
        XCTAssertEqual(snapshot.motorTemperature, temperatureValue(49_000))
    }

func testVescRideSnapshotProjectsBatteryLevelAndUpdateTime() throws {
        let telemetry = TelemetrySnapshot(
            operatingState: .parked,
            voltage: voltageValue(61_000),
            batteryLevelReported: batteryLevelValue(72),
            batteryLevelEstimated: batteryLevelValue(70)
        )
        let displayState = RideDisplayState(
            telemetry: telemetry,
            notificationCount: 1,
            lastUpdate: MonotonicMilliseconds(900)
        )

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.batteryLevelReported, batteryLevelValue(72))
        XCTAssertEqual(snapshot.batteryLevelEstimated, batteryLevelValue(70))
        XCTAssertEqual(snapshot.lastUpdate, MonotonicMilliseconds(900))
        XCTAssertEqual(
            snapshot.updateAge(
                at: MonotonicMilliseconds(1_000),
                staleAfter: MonotonicMilliseconds(250)
            ),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(100), freshness: .fresh)
        )
        XCTAssertEqual(
            snapshot.updateAge(
                at: MonotonicMilliseconds(1_300),
                staleAfter: MonotonicMilliseconds(250)
            ),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(400), freshness: .stale)
        )
    }

    func testVescRideSnapshotDerivesDutyHeadroomFromLiveDutyCycle() throws {
        let balancedTelemetry = TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(0))
        let idleNoiseTelemetry = TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(10))
        let loadedTelemetry = TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))

        let balanced = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: balancedTelemetry, notificationCount: 1),
            title: nil
        ))
        let idleNoise = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: idleNoiseTelemetry, notificationCount: 1),
            title: nil
        ))
        let loaded = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: loadedTelemetry, notificationCount: 1),
            title: nil
        ))

        XCTAssertEqual(balanced.dutyCycle, dutyCycle(0))
        XCTAssertEqual(balanced.dutyHeadroom, batteryLevelValue(100))
        XCTAssertEqual(idleNoise.dutyCycle, dutyCycle(10))
        XCTAssertEqual(idleNoise.dutyHeadroom, batteryLevelValue(100))
        XCTAssertEqual(loaded.dutyCycle, dutyCycle(230))
        XCTAssertEqual(loaded.dutyHeadroom, batteryLevelValue(77))
        XCTAssertEqual(loaded.dutyHeadroomMetricValue, .available(display: "77", accessibility: "77"))
        XCTAssertEqual(
            loaded.dutyHeadroomProgressMetricValue,
            .available(display: "77%", accessibility: "77%")
        )
        XCTAssertEqual(try XCTUnwrap(loaded.dutyHeadroomProgress), 0.77, accuracy: 0.001)
    }

    func testVescRideSnapshotMarksParkedHeadroomNotApplicableWithoutProgress() throws {
        let telemetry = TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(10))

        let snapshot = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: telemetry, notificationCount: 1),
            title: nil
        ))

        XCTAssertEqual(snapshot.dutyCycle, dutyCycle(10))
        XCTAssertNil(snapshot.dutyHeadroom)
        XCTAssertEqual(snapshot.dutyHeadroomApplicability, .notApplicable)
        XCTAssertEqual(
            snapshot.dutyHeadroomMetricValue,
            .status(display: "Not applicable", accessibility: "Not applicable")
        )
        XCTAssertEqual(
            snapshot.dutyHeadroomProgressMetricValue,
            .status(display: "Not applicable", accessibility: "Not applicable")
        )
        XCTAssertNil(snapshot.dutyHeadroomProgress)
    }

    func testVescRideSnapshotKeepsMissingDutyHeadroomUnavailable() throws {
        let telemetry = TelemetrySnapshot(operatingState: .parked, voltage: voltageValue(62_800))

        let snapshot = try XCTUnwrap(VescRideSnapshot(
            displayState: RideDisplayState(telemetry: telemetry, notificationCount: 1),
            title: nil
        ))

        XCTAssertNil(snapshot.dutyCycle)
        XCTAssertNil(snapshot.dutyHeadroom)
        XCTAssertEqual(snapshot.dutyHeadroomApplicability, .unavailable)
        XCTAssertEqual(snapshot.dutyHeadroomMetricValue, .unavailable)
        XCTAssertEqual(snapshot.dutyHeadroomProgressMetricValue, .unavailable)
        XCTAssertNil(snapshot.dutyHeadroomProgress)
    }

    func testVescRideSnapshotProjectsFootpadFromSharedTelemetry() throws {
        let footpad = FootpadTelemetry(state: 3, adc1Milliunits: 1_250, adc2Milliunits: 875)
        let telemetry = TelemetrySnapshot(footpad: footpad)
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.footpad, footpad)
        XCTAssertNil(snapshot.boardSpeed)
        XCTAssertNil(snapshot.boardAngle)
    }

    func testFootpadTelemetryExposesTypedAdcValues() {
        let footpad = FootpadTelemetry(state: 3, adc1Milliunits: 1_250, adc2Milliunits: nil)

        XCTAssertEqual(
            footpad.adc1MetricValue,
            .available(display: "1.25", accessibility: "1.25, available")
        )
        XCTAssertEqual(footpad.adc2MetricValue, .unavailable)
        XCTAssertEqual(footpad.stateDisplayText, "state 3")
        XCTAssertEqual(
            footpad.summaryText,
            "footpad state 3 · adc1 left 1.25 · adc2 right unavailable"
        )
    }

    func testFootpadTelemetryUsesTypedContactStateForDisplayAndAccessibility() {
        let cases: [(UInt8, FootpadContactState, String)] = [
            (0, .none, "not pressed"),
            (1, .left, "left pressed"),
            (2, .right, "right pressed"),
            (3, .both, "both pressed"),
        ]

        for (rawState, contactState, expectedDisplayText) in cases {
            let footpad = FootpadTelemetry(
                state: rawState,
                contactState: contactState,
                adc1Milliunits: 1_250,
                adc2Milliunits: 875
            )

            XCTAssertEqual(footpad.stateDisplayText, expectedDisplayText)
            XCTAssertEqual(
                footpad.accessibilityValue,
                "left / adc1, 1.25, available, right / adc2, 0.88, available, \(expectedDisplayText)"
            )
            XCTAssertEqual(
                footpad.summaryText,
                "footpad \(expectedDisplayText) · adc1 left 1.25 · adc2 right 0.88"
            )
        }
    }

    func testFootpadTelemetryKeepsZeroAdcAvailable() {
        let footpad = FootpadTelemetry(state: 0, adc1Milliunits: 0, adc2Milliunits: 0)

        XCTAssertEqual(
            footpad.adc1MetricValue,
            .available(display: "0.00", accessibility: "0.00, available")
        )
        XCTAssertEqual(
            footpad.adc2MetricValue,
            .available(display: "0.00", accessibility: "0.00, available")
        )
    }

    func testFootpadPresentationCopyResolvesFromThePackageCatalog() {
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.state", value: nil, table: "Localizable"),
            "state %lld"
        )
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.accessibility.summary", value: nil, table: "Localizable"),
            "%1$@, %2$@, %3$@, %4$@, %5$@"
        )
        XCTAssertEqual(
            Bundle.module.localizedString(forKey: "footpad.title", value: nil, table: "Localizable"),
            "Footpad"
        )
    }

    func testVescRideSnapshotProjectsAngleOnlyTelemetry() throws {
        let telemetry = TelemetrySnapshot(pitch: angleValue(14_200))
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.boardAngle, angleValue(14_200))
        XCTAssertNil(snapshot.batteryVoltage)
    }

    func testVescRideSnapshotDoesNotUseUnverifiedFactsForLiveDefaults() throws {
        let telemetry = TelemetrySnapshot(voltage: voltageValue(62_800))
        let displayState = RideDisplayState(telemetry: telemetry, notificationCount: 1)

        let snapshot = try XCTUnwrap(VescRideSnapshot(displayState: displayState, title: nil))

        XCTAssertEqual(snapshot.title, VescRideSnapshot.defaultTitle)
        XCTAssertEqual(snapshot.subProtocol, .generic)
        XCTAssertNil(snapshot.boardSpeed)
        XCTAssertNil(snapshot.dutyHeadroom)
        XCTAssertNil(snapshot.boardAngle)
        XCTAssertNil(snapshot.controllerTemperature)
        XCTAssertNil(snapshot.motorTemperature)
        XCTAssertNotEqual(snapshot.title, "Fungineers X7")
    }

    func testVescDebugSnapshotKeepsGuardrailAndReadOnlyStateTyped() {
        let snapshot = VescDebugSnapshot(
            profileTitle: "Profile: Street stable",
            transportDetail: "VESC Express · FW 6.x · UART bridge",
            dutyCycle: dutyCycle(820),
            maxSeenDutyCycle: dutyCycle(870),
            packVoltage: voltageValue(75_400),
            batteryCurrentLimit: batteryCurrentValue(45_000),
            motorCurrentLimit: phaseCurrentValue(90_000),
            lastFault: "FAULT_CODE_NONE",
            inputApp: "ADC + balance",
            canStatus: "single controller",
            logging: "local CSV armed",
            writeGuardrail: .policyRefusal
        )

        XCTAssertEqual(snapshot.dutyCycle, dutyCycle(820))
        XCTAssertEqual(snapshot.maxSeenDutyCycle, dutyCycle(870))
        XCTAssertEqual(snapshot.packVoltage, voltageValue(75_400))
        XCTAssertEqual(snapshot.batteryCurrentLimit, batteryCurrentValue(45_000))
        XCTAssertEqual(snapshot.motorCurrentLimit, phaseCurrentValue(90_000))
        XCTAssertEqual(snapshot.writeGuardrail, .policyRefusal)
    }

    func testSpeedObservationRemainsStickyAcrossTelemetryWithoutSpeed() {
        let core = CutoutSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speed: speedValue(1_234),
            voltage: voltageValue(117_000),
            batteryLevelEstimated: batteryLevelValue(77)
        )
        let batteryOnlySnapshot = TelemetrySnapshot(
            voltage: voltageValue(116_500),
            batteryLevelEstimated: batteryLevelValue(76)
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: speedSnapshot),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: batteryOnlySnapshot),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(core.displayState.telemetry?.voltage, Voltage(value: 116_500))
        XCTAssertEqual(core.displayState.notificationCount, 2)
        XCTAssertEqual(core.displayState.lastUpdate, MonotonicMilliseconds(43))
    }

    func testNotificationWithoutSnapshotAdvancesLastUpdate() {
        let core = CutoutSessionCore()
        let speedSnapshot = TelemetrySnapshot(
            speed: speedValue(1_234),
            voltage: voltageValue(117_000),
            batteryLevelEstimated: batteryLevelValue(77)
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: speedSnapshot),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil),
            receivedAt: MonotonicMilliseconds(99)
        )

        XCTAssertEqual(core.phase, .live)
        XCTAssertTrue(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.displayState.speed.millimetersPerSecond, 1_234)
        XCTAssertEqual(core.displayState.notificationCount, 2)
        XCTAssertEqual(core.displayState.lastUpdate, MonotonicMilliseconds(99))
    }

    func testVescOnewheelCoreBluetoothSessionSubscribesAndRequestsTelemetryOnLinkUp() throws {
        let session = CoreBluetoothSession.vescOnewheel()
        let runner = CoreBluetoothSessionRunner(
            session: session,
            writeLimit: TransportWriteLimitBytes(20)
        )

        let step = try runner.handle(.linkUp(at: MonotonicMilliseconds(7)))

        assertVescTelemetryRequests(step.operations, includesSubscribe: true)
        XCTAssertNil(step.snapshot?.speed)
    }

    func testVescOnewheelCoreBluetoothSessionRequestsTelemetryWithReadOnlyCommand() throws {
        let session = CoreBluetoothSession.vescOnewheel()
        let runner = CoreBluetoothSessionRunner(
            session: session,
            writeLimit: TransportWriteLimitBytes(20)
        )

        let step = try runner.handle(.command(.requestTelemetry, at: MonotonicMilliseconds(11)))

        assertVescTelemetryRequests(step.operations, includesSubscribe: false)
        XCTAssertNil(step.snapshot?.speed)
    }

    func testVescLiveOwnerWritesRequestsBeforeSubscribing() throws {
        let sink = RecordingOperationSink()
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc"),
                localName: "Floatwheel Atom",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(1))

        XCTAssertEqual(sink.events, [.write, .write, .write, .subscribe])
    }

    func testVescLiveOwnerRetriesTelemetryAfterLinkUp() throws {
        let sink = RecordingOperationSink()
        let executionQueue = DispatchQueue(label: "cutout.vesc-live-owner.retry")
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc"),
                localName: "Floatwheel Atom",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink,
            retryCommandOnLinkUp: .requestTelemetry,
            retryDelay: .milliseconds(10),
            executionQueue: executionQueue
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(1))
        XCTAssertEqual(sink.writes.count, 3)
        owner.handleNotificationStateUpdate(
            channel: .vescNordicUartNotify,
            isNotifying: true,
            error: nil
        )
        XCTAssertEqual(sink.writes.count, 3)

        waitForWrites(9, in: sink)

        XCTAssertGreaterThanOrEqual(sink.writes.count, 9)
        XCTAssertEqual(sink.writes.count % 3, 0)
        XCTAssertEqual(sink.writes.prefix(3), sink.writes.suffix(3))
    }

    func testVescLiveOwnerUsesCurrentMonotonicTimeForTelemetryRetry() throws {
        let sink = RecordingOperationSink()
        let executionQueue = DispatchQueue(label: "cutout.vesc-live-owner.monotonic")
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc"),
                localName: "Floatwheel Atom",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink,
            retryCommandOnLinkUp: .requestTelemetry,
            maximumRetryAttempts: 1,
            retryDelay: .milliseconds(10),
            executionQueue: executionQueue,
            monotonicClock: MonotonicClock {
                MonotonicMilliseconds(41)
            }
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(1))
        owner.handleNotificationStateUpdate(
            channel: .vescNordicUartNotify,
            isNotifying: true,
            error: nil
        )

        let retryFinished = expectation(description: "VESC telemetry retry uses current monotonic time")
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(30)) {
            retryFinished.fulfill()
        }
        wait(for: [retryFinished], timeout: 1.0)

        XCTAssertTrue(owner.records.contains(
            .command(.requestTelemetry, at: MonotonicMilliseconds(41))
        ))
    }

    func testVescLiveOwnerBoundsTelemetryRetries() throws {
        let sink = RecordingOperationSink()
        let executionQueue = DispatchQueue(label: "cutout.vesc-live-owner.bounds")
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc"),
                localName: "Floatwheel Atom",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink,
            retryCommandOnLinkUp: .requestTelemetry,
            maximumRetryAttempts: 2,
            retryDelay: .milliseconds(10),
            executionQueue: executionQueue
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(1))
        owner.handleNotificationStateUpdate(
            channel: .vescNordicUartNotify,
            isNotifying: true,
            error: nil
        )

        waitForWrites(9, in: sink)

        XCTAssertEqual(sink.writes.count, 9)
    }


    func testVescLiveOwnerRetriesAfterNonRealtimeNotification() throws {
        let sink = RecordingOperationSink()
        let executionQueue = DispatchQueue(label: "cutout.vesc-live-owner.notification")
        let owner = CoreBluetoothLiveSessionOwner(
            session: .vescOnewheel(),
            advertisement: CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-vesc"),
                localName: "Floatwheel Atom",
                advertisedServiceUuids: []
            ),
            writeLimit: TransportWriteLimitBytes(20),
            operationSink: sink,
            retryCommandOnLinkUp: .requestTelemetry,
            retryDelay: .milliseconds(10),
            executionQueue: executionQueue
        )

        _ = try owner.handleLinkUp(at: MonotonicMilliseconds(1))
        owner.handleNotificationStateUpdate(
            channel: .vescNordicUartNotify,
            isNotifying: true,
            error: nil
        )
        _ = try owner.handleNotification(
            bytes: Data([0x01]),
            channel: .bluetooth16(0xffff),
            at: MonotonicMilliseconds(2)
        )

        waitForWrites(9, in: sink)

        XCTAssertGreaterThanOrEqual(sink.writes.count, 9)
    }

    func testSettingsReadbackUpdatesCurrentSessionStateUntilDisconnect() {
        let core = CutoutSessionCore()
        let readback = SettingsReadback(entries: [
            SettingsReadbackEntry(
                field: RawSettingField(id: 42, value: 1_234),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
        ], availability: .available)
        var observedReadbacks: [SettingsReadback?] = []
        core.onSettingsReadbackChange = { observedReadbacks.append($0) }

        let action = SessionAction.withSettingsReadback(readback)
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [action]),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.settingsReadback, readback)
        XCTAssertEqual(core.settingsReadback?.availability, .available)
        XCTAssertEqual(observedReadbacks, [readback])

        core.disconnectAndScan()

        XCTAssertNil(core.settingsReadback)
        XCTAssertEqual(observedReadbacks, [readback, nil])
    }

    func testNonAvailableSettingsReadbackDoesNotCarryRawEntries() {
        let readback = SettingsReadback(entries: [
            SettingsReadbackEntry(
                field: RawSettingField(id: 42, value: 1_234),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
        ], availability: .unsupported)

        XCTAssertEqual(readback.availability, .unsupported)
        XCTAssertTrue(readback.entries.isEmpty)
        XCTAssertEqual(
            readback.eucGarageSettings,
            EucGarageSettingsSnapshot(
                beepMargin: .unsupported,
                tiltback: .unsupported,
                pedalMode: .unsupported
            )
        )
    }

    func testNonAvailableSettingsReadbackDoesNotCarryGarageProjection() {
        let readback = SettingsReadback(
            entries: [],
            availability: .unsupported,
            eucGarageSettings: EucGarageSettingsSnapshot(
                beepMargin: .available(Speed(value: 3_222)),
                tiltback: .available(Speed(value: 11_666)),
                pedalMode: .available(PedalMode.rawMode(1_920))
            )
        )

        XCTAssertEqual(
            readback.eucGarageSettings,
            EucGarageSettingsSnapshot(
                beepMargin: .unsupported,
                tiltback: .unsupported,
                pedalMode: .unsupported
            )
        )
    }

    func testSettingsReadbackCarriesProjectedVeteranGarageSettings() {
        let readback = SettingsReadback(entries: [
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x0005, value: 116),
                source: .reported,
                quality: .known,
                verification: .sourceAndHardwareVerified
            ),
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x0006, value: 420),
                source: .reported,
                quality: .known,
                verification: .sourceAndHardwareVerified
            ),
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x001e, value: 1_920),
                source: .reported,
                quality: .known,
                verification: .sourceAndHardwareVerified
            ),
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x9999, value: 123),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
        ], eucGarageSettings: EucGarageSettingsSnapshot(
            beepMargin: .available(Speed(value: 3_222)),
            tiltback: .available(Speed(value: 11_666)),
            pedalMode: .available(PedalMode.rawMode(1_920))
        ))

        XCTAssertEqual(
            readback.eucGarageSettings,
            EucGarageSettingsSnapshot(
                beepMargin: .available(Speed(value: 3_222)),
                tiltback: .available(Speed(value: 11_666)),
                pedalMode: .available(PedalMode.rawMode(1_920))
            )
        )
    }

    func testSettingsReadbackCarriesProjectedBegodeGarageSettings() {
        let readback = SettingsReadback(entries: [
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x040a, value: 50),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
            SettingsReadbackEntry(
                field: RawSettingField(id: 0x0406, value: 0),
                source: .reported,
                quality: .known,
                verification: .sourceVerified
            ),
        ], eucGarageSettings: EucGarageSettingsSnapshot(
            beepMargin: .unavailable,
            tiltback: .available(Speed(value: 13_888)),
            pedalMode: .unavailable
        ))

        XCTAssertEqual(
            readback.eucGarageSettings,
            EucGarageSettingsSnapshot(
                beepMargin: .unavailable,
                tiltback: .available(Speed(value: 13_888)),
                pedalMode: .unavailable
            )
        )
    }

    func testFaultHistoryReadbackUpdatesCurrentSessionStateUntilDisconnect() {
        let core = CutoutSessionCore()
        let readback = FaultHistoryReadback.faultSince(
            FaultHistoryEntry(
                code: FaultCode.unknown(id: 0x0040, value: 1),
                source: .reported,
                quality: .known,
                verification: .hardwareVerified
            ),
            sinceDistance: Distance(value: 61_456_941)
        )
        var observedReadbacks: [FaultHistoryReadback?] = []
        core.onFaultHistoryReadbackChange = { observedReadbacks.append($0) }

        let action = SessionAction.withFaultHistoryReadback(readback)
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [action]),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.faultHistoryReadback, readback)
        XCTAssertEqual(core.faultHistoryReadback?.availability, .available)
        XCTAssertEqual(observedReadbacks, [readback])

        core.disconnectAndScan()

        XCTAssertNil(core.faultHistoryReadback)
        XCTAssertEqual(observedReadbacks, [readback, nil])
    }

    func testFaultHistoryReadbackConstructorsKeepNoFaultEvidenceExplicit() {
        let distance = Distance(value: 61_456_941)
        let noFault = FaultHistoryReadback.noFaultSince(distance)
        let unavailable = FaultHistoryReadback.unavailable()
        let unsupported = FaultHistoryReadback.unsupported()

        XCTAssertEqual(noFault.availability, .available)
        XCTAssertNil(noFault.lastFault)
        XCTAssertEqual(noFault.sinceDistance, distance)
        XCTAssertEqual(unavailable.availability, .unavailable)
        XCTAssertNil(unavailable.lastFault)
        XCTAssertNil(unavailable.sinceDistance)
        XCTAssertEqual(unsupported.availability, .unsupported)
        XCTAssertNil(unsupported.lastFault)
        XCTAssertNil(unsupported.sinceDistance)
    }

    func testFaultHistoryGeneratedReadbackStripsPayloadWhenUnavailable() {
        let distance = DistanceReading(
            value: Distance(value: 61_456_941),
            source: .reported,
            quality: .known,
            verification: .sourceVerified
        )
        let unavailable = FaultHistoryReadback(
            MobileFaultHistoryReadbackDto(
                availability: .unavailable,
                lastFault: nil,
                sinceDistance: distance
            )
        )
        let unsupported = FaultHistoryReadback(
            MobileFaultHistoryReadbackDto(
                availability: .unsupported,
                lastFault: nil,
                sinceDistance: distance
            )
        )

        XCTAssertEqual(unavailable, FaultHistoryReadback.unavailable())
        XCTAssertEqual(unsupported, FaultHistoryReadback.unsupported())
    }

    func testBmsSnapshotUpdatesCurrentSessionStateUntilDisconnect() {
        let core = CutoutSessionCore()
        let snapshot = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "unknown BMS topology",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 0,
                bmsCount: 0,
                confidence: .unverified
            ),
            energyPercent: BatteryLevel(value: 72),
            voltage: Voltage(value: 81_600),
            current: BatteryCurrent(value: -1_250),
            highestTemperature: Temperature(value: 37_800)
        )
        var observedSnapshots: [BmsSnapshot?] = []
        core.onBmsSnapshotChange = { observedSnapshots.append($0) }

        let action = SessionAction.withBmsSnapshot(snapshot)
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [action]),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.bmsSnapshot, snapshot)
        XCTAssertEqual(core.bmsSnapshot?.topology.confidence, .unverified)
        XCTAssertEqual(observedSnapshots, [snapshot])

        core.disconnectAndScan()

        XCTAssertNil(core.bmsSnapshot)
        XCTAssertEqual(observedSnapshots, [snapshot, nil])
    }

    func testBmsSnapshotAggregatesCollectedPagesForPackOverview() {
        let core = CutoutSessionCore()
        let metadataPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 2,
            pageKind: "metadata",
            pageVerification: .sourceVerified,
            voltage: Voltage(value: 95_800),
            current: BatteryCurrent(value: 0)
        )
        let cellPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 3,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            cellDelta: VoltageDelta(value: 12),
            lowestGroupIndex: 1,
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_090), alertLevel: .warning),
                BmsGroupSnapshot(index: 2, voltage: Voltage(value: 4_102)),
            ]
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(metadataPage)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(cellPage)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertNil(core.bmsSnapshot?.pageSelector)
        XCTAssertNil(core.bmsSnapshot?.pageKind)
        XCTAssertEqual(core.bmsSnapshot?.topology.layoutLabel, "8 observed BMS groups")
        XCTAssertEqual(core.bmsSnapshot?.voltage, Voltage(value: 95_800))
        XCTAssertEqual(core.bmsSnapshot?.current, BatteryCurrent(value: 0))
        XCTAssertEqual(core.bmsSnapshot?.cellDelta, VoltageDelta(value: 12))
        XCTAssertEqual(core.bmsSnapshot?.groups.count, 2)
    }

    func testBmsSnapshotCollectionDoesNotPublishCursorOnlyUpdates() {
        let core = CutoutSessionCore()
        let firstPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 0,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            voltage: Voltage(value: 95_800),
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_090))
            ]
        )
        let cursorOnlyPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            pageSelector: 1,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            voltage: Voltage(value: 95_800),
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_090))
            ]
        )
        var observedSnapshots: [BmsSnapshot?] = []
        core.onBmsSnapshotChange = { observedSnapshots.append($0) }

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(firstPage)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(cursorOnlyPage)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(observedSnapshots.count, 1)
        XCTAssertNil(core.bmsSnapshot?.pageSelector)
        XCTAssertNil(core.bmsSnapshot?.pageKind)
    }

    func testBmsSnapshotCollectionKeepsSameSelectorWithDifferentProtocolTags() {
        let core = CutoutSessionCore()
        let topology = BmsTopology(
            layoutLabel: "64 observed BMS groups",
            seriesGroupCount: nil,
            parallelCount: nil,
            packCount: 1,
            bmsCount: 2,
            confidence: .unverified
        )
        let firstBank = BmsSnapshot(
            topology: topology,
            pageSelector: 0,
            pageTag: 0x02,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            groups: [
                BmsGroupSnapshot(index: 1, voltage: Voltage(value: 0))
            ]
        )
        let secondBank = BmsSnapshot(
            topology: topology,
            pageSelector: 0,
            pageTag: 0x03,
            pageKind: "cell voltage",
            pageVerification: .sourceVerified,
            groups: [
                BmsGroupSnapshot(index: 33, voltage: Voltage(value: 0))
            ]
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(firstBank)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(secondBank)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(core.bmsSnapshot?.groups.map(\.index), [1, 33])
    }

    func testBmsSnapshotDoesNotReplaceObservedPackIdentityWithUnknown() {
        let core = CutoutSessionCore()
        let observedPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "8 observed BMS groups",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 1,
                bmsCount: 1,
                confidence: .unverified
            ),
            voltage: Voltage(value: 95_800)
        )
        let unknownPage = BmsSnapshot(
            topology: BmsTopology(
                layoutLabel: "unknown BMS topology",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 0,
                bmsCount: 0,
                confidence: .unverified
            ),
            current: BatteryCurrent(value: 0)
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(observedPage)]),
            receivedAt: MonotonicMilliseconds(42)
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(operations: [], snapshot: nil, actions: [.withBmsSnapshot(unknownPage)]),
            receivedAt: MonotonicMilliseconds(43)
        )

        XCTAssertEqual(core.bmsSnapshot?.topology.layoutLabel, "8 observed BMS groups")
        XCTAssertEqual(core.bmsSnapshot?.topology.bmsCount, 1)
        XCTAssertEqual(core.bmsSnapshot?.current, BatteryCurrent(value: 0))
    }

    func testProtocolIdentityCandidateUpdatesFromVeteranModelId() {
        let core = CutoutSessionCore()
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NF2557",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: nil,
                actions: [.protocolIdentity(veteranModelId: 43)]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "NF2557")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "NOSFET Aero confirmed by model id 43")
        XCTAssertEqual(core.protocolIdentityCandidate?.support.electricUnicycleModel, .aero)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["NOSFET Aero confirmed by model id 43"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=NOSFET Aero confirmed by model id 43")
    }

    func testProtocolIdentityFallbackDisplayNameUsesDetectedFamily() {
        let cases: [(DeviceDetectionProtocolFamily?, String, String)] = [
            (.veteranLeaperkimNosfet, "protocol_identity.fallback.veteran_nosfet", "Veteran/NOSFET device"),
            (.begodeGotway, "protocol_identity.fallback.begode", "Begode device"),
            (.vesc, "protocol_identity.fallback.vesc", "VESC device"),
            (nil, "protocol_identity.fallback.unknown", "Detected rideable"),
        ]

        for (protocolFamily, key, expected) in cases {
            XCTAssertEqual(pevLocalizedText(key), expected)
            XCTAssertEqual(protocolIdentityFallbackDisplayName(protocolFamily: protocolFamily), pevLocalizedText(key))
        }
    }

    func testPevcapIdentityDoesNotUseProvisionalSelectedModel() {
        XCTAssertNil(captureResolvedIdentity(protocolIdentityCandidate: nil))
    }

    func testPevcapIdentityUsesProtocolConfirmedCandidate() {
        let candidate = DevicePickerDiscoveryCandidate(candidate: mobileDiscoveryCandidateFromVeteranProtocolIdentity(
            platformIdentifier: "ios-local-aero",
            displayName: "NF2557",
            modelId: 43
        ))

        let identity = captureResolvedIdentity(protocolIdentityCandidate: candidate)

        XCTAssertEqual(identity?.protocolFamily, .veteranLeaperkimNosfet)
        XCTAssertEqual(identity?.model?.value, "NOSFET Aero")
        XCTAssertEqual(identity?.model?.verification, .hardwareVerified)
    }

    func testPevcapAnnotationSanitizesDelimiterCharacters() {
        XCTAssertEqual(
            pevcapAnnotation(key: "device_kind", value: "foo=bar\nbaz\rqux"),
            "device_kind=foo bar baz qux"
        )
        XCTAssertEqual(
            sanitizedPevcapAnnotation("user_note=one=two\nthree"),
            "user_note=one two three"
        )
    }

    func testBegodeProbeWritesAreLabeledForDetectionCapture() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))

        XCTAssertTrue(core.records.contains("begode_probe_write=model"))
        XCTAssertTrue(core.records.contains("begode_probe_write=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_write=imu"))
    }

    func testBegodeProbeWriteDoesNotUseSkippedWriteGuard() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("N".utf8))

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
        XCTAssertTrue(core.records.contains("begode_probe_write=model"))
    }

    func testVescTelemetryRequestDoesNotUseSkippedWriteGuard() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(
            channel: .vescNordicUartWrite,
            bytes: Data([0x02, 0x01, 0x04, 0x40, 0x84, 0x03])
        )

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
    }

    func testVescRealtimeTelemetryRequestDoesNotUseReadOnlyWriteGuard() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(
            channel: .vescNordicUartWrite,
            bytes: Data([0x02, 0x01, 0x0e, 0xe1, 0xce, 0x03])
        )

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
    }

    func testFalconLinkUpPlansBegodeIdentityProbeWrites() throws {
        let runner = CoreBluetoothSessionRunner(
            session: try .electricUnicycle(model: .falcon),
            writeLimit: TransportWriteLimitBytes(23)
        )

        let step = try runner.handle(.linkUp(at: MonotonicMilliseconds(42)))

        XCTAssertEqual(
            step.operations,
            [
                .subscribe(channel: .bluetooth16(0xffe1)),
                .writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("N".utf8)),
                .writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("V".utf8)),
                .writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("M".utf8)),
            ]
        )
    }

    func testIdentificationProbeTransportSubscribesBeforeOrderedWrites() {
        let sink = RecordingOperationSink()
        let transport = IdentificationProbeTransportCoordinator(
            detectionSession: DeviceDetectionSession()
        )

        transport.subscribe(using: sink)
        XCTAssertEqual(sink.events, [.subscribe])

        let outcome = transport.notificationsEnabled(
            at: MonotonicMilliseconds(42),
            using: sink
        )

        XCTAssertEqual(sink.events, [.subscribe, .write, .write, .write])
        XCTAssertEqual(sink.writes, [Data("N".utf8), Data("V".utf8), Data("M".utf8)])
        guard case .writes = outcome else {
            return XCTFail("expected ordered probe writes")
        }

        let resolution = transport.observeNotification(
            channel: .bluetooth16(0xffe1),
            bytes: Data("NAME=Falcon".utf8)
        )
        XCTAssertEqual(resolution.modelBanner, Data("Falcon".utf8))
    }

    func testUnrelatedWriteReachesNormalTransportValidation() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data([0x01]))

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
        XCTAssertFalse(core.records.contains("begode_probe_write=model"))
    }

    func testMultiBytePayloadStartingWithProbeByteReachesNormalTransportValidation() {
        let core = CutoutSessionCore()

        core.writeWithoutResponse(channel: .bluetooth16(0xffe1), bytes: Data("NAME".utf8))

        XCTAssertEqual(core.phase, .failed(.missingWriteChannel))
        XCTAssertFalse(core.records.contains("begode_probe_write=model"))
    }

    func testBegodeProbeResponsesAreLabeledFromDetectionSession() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("GW FALCON 1.0".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("MPU6500".utf8))

        XCTAssertTrue(core.records.contains("begode_probe_response=model"))
        XCTAssertTrue(core.records.contains("begode_probe_response=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_response=imu"))
    }

    func testBegodeProbeResponseUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon"),
                localName: "Typed Begode Falcon",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "Typed Begode Falcon")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/Falcon confirmed by reported model Falcon")
        XCTAssertEqual(core.protocolIdentityCandidate?.support.electricUnicycleModel, .falcon)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/Falcon confirmed by reported model Falcon"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Begode/Falcon confirmed by reported model Falcon")
    }

    func testBegodeFirmwareProbeResponseUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon-code"),
                localName: "GotWay_002441",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("GW-FALCON".utf8))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "GotWay_002441")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/GotWay identity probe collected; code GW-FALCON")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .unknownRecordable(disabledReason: "Unresolved Begode code banner")
        )
        XCTAssertNil(core.protocolIdentityCandidate?.pickerRow.connectionRoute)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/GotWay identity probe collected; code GW-FALCON"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Begode/GotWay identity probe collected; code GW-FALCON")
    }

    func testBegodeImuProbeResponseUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-falcon-imu"),
                localName: "GotWay_002441",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("MPU6500".utf8))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "GotWay_002441")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/GotWay identity probe collected; imu MPU6500")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .unknownRecordable(disabledReason: "Begode model not confirmed")
        )
        XCTAssertNil(core.protocolIdentityCandidate?.pickerRow.connectionRoute)
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/GotWay identity probe collected; imu MPU6500"]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Begode/GotWay identity probe collected; imu MPU6500")
    }

    func testFragmentedBegodeFrameUpdatesProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        let frame: [UInt8] = [
            0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
            0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
            0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
        ]
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-gotway"),
                localName: "Mystery Wheel",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionNotification(channel: channel, bytes: Data(Array(frame.prefix(20))))
        XCTAssertNil(core.protocolIdentityCandidate)

        core.observeDetectionNotification(channel: channel, bytes: Data(Array(frame.dropFirst(20))))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "Mystery Wheel")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Begode/GotWay identity probe collected; model not confirmed")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .unknownRecordable(disabledReason: "Begode model not confirmed")
        )
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            ["Begode/GotWay identity probe collected; model not confirmed"]
        )
        XCTAssertEqual(
            core.records.last,
            "protocol_identity=Begode/GotWay identity probe collected; model not confirmed"
        )
    }

    func testMixedProtocolFamiliesUpdateProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)
        let begodeFrame: [UInt8] = [
            0x55, 0xaa, 0x17, 0x75, 0x05, 0x38, 0x00, 0x76,
            0x02, 0xee, 0xfb, 0x64, 0xf4, 0x94, 0x14, 0x81,
            0x00, 0x09, 0x00, 0x18, 0x5a, 0x5a, 0x5a, 0x5a,
        ]
        var veteranFrame = Array(repeating: UInt8(0), count: 42)
        veteranFrame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
        veteranFrame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-conflict"),
                localName: "Conflicting wheel",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        core.observeDetectionNotification(channel: channel, bytes: Data(veteranFrame))
        core.observeDetectionNotification(channel: channel, bytes: Data(begodeFrame))

        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "Conflicting wheel")
        XCTAssertEqual(core.protocolIdentityCandidate?.detail, "Conflicting protocol family evidence")
        XCTAssertEqual(
            core.protocolIdentityCandidate?.support,
            .conflicting(disabledReason: "Conflicting identity evidence")
        )
        XCTAssertEqual(
            observedCandidates.compactMap { $0?.detail },
            [
                "NOSFET Aero confirmed by model id 43",
                "Conflicting protocol family evidence",
            ]
        )
        XCTAssertEqual(core.records.last, "protocol_identity=Conflicting protocol family evidence")
    }

    func testMalformedBegodeProbeResponseIsLabeledFromDetectionSession() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data([0x4e, 0x41, 0x4d, 0x45, 0x3d, 0x46, 0x61, 0x6c, 0x63, 0x6f, 0x6e, 0x00]))

        XCTAssertTrue(core.records.contains("begode_probe_malformed=model"))
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
    }

    func testMalformedBegodeModelResponseIsLabeledAfterQueuedProbeWrites() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data([0x4e, 0x41, 0x4d, 0x45, 0x3d, 0x46, 0x61, 0x6c, 0x63, 0x6f, 0x6e, 0x00]))

        XCTAssertTrue(core.records.contains("begode_probe_malformed=model"))
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
    }

    func testOutstandingBegodeProbeResponsesAreLabeledMissing() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.markOutstandingBegodeProbeResponsesMissing()

        XCTAssertTrue(core.records.contains("begode_probe_missing=model"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=imu"))
    }

    func testBegodeProbeResponsesExpireOnlyAfterMonotonicDeadline() {
        var now = MonotonicMilliseconds(1_000)
        let core = CutoutSessionCore(clock: MonotonicClock(now: { now }))
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))

        now = MonotonicMilliseconds(2_999)
        core.expireOutstandingBegodeProbeResponses()
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))

        now = MonotonicMilliseconds(3_000)
        core.expireOutstandingBegodeProbeResponses()
        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))

        now = MonotonicMilliseconds(3_001)
        core.expireOutstandingBegodeProbeResponses()
        XCTAssertTrue(core.records.contains("begode_probe_missing=model"))
    }

    func testAnsweredBegodeProbeIsNotLabeledMissing() {
        let core = CutoutSessionCore()
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))
        core.markOutstandingBegodeProbeResponsesMissing()

        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
    }

    func testAnsweredBegodeProbeDoesNotHideOtherMissingResponses() {
        var now = MonotonicMilliseconds(1_000)
        let core = CutoutSessionCore(clock: MonotonicClock(now: { now }))
        let channel = BluetoothUuid.bluetooth16(0xffe1)

        core.observeDetectionProbeWrite(channel: channel, bytes: Data("N".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("V".utf8))
        core.observeDetectionProbeWrite(channel: channel, bytes: Data("M".utf8))
        core.observeDetectionNotification(channel: channel, bytes: Data("NAME=Falcon".utf8))

        now = MonotonicMilliseconds(3_003)
        core.expireOutstandingBegodeProbeResponses()

        XCTAssertFalse(core.records.contains("begode_probe_missing=model"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=firmware"))
        XCTAssertTrue(core.records.contains("begode_probe_missing=imu"))
    }

    func testProtocolIdentityCandidatePrefersSelectedAdvertisement() {
        let core = CutoutSessionCore()
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-selected"),
                localName: "NF2557",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-last"),
                localName: "Later scan row",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )

        XCTAssertFalse(core.pair(platformIdentifier: "ios-local-selected"))
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: nil,
                actions: [.protocolIdentity(veteranModelId: 43)]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        XCTAssertEqual(core.protocolIdentityCandidate?.platformIdentifier, "ios-local-selected")
        XCTAssertEqual(core.protocolIdentityCandidate?.displayName, "NF2557")
    }

    func testDisconnectAndScanClearsProtocolIdentityCandidate() {
        let core = CutoutSessionCore()
        var observedCandidates: [DevicePickerDiscoveryCandidate?] = []
        core.onProtocolIdentityCandidateChange = { observedCandidates.append($0) }
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NF2557",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: nil,
                actions: [.protocolIdentity(veteranModelId: 43)]
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        core.disconnectAndScan()

        XCTAssertEqual(core.protocolIdentityCandidate, nil)
        XCTAssertEqual(
            observedCandidates.map { $0?.detail },
            ["NOSFET Aero confirmed by model id 43", nil]
        )
    }

    func testDisconnectAndScanClearsRideStateAndReturnsPickerToScanning() {
        let core = CutoutSessionCore()
        core.observeAdvertisement(
            CoreBluetoothAdvertisement(
                peripheralIdentifier: CoreBluetoothPeripheralIdentifier("ios-local-aero"),
                localName: "NOSFET Aero",
                advertisedServiceUuids: [.bluetooth16(0xFFE0)]
            )
        )
        core.applyNotificationStep(
            CoreBluetoothSessionStep(
                operations: [],
                snapshot: TelemetrySnapshot(speed: speedValue(1_234))
            ),
            receivedAt: MonotonicMilliseconds(42)
        )

        core.disconnectAndScan()

        XCTAssertEqual(core.phase, .scanning)
        XCTAssertEqual(core.displayState, RideDisplayState())
        XCTAssertFalse(core.hasObservedSpeedSnapshot)
        XCTAssertEqual(core.scanState.status, .scanning)
        XCTAssertEqual(core.scanState.rows.map(\.title), ["NOSFET Aero"])
    }

    func testRideStateCarriesPhaseAndTelemetrySnapshot() {
        let displayState = RideDisplayState(
            speed: SpeedReadout(millimetersPerSecond: 1_234),
            telemetry: TelemetrySnapshot(speed: speedValue(1_234), operatingState: .riding),
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(9_876)
        )
        let rideState = EucRideScreenState(phase: .subscribing, displayState: displayState)

        XCTAssertEqual(rideState.phaseText, "Subscribing...")
        XCTAssertEqual(rideState.speedText, "2.8")
        XCTAssertEqual(rideState.speedUnit, "mph")
        XCTAssertEqual(rideState.operatingState, .riding)
        XCTAssertEqual(rideState.telemetry?.speed, Speed(value: 1_234))
    }

    func testRideStateExposesPwmHeadroomWhileStandingOrRiding() {
        let riding = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))
            )
        )
        let standing = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .standing, pwm: dutyCycle(230))
            )
        )

        XCTAssertEqual(riding.pwmHeadroomApplicability, .available)
        XCTAssertEqual(riding.pwmHeadroomPermille, 770)
        XCTAssertEqual(standing.pwmHeadroomApplicability, .available)
        XCTAssertEqual(standing.pwmHeadroomPermille, 770)
    }

    func testRideStateTreatsIdlePwmHeadroomAsFullHeadroom() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .standing, pwm: dutyCycle(10))
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .available)
        XCTAssertEqual(rideState.pwmHeadroomPermille, 1_000)
    }

    func testRideStateStatusUsesOperatingStateWhenLive() {
        let parked = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked)
            )
        )
        let riding = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding)
            )
        )
        let standing = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .standing)
            )
        )
        let charging = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .charging)
            )
        )

        XCTAssertEqual(parked.statusText, "Parked")
        XCTAssertEqual(riding.statusText, "Riding")
        XCTAssertEqual(standing.statusText, "Standing")
        XCTAssertEqual(charging.statusText, "Charging")
    }

    func testRideStateDistinguishesEmptyLiveSnapshotFromPopulatedTelemetry() {
        let waiting = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )
        let populated = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(118_000))
            )
        )

        XCTAssertEqual(waiting.telemetryAvailability, .waitingForValues)
        XCTAssertEqual(populated.telemetryAvailability, .populated)
    }

    func testRideStateCarriesTypedWarningSeverity() {
        let failed = EucRideScreenState(
            phase: .failed(.connectFailed("link dropped")),
            displayState: RideDisplayState()
        )
        let inactive = EucRideScreenState(
            phase: .scanning,
            displayState: RideDisplayState()
        )
        let waiting = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )
        let populated = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(118_000))
            )
        )

        XCTAssertEqual(failed.warningState.severity, .failed)
        XCTAssertEqual(inactive.warningState.severity, .unavailable)
        XCTAssertEqual(waiting.warningState.severity, .caution)
        XCTAssertEqual(populated.warningState.severity, .normal)
        XCTAssertEqual(waiting.warningState.title, "Waiting for telemetry")
    }

    func testRideStateRecommendsReducingAccelerationForLowRidingPwmHeadroom() {
        let lowHeadroom = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(800))
            )
        )
        let healthyHeadroom = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(500))
            )
        )
        let parked = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(800))
            )
        )

        XCTAssertEqual(lowHeadroom.pwmHeadroomPermille, 200)
        XCTAssertEqual(lowHeadroom.warningState.severity, .reduceAcceleration)
        XCTAssertEqual(lowHeadroom.warningState.title, "Reduce acceleration")
        XCTAssertEqual(healthyHeadroom.warningState.severity, .normal)
        XCTAssertEqual(parked.warningState.severity, .normal)
    }

    func testRideStateTreatsMissingLiveSnapshotAsTelemetryUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState()
        )

        XCTAssertEqual(rideState.telemetryAvailability, .unavailable)
        XCTAssertEqual(rideState.controllerOnlyConfidence, .unknown)
    }

    func testRideStateTreatsParkedPwmHeadroomAsNotApplicable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(0))
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .notApplicable)
        XCTAssertNil(rideState.pwmHeadroomPermille)
    }

    func testRideStateOwnsTypedPwmHeadroomPresentation() throws {
        let available = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding, pwm: dutyCycle(230))
            )
        )
        let notApplicable = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(0))
            )
        )
        let unavailable = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot(operatingState: .riding))
        )

        XCTAssertEqual(
            available.pwmHeadroomMetricValue,
            .available(display: "77%", accessibility: "77%")
        )
        XCTAssertEqual(try XCTUnwrap(available.pwmHeadroomProgress), 0.77, accuracy: 0.001)
        XCTAssertEqual(
            notApplicable.pwmHeadroomMetricValue,
            .status(display: "Not applicable", accessibility: "Not applicable")
        )
        XCTAssertNil(notApplicable.pwmHeadroomProgress)
        XCTAssertEqual(unavailable.pwmHeadroomMetricValue, .unavailable)
        XCTAssertNil(unavailable.pwmHeadroomProgress)
    }

    func testRideStateTreatsMissingPwmHeadroomAsUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .riding)
            )
        )

        XCTAssertEqual(rideState.pwmHeadroomApplicability, .unavailable)
        XCTAssertNil(rideState.pwmHeadroomPermille)
    }

    func testRideStateAccountsForVisibleFieldsInPopulatedLiveSnapshot() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                speed: SpeedReadout(millimetersPerSecond: 1_234),
                telemetry: TelemetrySnapshot(
                    speed: speedValue(1_234),
                    operatingState: .riding,
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(2_000),
                    controllerTemperature: temperatureValue(31_000),
                    pwm: dutyCycle(230),
                    batteryLevelEstimated: batteryLevelValue(80)
                )
            )
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .status), .sessionState)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .speed), .liveTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .updateAge), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .pwmHeadroom), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .sagAdjustedEnergy), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .packVoltage), .liveTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .thermal), .liveTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .warningState), .sessionState)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .voltageSag), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .regenPower), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .limpHomeRange), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .tabs), .staticNavigation)
    }

    func testRideStateRequiresRepresentativeLiveFieldsForValidation() {
        let ready = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    speed: speedValue(1_234),
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(2_000),
                    controllerTemperature: temperatureValue(31_000),
                    pwm: dutyCycle(230)
                )
            )
        )
        let missing = EucRideScreenState(
            phase: .subscribing,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(118_000))
            )
        )

        XCTAssertTrue(ready.isLiveValidationReady)
        XCTAssertEqual(ready.liveValidationMissingFields, [])
        XCTAssertFalse(missing.isLiveValidationReady)
        XCTAssertEqual(
            missing.liveValidationMissingFields,
            [.livePhase, .updateAge, .speed, .power, .pwm, .thermal]
        )
    }

    func testRideStateAccountsForRegenerationPowerOnlyWhenFlowIsRegeneration() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    operatingState: .riding,
                    voltage: voltageValue(96_700),
                    batteryCurrent: batteryCurrentValue(-800),
                    powerFlow: .regeneration
                )
            )
        )

        XCTAssertEqual(rideState.regenerationPower, powerValue(-77_360))
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .derivedTelemetry)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .regenPower), .derivedTelemetry)
    }

    func testRideStateDoesNotAccountForUnverifiedNegativePowerAsRegeneration() {
        let unknownFlowState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(96_700),
                    batteryCurrent: batteryCurrentValue(-800),
                    powerFlow: .negativeUnknown
                )
            )
        )
        let chargingState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(-2_000),
                    powerFlow: .charging
                )
            )
        )

        XCTAssertNil(unknownFlowState.regenerationPower)
        XCTAssertEqual(unknownFlowState.visibleFieldCoverage.source(for: .regenPower), .explicitlyUnavailable)
        XCTAssertNil(chargingState.regenerationPower)
        XCTAssertEqual(chargingState.visibleFieldCoverage.source(for: .regenPower), .explicitlyUnavailable)
    }

    func testRideStateAccountsForVoltageSagAndLimpHomeOnlyWhenTypedValuesExist() {
        let unavailableState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(voltage: voltageValue(96_700))
            )
        )
        let typedState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(96_700),
                    voltageSag: VoltageDelta(value: -1_200),
                    limpHomeRange: Distance(value: 22_852_500)
                )
            )
        )

        XCTAssertNil(unavailableState.voltageSag)
        XCTAssertNil(unavailableState.limpHomeRange)
        XCTAssertEqual(unavailableState.visibleFieldCoverage.source(for: .voltageSag), .explicitlyUnavailable)
        XCTAssertEqual(unavailableState.visibleFieldCoverage.source(for: .limpHomeRange), .explicitlyUnavailable)
        XCTAssertEqual(typedState.voltageSag, VoltageDelta(value: -1_200))
        XCTAssertEqual(typedState.limpHomeRange, Distance(value: 22_852_500))
        XCTAssertEqual(typedState.visibleFieldCoverage.source(for: .voltageSag), .derivedTelemetry)
        XCTAssertEqual(typedState.visibleFieldCoverage.source(for: .limpHomeRange), .derivedTelemetry)
    }

    func testRideStateBuildsControllerOnlyEstimateFromLiveTelemetry() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(117_600),
                    batteryCurrent: batteryCurrentValue(38_000),
                    voltageSag: VoltageDelta(value: 4_800),
                    batteryLevelEstimated: batteryLevelValue(71)
                )
            )
        )

        XCTAssertEqual(rideState.controllerOnlyEstimatePercent, batteryLevelValue(71))
        XCTAssertEqual(rideState.controllerOnlyEstimateDetail, .recentSag)
        XCTAssertEqual(rideState.controllerOnlyConfidence, .medium)
    }

    func testRideStateLowersControllerOnlyEstimateConfidenceWhenSagIsUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(117_600),
                    batteryLevelReported: batteryLevelValue(68)
                )
            )
        )

        XCTAssertEqual(rideState.controllerOnlyEstimatePercent, batteryLevelValue(68))
        XCTAssertEqual(rideState.controllerOnlyEstimateDetail, .voltageCurve)
        XCTAssertEqual(rideState.controllerOnlyConfidence, .low)
    }

    func testRideStateAccountsForParkedPwmAsNotApplicable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(operatingState: .parked, pwm: dutyCycle(0))
            )
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .pwmHeadroom), .notApplicable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .speed), .explicitlyUnavailable)
    }

    func testRideStateAccountsForEmptyLiveSnapshotAsExplicitlyUnavailable() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .speed), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .packVoltage), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .explicitlyUnavailable)
        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .thermal), .explicitlyUnavailable)
    }

    func testRideStateClassifiesUpdateAgeFromMonotonicTimestamp() {
        let missing = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(telemetry: TelemetrySnapshot())
        )
        let fresh = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(1_000))
            )
        )
        let stale = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(lastUpdate: MonotonicMilliseconds(1_000))
        )

        XCTAssertEqual(
            missing.updateAge(at: MonotonicMilliseconds(1_100), staleAfter: MonotonicMilliseconds(250)),
            EucRideUpdateAge(elapsed: nil, freshness: .unavailable)
        )
        XCTAssertEqual(
            fresh.updateAge(at: MonotonicMilliseconds(1_100), staleAfter: MonotonicMilliseconds(250)),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(100), freshness: .fresh)
        )
        XCTAssertEqual(
            stale.updateAge(at: MonotonicMilliseconds(1_300), staleAfter: MonotonicMilliseconds(250)),
            EucRideUpdateAge(elapsed: MonotonicMilliseconds(300), freshness: .stale)
        )
        XCTAssertEqual(fresh.visibleFieldCoverage.source(for: .updateAge), .liveTelemetry)
    }

    func testRideStateUsesTypedStaleWarningWhenTelemetryIsOld() {
        let stale = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(1_000), voltage: voltageValue(118_000)),
                lastUpdate: MonotonicMilliseconds(4_000)
            )
        )
        let fresh = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(at: MonotonicMilliseconds(3_900), voltage: voltageValue(118_000)),
                lastUpdate: MonotonicMilliseconds(4_000)
            )
        )

        XCTAssertEqual(
            stale.warningState(at: MonotonicMilliseconds(4_000), staleAfter: MonotonicMilliseconds(2_000)),
            EucRideWarningState(severity: .caution, title: "Telemetry stale", detail: "Last update 3 seconds ago")
        )
        XCTAssertEqual(
            fresh.warningState(at: MonotonicMilliseconds(4_000), staleAfter: MonotonicMilliseconds(2_000)).severity,
            .normal
        )
    }

    func testRideStatePrefersStaleWarningOverLowPwmHeadroom() {
        let stale = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    at: MonotonicMilliseconds(1_000),
                    operatingState: .riding,
                    pwm: dutyCycle(800)
                )
            )
        )

        XCTAssertEqual(stale.warningState.severity, .reduceAcceleration)
        XCTAssertEqual(
            stale.warningState(at: MonotonicMilliseconds(4_000), staleAfter: MonotonicMilliseconds(2_000)),
            EucRideWarningState(severity: .caution, title: "Telemetry stale", detail: "Last update 3 seconds ago")
        )
    }

    func testRideStateDoesNotClaimDerivedPowerForZeroCurrent() {
        let rideState = EucRideScreenState(
            phase: .live,
            displayState: RideDisplayState(
                telemetry: TelemetrySnapshot(
                    voltage: voltageValue(118_000),
                    batteryCurrent: batteryCurrentValue(0)
                )
            )
        )

        XCTAssertEqual(rideState.visibleFieldCoverage.source(for: .power), .explicitlyUnavailable)
    }

    func testDisplayStateProvidesDebugRowsForLiveValidation() {
        let displayState = RideDisplayState(
            speed: SpeedReadout(millimetersPerSecond: 1_234),
            notificationCount: 7,
            lastUpdate: MonotonicMilliseconds(9_876)
        )

        XCTAssertEqual(
            displayState.debugRows,
            [
                SessionDebugRow(
                    id: "Notifications",
                    label: "Notifications",
                    metricValue: .status(display: "7", accessibility: "7")
                ),
                SessionDebugRow(
                    id: "Last update",
                    label: "Last update",
                    metricValue: .status(display: "9876 ms", accessibility: "9876 ms")
                ),
            ]
        )
    }

    private var scriptedVescCandidate: DevicePickerDiscoveryCandidate {
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "scripted-vesc",
            displayName: "Scripted VESC",
            productCategory: "VESC Onewheel",
            evidence: "test script",
            detail: "core callback fixture",
            support: .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil),
            symbolName: "circle.hexagongrid.circle"
        )
    }

    private var scriptedProbeCandidate: DevicePickerDiscoveryCandidate {
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "scripted-probe",
            displayName: "Unknown EUC",
            productCategory: "Electric unicycle",
            evidence: "test script",
            detail: "identification required",
            support: .probeRecommended(disabledReason: "Identity probe required"),
            symbolName: "magnifyingglass"
        )
    }
}

private func assertVescTelemetryRequests(
    _ operations: [CoreBluetoothPlannedOperation],
    includesSubscribe: Bool,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    let expectedWriteCount = 3
    XCTAssertEqual(operations.count, expectedWriteCount + (includesSubscribe ? 1 : 0), file: file, line: line)
    if includesSubscribe {
        XCTAssertTrue(
            operations.contains(.subscribe(channel: .vescNordicUartNotify)),
            file: file,
            line: line
        )
    }
    let writes = operations.compactMap { operation -> Data? in
        guard case .writeWithoutResponse(channel: .vescNordicUartWrite, bytes: let bytes) = operation else {
            return nil
        }
        return bytes
    }
    XCTAssertEqual(writes.count, expectedWriteCount, file: file, line: line)
    XCTAssertTrue(writes.first.map { isRefloatRequest($0, command: 32) } ?? false, file: file, line: line)
    XCTAssertEqual(writes[1], Data([2, 1, 14, 225, 206, 3]), file: file, line: line)
    XCTAssertEqual(writes[2], Data([2, 1, 4, 64, 132, 3]), file: file, line: line)
}

private func isRefloatRequest(_ bytes: Data, command: UInt8) -> Bool {
    bytes.count >= 7
        && bytes.first == 0x02
        && bytes.last == 0x03
        && bytes[bytes.index(bytes.startIndex, offsetBy: 2)] == 36
        && bytes[bytes.index(bytes.startIndex, offsetBy: 3)] == 101
        && bytes[bytes.index(bytes.startIndex, offsetBy: 4)] == command
}

private final class RecordingOperationSink: CoreBluetoothOperationSink {
    enum Event: Equatable {
        case subscribe
        case write
    }

    private let lock = NSLock()
    private let writesChanged = NSCondition()
    private var recordedWrites: [Data] = []
    private var recordedEvents: [Event] = []

    var writes: [Data] {
        lock.lock()
        defer { lock.unlock() }
        return recordedWrites
    }

    var events: [Event] {
        lock.lock()
        defer { lock.unlock() }
        return recordedEvents
    }

    func subscribe(channel: BluetoothUuid) {
        lock.lock()
        defer { lock.unlock() }
        recordedEvents.append(.subscribe)
    }

    func writeWithoutResponse(channel: BluetoothUuid, bytes: Data) {
        lock.lock()
        recordedWrites.append(bytes)
        recordedEvents.append(.write)
        lock.unlock()

        writesChanged.lock()
        writesChanged.broadcast()
        writesChanged.unlock()
    }

    func disconnect() {}

    func waitForWrites(_ expectedCount: Int, timeout: TimeInterval) -> Bool {
        writesChanged.lock()
        defer { writesChanged.unlock() }

        let deadline = Date().addingTimeInterval(timeout)
        while writes.count < expectedCount {
            guard writesChanged.wait(until: deadline) else { return false }
        }
        return true
    }
}

private func waitForWrites(_ expectedCount: Int, in sink: RecordingOperationSink) {
    XCTAssertTrue(
        sink.waitForWrites(expectedCount, timeout: 1.5),
        "operation sink did not reach \(expectedCount) writes"
    )
}

private func speedValue(_ value: Int32) -> Speed {
    Speed(value: value)
}

private func voltageValue(_ value: Int32) -> Voltage {
    Voltage(value: value)
}

private func batteryCurrentValue(_ value: Int32) -> BatteryCurrent {
    BatteryCurrent(value: value)
}

private func phaseCurrentValue(_ value: Int32) -> PhaseCurrent {
    PhaseCurrent(value: value)
}

private func powerValue(_ value: Int64) -> Power {
    Power(value: value)
}

private func temperatureValue(_ value: Int32) -> Temperature {
    Temperature(value: value)
}

private func angleValue(_ value: Int32) -> Angle {
    Angle(value: value)
}

private func batteryLevelValue(_ value: UInt8) -> BatteryLevel {
    BatteryLevel(value: value)
}

private func dutyCycle(_ permille: Int16) -> DutyCycle {
    DutyCycle(permille: permille)
}

private final class TestMonotonicClock {
    var now: MonotonicMilliseconds

    init(_ now: MonotonicMilliseconds) {
        self.now = now
    }
}

private extension [EucRideVisibleFieldCoverage] {
    func source(for field: EucRideVisibleField) -> EucRideVisibleFieldSource? {
        first { $0.field == field }?.source
    }
}

private final class RecordingReconnectScheduler: ConnectionReconnectScheduling {
    private final class Token: ConnectionReconnectCancellable {
        var isCancelled = false

        func cancel() {
            isCancelled = true
        }
    }

    private var scheduled: [(token: Token, operation: () -> Void)] = []

    func schedule(after _: UInt64, operation: @escaping () -> Void) -> any ConnectionReconnectCancellable {
        let token = Token()
        scheduled.append((token, operation))
        return token
    }

    func runAll() {
        let scheduled = scheduled
        self.scheduled.removeAll()
        for entry in scheduled where !entry.token.isCancelled {
            entry.operation()
        }
    }
}

private final class RecordingRideMapPollScheduler: RideMapWritePollingScheduling {
    private var scheduled: [() -> Void] = []

    var scheduledCount: Int { scheduled.count }
    private(set) var runCount = 0

    func schedule(after _: UInt64, operation: @escaping () -> Void) {
        scheduled.append(operation)
    }

    @discardableResult
    func runNext() -> Bool {
        guard scheduled.isEmpty == false else { return false }
        runCount += 1
        let operation = scheduled.removeFirst()
        operation()
        return true
    }
}
