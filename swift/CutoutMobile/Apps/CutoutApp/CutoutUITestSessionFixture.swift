import CutoutMobile
import CutoutMobileFFI

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
        case .euc, .dynamicEuc, .staleEuc, .reconnectingEuc, .connectingEuc, .eucOverview, .eucNoBms, .eucUnknownTopology:
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
        case .bluetoothUnavailable, .bluetoothPermissionDenied, .vesc, .dynamicVesc, .warningVesc, .stopVesc, .operatingModeVesc, .pendingVesc, .staleVesc, .failedVesc, .reconnectingVesc, .bluetoothLossVesc, .connectingVesc, .autoVescLiveActivity, .autoDynamicVescLiveActivity, .autoCriticalVescLiveActivity, .autoUnavailableVescLiveActivity, .autoStaleVescLiveActivity:
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

    var testScript: CutoutSessionTestScript {
        let telemetryUpdate = refreshesVescSafetyState ? telemetry : dynamicTelemetryUpdate
        return CutoutSessionTestScript(
            candidate: candidate,
            telemetry: emitsPendingTelemetry ? nil : telemetry,
            telemetryUpdate: telemetryUpdate,
            telemetryUpdateDelayMilliseconds: telemetryUpdateDelayMilliseconds,
            bmsSnapshot: testBmsSnapshot,
            startsLive: startsLive,
            initialBluetoothState: initialBluetoothState,
            failsConnection: failsConnection,
            identificationProbeFailure: identificationProbeFailure,
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
        makeEucBmsSnapshot(groups: [
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
            )
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

#endif
