import CutoutMobile
import CutoutMobileFFI
import Foundation

#if DEBUG
    enum CutoutUITestSessionFixture: Equatable {
        case unknownDevice
        case unknownDeviceFinishFailure
        case probeDevice
        case probeTimeout
        case probeMalformedResponse
        case probeConflictingEvidence
        case probeUnsupported
        case bluetoothUnavailable
        case bluetoothPermissionDenied
        case vesc
        case dynamicVesc
        case warningVesc(VescRideWarning)
        case stopVesc(VescRideStopReason)
        case operatingModeVesc(VescRideOperatingMode)
        case pendingVesc
        case staleVesc
        case failedVesc
        case reconnectingVesc
        case bluetoothLossVesc
        case connectingVesc
        case euc
        case dynamicEuc
        case staleEuc
        case reconnectingEuc
        case connectingEuc
        case eucOverview
        case eucNoBms
        case eucUnknownTopology
        case autoVescLiveActivity
        case autoDynamicVescLiveActivity
        case autoCriticalVescLiveActivity
        case autoUnavailableVescLiveActivity
        case autoStaleVescLiveActivity

        init?(value: String?) {
            switch value {
            case "unknown-device": self = .unknownDevice
            case "unknown-device-finish-failure": self = .unknownDeviceFinishFailure
            case "probe-device": self = .probeDevice
            case "probe-timeout": self = .probeTimeout
            case "probe-malformed": self = .probeMalformedResponse
            case "probe-conflict": self = .probeConflictingEvidence
            case "probe-unsupported": self = .probeUnsupported
            case "bluetooth-unavailable": self = .bluetoothUnavailable
            case "bluetooth-permission-denied": self = .bluetoothPermissionDenied
            case "vesc": self = .vesc
            case "vesc-dynamic": self = .dynamicVesc
            case "vesc-low-voltage": self = .warningVesc(.lowVoltage)
            case "vesc-high-voltage": self = .warningVesc(.highVoltage)
            case "vesc-mosfet-temperature": self = .warningVesc(.mosfetTemperature)
            case "vesc-motor-temperature": self = .warningVesc(.motorTemperature)
            case "vesc-current": self = .warningVesc(.current)
            case "vesc-duty-pushback": self = .warningVesc(.dutyPushback)
            case "vesc-temperature-pushback": self = .warningVesc(.temperaturePushback)
            case "vesc-wheelslip": self = .warningVesc(.wheelslip)
            case "vesc-sensors": self = .warningVesc(.sensors)
            case "vesc-low-battery": self = .warningVesc(.lowBattery)
            case "vesc-error": self = .warningVesc(.error)
            case "vesc-pitch-stop": self = .stopVesc(.pitch)
            case "vesc-roll-stop": self = .stopVesc(.roll)
            case "vesc-switch-half-stop": self = .stopVesc(.switchHalf)
            case "vesc-switch-full-stop": self = .stopVesc(.switchFull)
            case "vesc-reverse-stop": self = .stopVesc(.reverse)
            case "vesc-quick-stop": self = .stopVesc(.quickStop)
            case "vesc-handtest": self = .operatingModeVesc(.handtest)
            case "vesc-darkride": self = .operatingModeVesc(.darkride)
            case "vesc-flywheel": self = .operatingModeVesc(.flywheel)
            case "vesc-pending": self = .pendingVesc
            case "vesc-stale": self = .staleVesc
            case "vesc-failure": self = .failedVesc
            case "vesc-reconnect": self = .reconnectingVesc
            case "vesc-bluetooth-loss": self = .bluetoothLossVesc
            case "vesc-connecting": self = .connectingVesc
            case "euc": self = .euc
            case "euc-dynamic": self = .dynamicEuc
            case "euc-stale": self = .staleEuc
            case "euc-reconnect": self = .reconnectingEuc
            case "euc-connecting": self = .connectingEuc
            case "euc-overview": self = .eucOverview
            case "euc-no-bms": self = .eucNoBms
            case "euc-unknown-topology": self = .eucUnknownTopology
            case "vesc-live-activity-auto": self = .autoVescLiveActivity
            case "vesc-live-activity-dynamic-auto": self = .autoDynamicVescLiveActivity
            case "vesc-live-activity-critical-auto": self = .autoCriticalVescLiveActivity
            case "vesc-live-activity-unavailable-auto": self = .autoUnavailableVescLiveActivity
            case "vesc-live-activity-stale-auto": self = .autoStaleVescLiveActivity
            default: return nil
            }
        }

        init?(arguments: [String]) {
            guard let value = Self.standardLaunchArgumentValue(arguments), let fixture = Self(value: value) else {
                return nil
            }
            self = fixture
        }

        static func resolve(
            environmentValue: String? = nil,
            persistedValue: String?,
            arguments: [String]
        ) -> Self? {
            Self(value: environmentValue)
                ?? Self(arguments: arguments)
                ?? Self(value: persistedValue)
        }

        private static func standardLaunchArgumentValue(_ arguments: [String]) -> String? {
            guard let keyIndex = arguments.firstIndex(of: "-CUTOUT_UI_TEST_FIXTURE") else { return nil }
            let valueIndex = arguments.index(after: keyIndex)
            guard valueIndex < arguments.endIndex else { return nil }
            return arguments[valueIndex]
        }

        var candidate: DevicePickerDiscoveryCandidate {
            switch self {
            case .unknownDevice, .unknownDeviceFinishFailure:
                DevicePickerDiscoveryCandidate(
                    platformIdentifier: "ui-test-unknown-device",
                    displayName: "Unknown BLE device",
                    productCategory: "Unknown personal electric vehicle",
                    evidence: "UI test fixture",
                    detail: "Deterministic record-only capture device",
                    support: .unknownRecordable(disabledReason: "Unknown device fixture"),
                    symbolName: "questionmark.circle"
                )
            case .probeDevice, .probeTimeout, .probeMalformedResponse, .probeConflictingEvidence, .probeUnsupported:
                DevicePickerDiscoveryCandidate(
                    platformIdentifier: "ui-test-probe",
                    displayName: "Unknown EUC",
                    productCategory: "Electric unicycle",
                    evidence: "UI test fixture",
                    detail: "Deterministic identification probe device",
                    support: .probeRecommended(disabledReason: "Identity probe required"),
                    symbolName: "magnifyingglass"
                )
            case .euc, .dynamicEuc, .staleEuc, .reconnectingEuc, .connectingEuc, .eucOverview, .eucNoBms,
                .eucUnknownTopology:
                DevicePickerDiscoveryCandidate(
                    platformIdentifier: "ui-test-euc",
                    displayName: "Test EUC",
                    productCategory: "Electric unicycle",
                    evidence: "UI test fixture",
                    detail: "Deterministic accessibility test device",
                    support: .supported(
                        connectionRoute: .electricUnicycle,
                        electricUnicycleModel: .aero
                    ),
                    symbolName: "circle.hexagongrid.circle"
                )
            case .bluetoothUnavailable, .bluetoothPermissionDenied, .vesc, .dynamicVesc, .warningVesc, .stopVesc,
                .operatingModeVesc, .pendingVesc, .staleVesc, .failedVesc, .reconnectingVesc, .bluetoothLossVesc,
                .connectingVesc, .autoVescLiveActivity, .autoDynamicVescLiveActivity, .autoCriticalVescLiveActivity,
                .autoUnavailableVescLiveActivity, .autoStaleVescLiveActivity:
                DevicePickerDiscoveryCandidate(
                    platformIdentifier: "ui-test-vesc",
                    displayName: "Refloat VESC",
                    productCategory: "VESC Onewheel",
                    evidence: "UI test fixture",
                    detail: "Deterministic accessibility test device",
                    support: .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil),
                    symbolName: "circle.hexagongrid.circle"
                )
            }
        }

        var startsLive: Bool {
            self == .autoVescLiveActivity
                || self == .autoDynamicVescLiveActivity
                || self == .autoCriticalVescLiveActivity
                || self == .autoUnavailableVescLiveActivity
                || self == .autoStaleVescLiveActivity
        }
        var initialBluetoothState: CutoutSessionTestInitialBluetoothState {
            switch self {
            case .bluetoothUnavailable: .unavailable
            case .bluetoothPermissionDenied: .permissionDenied
            default: .scanning
            }
        }
        var failsConnection: Bool { self == .failedVesc }
        var identificationProbeFailure: IdentificationProbeFailure? {
            switch self {
            case .probeTimeout: .timedOut
            case .probeMalformedResponse: .malformedResponse
            case .probeConflictingEvidence: .conflictingEvidence
            case .probeUnsupported: .unsupported
            default: nil
            }
        }
        var detectedSupport: DevicePickerCandidateSupport? {
            switch self {
            case .probeDevice, .probeTimeout, .probeMalformedResponse, .probeConflictingEvidence, .probeUnsupported:
                .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero)
            default:
                nil
            }
        }
        var reconnectsAfterFirstLive: Bool { self == .reconnectingVesc || self == .reconnectingEuc }
        var emitsPendingTelemetry: Bool {
            self == .pendingVesc || self == .autoUnavailableVescLiveActivity
        }
        var emitsStaleTelemetry: Bool {
            self == .staleVesc || self == .staleEuc || self == .autoStaleVescLiveActivity
        }
        var flushCaptureSucceeds: Bool { self != .unknownDeviceFinishFailure }
        var isEuc: Bool {
            self == .probeDevice
                || self == .probeTimeout
                || self == .probeMalformedResponse
                || self == .probeConflictingEvidence
                || self == .probeUnsupported
                || self == .euc
                || self == .dynamicEuc
                || self == .staleEuc
                || self == .reconnectingEuc
                || self == .connectingEuc
                || self == .eucOverview
                || self == .eucNoBms
                || self == .eucUnknownTopology
        }

        private var refreshesVescSafetyState: Bool {
            testVescWarning != nil || testVescStopReason != nil
        }

        private var testVescWarning: VescRideWarning? {
            switch self {
            case .warningVesc(let warning): warning
            case .stopVesc: VescRideWarning.none
            default: nil
            }
        }

        private var testVescStopReason: VescRideStopReason? {
            switch self {
            case .stopVesc(let stopReason): stopReason
            default: nil
            }
        }

        private var testVescOperatingMode: VescRideOperatingMode? {
            switch self {
            case .operatingModeVesc(let operatingMode): operatingMode
            default: nil
            }
        }

        private var testBmsSnapshot: BmsSnapshot? {
            switch self {
            case .euc: eucBmsSnapshot
            case .eucOverview: eucBmsOverviewSnapshot
            case .eucUnknownTopology: eucUnknownTopologyBmsSnapshot
            default: nil
            }
        }

        /// Validated identity packets shared with the Rust device-session tests.
        private var protocolNotifications: [Data] {
            if isEuc {
                var frame = Data(repeating: 0, count: 42)
                frame.replaceSubrange(0..<4, with: [0xdc, 0x5a, 0x5c, 38])
                frame.replaceSubrange(28..<30, with: [0xa7, 0xf8])
                return [frame]
            }
            return [
                Data([
                    2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101,
                    114, 104, 97, 115, 104, 0, 38, 208, 3,
                ])
            ]
        }

        var testScript: CutoutSessionTestScript {
            let telemetryUpdate = refreshesVescSafetyState ? telemetry : dynamicTelemetryUpdate
            return CutoutSessionTestScript(
                candidate: candidate,
                telemetry: emitsPendingTelemetry ? nil : telemetry,
                protocolNotifications: protocolNotifications,
                protocolNotificationIntervalMilliseconds: isEuc
                    && CommandLine.arguments.contains("-CUTOUT_UI_TEST_SETTINGS") ? 500 : nil,
                telemetryUpdate: telemetryUpdate,
                telemetryUpdateDelayMilliseconds: telemetryUpdateDelayMilliseconds,
                bmsSnapshot: testBmsSnapshot,
                startsLive: startsLive,
                initialBluetoothState: initialBluetoothState,
                failsConnection: failsConnection,
                identificationProbeFailure: identificationProbeFailure,
                detectedSupport: detectedSupport,
                emitsLateLiveAfterFailure: failsConnection,
                reconnectsAfterFirstLive: reconnectsAfterFirstLive,
                reconnectAfterLiveMilliseconds: reconnectsAfterFirstLive ? 1_500 : 0,
                reconnectDelayMilliseconds: reconnectsAfterFirstLive ? 5_000 : 0,
                bluetoothLossAfterFirstLiveMilliseconds: self == .bluetoothLossVesc ? 1_500 : nil,
                emitsStaleTelemetry: emitsStaleTelemetry,
                flushCaptureSucceeds: flushCaptureSucceeds,
                connectionDelayMilliseconds: startsLive ? 0 : (failsConnection ? 3_000 : connectingDelayMilliseconds)
            )
        }

        private var connectingDelayMilliseconds: UInt64 {
            switch self {
            case .connectingVesc, .connectingEuc: 5_000
            default: 1_000
            }
        }

        private var telemetryUpdateDelayMilliseconds: UInt64 {
            guard dynamicTelemetryUpdate != nil || refreshesVescSafetyState else { return 0 }
            return self == .autoDynamicVescLiveActivity ? 8_000 : 1_500
        }

        private var telemetry: TelemetrySnapshot {
            if isEuc {
                return TelemetrySnapshot(
                    speed: Speed(value: 12_000),
                    speedSource: .reported,
                    speedQuality: .known,
                    operatingState: .riding,
                    voltage: Voltage(value: 82_000),
                    batteryCurrent: BatteryCurrent(value: 8_000),
                    controllerTemperature: Temperature(value: 31_000),
                    batteryLevelReported: BatteryLevel(value: 64)
                )
            }
            return TelemetrySnapshot(
                speed: Speed(value: 8_000),
                speedSource: .reported,
                speedQuality: .known,
                operatingState: .riding,
                vescOperatingMode: testVescOperatingMode,
                vescWarning: testVescWarning,
                vescStopReason: testVescStopReason,
                voltage: Voltage(value: 50_400),
                batteryCurrent: BatteryCurrent(value: 12_000),
                controllerTemperature: Temperature(value: 32_000),
                pwm: DutyCycle(permille: self == .autoCriticalVescLiveActivity ? 850 : 230),
                batteryLevelReported: BatteryLevel(value: 72)
            )
        }

        private var dynamicTelemetryUpdate: TelemetrySnapshot? {
            switch self {
            case .dynamicVesc, .autoDynamicVescLiveActivity:
                TelemetrySnapshot(
                    speed: Speed(value: 16_000),
                    speedSource: .reported,
                    speedQuality: .known,
                    operatingState: .riding,
                    voltage: Voltage(value: 62_000),
                    batteryCurrent: BatteryCurrent(value: 12_000),
                    motorCurrent: PhaseCurrent(value: 20_000),
                    controllerTemperature: Temperature(value: 43_000),
                    motorTemperature: Temperature(value: 49_000),
                    pwm: DutyCycle(permille: 720),
                    batteryLevelReported: BatteryLevel(value: 71)
                )
            case .dynamicEuc:
                TelemetrySnapshot(
                    speed: Speed(value: 18_000),
                    speedSource: .reported,
                    speedQuality: .known,
                    operatingState: .riding,
                    voltage: Voltage(value: 80_000),
                    batteryCurrent: BatteryCurrent(value: 10_000),
                    controllerTemperature: Temperature(value: 35_000),
                    batteryLevelReported: BatteryLevel(value: 61)
                )
            default:
                nil
            }
        }

        private var eucBmsSnapshot: BmsSnapshot {
            if let flag = CommandLine.arguments.firstIndex(of: "-CUTOUT_UI_TEST_BMS_COUNT"),
                CommandLine.arguments.indices.contains(flag + 1),
                let count = Int(CommandLine.arguments[flag + 1]), count > 0
            {
                return BmsSnapshot(
                    topology: BmsTopology(
                        layoutLabel: "15 observed BMS groups", seriesGroupCount: nil, parallelCount: nil, packCount: 1,
                        bmsCount: 1, confidence: .unverified),
                    pageSelector: 6,
                    cellDelta: VoltageDelta(value: 16),
                    lowestGroupIndex: 1,
                    observedGroupCount: count,
                    highestGroupIndex: count,
                    highestTemperature: Temperature(value: 21_800),
                    temperatureReadings: [21_800, 21_600, 21_500, 21_700, 21_600, 21_800].map {
                        Temperature(value: $0)
                    },
                    groups: (1...count).map { index in
                        BmsGroupSnapshot(
                            index: index,
                            label: "page \((index - 1) / 15 + 1) · group \((index - 1) % 15 + 1)",
                            voltage: Voltage(
                                value: index == 1
                                    ? 4_177
                                    : (index == count
                                        ? 4_193 : (index <= 15 || (31...45).contains(index) ? 4_181 : 4_192)))
                        )
                    }
                )
            }
            return makeEucBmsSnapshot(groups: [
                BmsGroupSnapshot(
                    index: 7,
                    label: "right pack group 7",
                    voltage: Voltage(value: 4_036),
                    temperature: Temperature(value: 38_000),
                    isBalancing: true,
                    alertLevel: .warning,
                    detail: "lowest group"
                ),
                BmsGroupSnapshot(
                    index: 12,
                    label: "right pack group 12",
                    voltage: Voltage(value: 4_060),
                    temperature: Temperature(value: 34_000),
                    isBalancing: true,
                    alertLevel: .nominal
                ),
            ])
        }

        private var eucBmsOverviewSnapshot: BmsSnapshot {
            makeEucBmsSnapshot(groups: [])
        }

        private func makeEucBmsSnapshot(groups: [BmsGroupSnapshot]) -> BmsSnapshot {
            BmsSnapshot(
                topology: BmsTopology(
                    layoutLabel: "20S4P test pack",
                    seriesGroupCount: 20,
                    parallelCount: 4,
                    packCount: 1,
                    bmsCount: 1,
                    confidence: .verified
                ),
                pageKind: "overview",
                pageVerification: .hardwareVerified,
                energyPercent: BatteryLevel(value: 64),
                voltage: Voltage(value: 82_000),
                current: BatteryCurrent(value: 8_000),
                cellDelta: VoltageDelta(value: 24),
                lowestGroupIndex: 7,
                observedGroupCount: 2,
                highestGroupIndex: 12,
                highestTemperature: Temperature(value: 38_000),
                temperatureReadings: [Temperature(value: 38_000), Temperature(value: 34_000)],
                highestTemperatureLabel: "right pack",
                balancingSummary: "balancing 2 groups",
                balancingDetail: "groups 7 and 12",
                faultSummary: "no active faults",
                faultDetail: "last fault unavailable",
                groups: groups
            )
        }

        private var eucUnknownTopologyBmsSnapshot: BmsSnapshot {
            BmsSnapshot(
                topology: BmsTopology(
                    layoutLabel: "topology unverified",
                    seriesGroupCount: nil,
                    parallelCount: nil,
                    packCount: 1,
                    bmsCount: 1,
                    confidence: .unverified
                ),
                voltage: Voltage(value: 82_000),
                faultSummary: "BMS found, map unknown",
                faultDetail: "Awaiting a verified topology.",
                faults: [BmsFault(code: "0x0040", label: "needs decoder", level: .warning)],
                captureActionTitle: "Record unsupported pack",
                captureActionState: "disabled for launch"
            )
        }
    }

    /// Drives the real app-owned music monitor and command path without an installed music app.
    @MainActor
    final class CutoutUITestMusicMonitor: AppleMusicMonitorDriving {
        private var state: MobileMusicPlaybackStateDto
        private var trackNumber = 1
        private let emitsObservations: Bool
        private let previousOnly: Bool
        private var onObservation: (@MainActor (MusicProviderObservation) -> Void)?

        private init(
            state: MobileMusicPlaybackStateDto, emitsObservations: Bool = true, previousOnly: Bool = false
        ) {
            self.state = state
            self.emitsObservations = emitsObservations
            self.previousOnly = previousOnly
        }

        static func resolve() -> CutoutUITestMusicMonitor? {
            let value = UserDefaults.standard.string(forKey: "CUTOUT_UI_TEST_MUSIC")
            return switch value {
            case "playing": CutoutUITestMusicMonitor(state: .playing)
            case "paused": CutoutUITestMusicMonitor(state: .paused)
            case "recovery": CutoutUITestMusicMonitor(state: .stale)
            case "silent": CutoutUITestMusicMonitor(state: .stopped, emitsObservations: false)
            case "previous-only": CutoutUITestMusicMonitor(state: .playing, previousOnly: true)
            default: nil
            }
        }

        func requestAuthorization(allowPrompt: Bool) async -> Bool { true }

        func unauthorizedSnapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
            snapshot(observedAtMs: observedAtMs)
        }

        func startMonitoring(
            observedAtMs: @escaping @MainActor () -> UInt64,
            onObservation: @escaping @MainActor (MusicProviderObservation) -> Void
        ) async {
            self.onObservation = onObservation
            refreshObservation(observedAtMs: observedAtMs())
        }

        func stopMonitoring() { onObservation = nil }
        func applySuspension(_ suspension: MobileMusicProviderSuspension) {}

        func refreshObservation(observedAtMs: UInt64) {
            guard emitsObservations else { return }
            onObservation?(MusicProviderObservation(snapshot: snapshot(observedAtMs: observedAtMs)))
        }

        func perform(_ command: MobileMusicCommandDto) -> MusicCommandOutcome {
            switch command {
            case .play: state = .playing
            case .pause: state = .paused
            case .next: trackNumber += 1
            case .previous: trackNumber = max(1, trackNumber - 1)
            case .openProvider: return .unavailable
            }
            return .accepted
        }

        private func snapshot(observedAtMs: UInt64) -> MobileMusicSnapshotDto {
            MobileMusicSnapshotDto(
                provider: .appleMusic,
                sessionId: "ui-test-music",
                state: state,
                item: .init(
                    identifier: "ui-test-track-\(trackNumber)",
                    title: "Everything In Its Right Place \(trackNumber)",
                    artist: "Radiohead"
                ),
                positionMilliseconds: 45_000,
                durationMilliseconds: 240_000,
                observedAtMs: observedAtMs,
                capabilities: state == .stale || previousOnly
                    ? .init(previous: previousOnly, play: false, pause: false, next: false, openProvider: true)
                    : .init(previous: true, play: true, pause: true, next: true, openProvider: false)
            )
        }
    }
    /// Opt-in Simulator data for exercising the real saved-history queries and navigation.
    /// Rust creates IDs, admits samples, computes summaries, and applies lifecycle transitions.
    enum CutoutUITestSavedRideFixture {
        private static let receiptKey = "io.cutout.ui-test.saved-ride-receipts.v2"

        private static var isEnabled: Bool {
            #if os(iOS) && targetEnvironment(simulator)
                CommandLine.arguments.contains("--seed-ui-test-ride-history")
                    && CutoutUITestSessionFixture(arguments: CommandLine.arguments) != nil
            #else
                false
            #endif
        }

        static var accessibilityValue: String? {
            guard isEnabled, let ids = UserDefaults.standard.stringArray(forKey: receiptKey) else { return nil }
            return "saved-rides:\(ids.joined(separator: ","))"
        }

        static func runIfRequested(database: RideDatabaseHandle) async throws {
            guard isEnabled else { return }
            try await Task.detached(priority: .utility) {
                let nowWall = UInt64(Date().timeIntervalSince1970 * 1_000)
                let nowMonotonic = UInt64(ProcessInfo.processInfo.systemUptime * 1_000)
                let recentWindow = MobileRideMapLimits.rustOwned.historyRecentWindowMilliseconds
                let cutoff = nowWall > recentWindow ? nowWall - recentWindow : 0
                let defaults = UserDefaults.standard
                if let existing = defaults.stringArray(forKey: receiptKey), existing.count == 2,
                    Set(existing).count == 2
                {
                    let records = try existing.compactMap { value -> MobileRideRecordDto? in
                        guard let uuid = UUID(uuidString: value)?.uuid else { return nil }
                        let id = MobileRideIdDto(bytes: withUnsafeBytes(of: uuid) { Data($0) })
                        return try database.findRide(rideId: id)
                    }
                    if records.count == 2,
                        records.allSatisfy({
                            $0.state == .saved && $0.summary.pointCount == 3
                                && $0.createdAtMilliseconds >= cutoff && $0.createdAtMilliseconds <= nowWall
                        })
                    {
                        return
                    }
                }
                let routeCount = Int(MobileRideMapLimits.rustOwned.historyPageLimit) + 1
                let totalOffset = UInt64(routeCount + 1) * 3_000
                guard nowWall > totalOffset, nowMonotonic > totalOffset else {
                    throw CocoaError(.coderInvalidValue)
                }
                var receipts: [String] = []
                for index in 0..<routeCount {
                    try Task.checkCancellation()
                    let offset = UInt64(index) * 3_000
                    let startedMonotonic = nowMonotonic - totalOffset + offset
                    let startedWall = nowWall - totalOffset + offset
                    let id = try database.createRideWithMonotonicStart(
                        source: .live,
                        createdAtMilliseconds: startedWall,
                        monotonicCreatedAtMilliseconds: startedMonotonic
                    )
                    _ = try database.transition(id: id, event: .start, monotonicAtMilliseconds: startedMonotonic)
                    for point in 0..<3 {
                        let elapsed = UInt64(point) * 1_000
                        let admitted = try database.appendLocation(
                            id: id,
                            location: MobileRideLocationDto(
                                latitudeDegrees: 39.7 + Double(index) * 0.01 + Double(point) * 0.0001,
                                longitudeDegrees: -104.9,
                                monotonicMilliseconds: startedMonotonic + elapsed,
                                wallClockUnixMilliseconds: startedWall + elapsed,
                                horizontalAccuracyMillimetres: 3_000,
                                source: .live
                            )
                        )
                        guard admitted == .accepted else { throw CocoaError(.coderInvalidValue) }
                    }
                    _ = try database.transition(id: id, event: .stop, monotonicAtMilliseconds: startedMonotonic + 2_000)
                    _ = try database.transition(id: id, event: .save, monotonicAtMilliseconds: startedMonotonic + 2_000)
                    guard id.bytes.count == 16,
                        let record = try database.findRide(rideId: id), record.state == .saved,
                        record.summary.pointCount == 3
                    else { throw CocoaError(.coderInvalidValue) }
                    if index == 0 || index == routeCount - 1 {
                        receipts.append(NSUUID(uuidBytes: [UInt8](id.bytes)).uuidString.lowercased())
                    }
                }
                // Reuse the same validated Rust receipts when this explicit fixture relaunches.
                defaults.set(receipts, forKey: receiptKey)
            }.value
        }
    }
#endif

#if DEBUG && targetEnvironment(simulator)
    /// Representative typed view input, enabled only by the explicit secondary-readings UI test flag.
    /// It exercises rendering and accessibility; it does not simulate physical telemetry or GPS acquisition.
    enum CutoutUITestSecondaryRideFixture {
        private static var fixture: CutoutUITestSessionFixture? {
            guard CommandLine.arguments.contains("--ui-test-ride-secondary-readings") else { return nil }
            return CutoutUITestSessionFixture.resolve(
                environmentValue: ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_FIXTURE"],
                persistedValue: UserDefaults.standard.string(forKey: "CUTOUT_UI_TEST_FIXTURE"),
                arguments: CommandLine.arguments
            )
        }

        static func eucRideState(at now: MonotonicMilliseconds) -> EucRideScreenState? {
            guard fixture?.isEuc == true else { return nil }
            let estimate = MobileChargeTimeEstimateDto(
                lower: MobileDurationDto(milliseconds: 90_000),
                expected: MobileDurationDto(milliseconds: 120_000),
                upper: MobileDurationDto(milliseconds: 180_000),
                kind: .atPresentCurrent,
                confidence: .medium,
                currentRate: MobileCurrentRateSummaryDto(
                    meanMilliamps: 2_000, minimumMilliamps: 1_900,
                    maximumMilliamps: 2_100, variabilityPermille: 100
                ),
                batteryLevel: BatteryLevelReading(
                    value: BatteryLevel(value: 64), source: .reported,
                    quality: .known, verification: .unverified
                ),
                batteryLevelBasis: .reported,
                batteryProfileId: nil,
                capacitySource: .estimated,
                voltageSag: nil,
                calculatedAt: MobileMonotonicMillisDto(milliseconds: now.rawValue),
                validUntil: MobileMonotonicMillisDto(milliseconds: now.rawValue + 60_000)
            )
            let telemetry = TelemetrySnapshot(
                at: now,
                speed: Speed(value: 0),
                operatingState: .charging,
                voltage: Voltage(value: 82_000),
                batteryCurrent: BatteryCurrent(value: 2_000),
                power: Power(value: 164_000),
                powerFlow: .charging,
                controllerTemperature: Temperature(value: 31_000),
                pwm: DutyCycle(permille: 0),
                limpHomeRange: Distance(value: 22_852_500),
                batteryLevelReported: BatteryLevel(value: 64),
                chargeEstimate: .withPresentationFixture(
                    MobileChargeEstimateStateDto(
                        kind: .available, estimate: estimate, voltageSag: nil,
                        unavailableReason: nil, error: nil, resetReason: nil,
                        samples: 5, observedFor: MobileDurationDto(milliseconds: 30_000)
                    ))
            )
            return EucRideScreenState(
                phase: .live,
                displayState: RideDisplayState(
                    speed: SpeedReadout(snapshot: telemetry), telemetry: telemetry, lastUpdate: now
                )
            )
        }

        static func phoneLocationReadback(at now: MonotonicMilliseconds) -> PhoneLocationReadback? {
            guard fixture?.isEuc == true else { return nil }
            let sourceTime = Date().timeIntervalSince1970
            let state = MobilePhoneLocationState()
            _ = state.ingest(
                sample: MobilePhoneLocationSampleDto(
                    wallClockUnixMs: UInt64(sourceTime * 1_000),
                    sourceTimestampUnixSeconds: sourceTime,
                    latitudeDegrees: 39.7, longitudeDegrees: -104.9,
                    altitudeMeters: 1_600, horizontalAccuracyMeters: 4,
                    verticalAccuracyMeters: 6, speedMetersPerSecond: 3,
                    speedAccuracyMetersPerSecond: 0.2, courseDegrees: 90,
                    courseAccuracyDegrees: 3
                ))
            return PhoneLocationReadback(snapshot: state.currentSnapshot(), receivedAt: now)
        }

        static func vescRideSnapshot(at now: MonotonicMilliseconds) -> VescRideSnapshot? {
            guard let fixture, !fixture.isEuc else { return nil }
            return VescRideSnapshot(
                title: "VESC", vehicleKind: .float, subProtocol: .generic,
                controllerState: .unknown, operatingState: .riding, warning: .none,
                boardSpeed: Speed(value: 8_000), dutyCycle: DutyCycle(permille: 230),
                dutyHeadroom: BatteryLevel(value: 77),
                batteryVoltage: Voltage(value: 50_400),
                batteryLevelReported: BatteryLevel(value: 72),
                batteryCurrent: BatteryCurrent(value: 12_000),
                motorCurrent: PhaseCurrent(value: 5_000),
                boardAngle: Angle(value: 1_500),
                controllerTemperature: Temperature(value: 32_000),
                motorTemperature: Temperature(value: 28_000),
                footpad: FootpadTelemetry(
                    state: 3, contactState: .both,
                    adc1Milliunits: 1_250, adc2Milliunits: 875
                ),
                lastUpdate: now
            )
        }
    }
#endif
