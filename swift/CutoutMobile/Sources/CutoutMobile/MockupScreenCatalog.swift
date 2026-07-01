import Foundation

public enum MockupScreenID: String, CaseIterable, Equatable, Hashable, Sendable {
    case devicePicker
    case eucRide
    case eucGarage
    case vescOnewheelRide
    case vescDebug
}

public struct MockupMetric: Equatable, Hashable, Sendable {
    public let label: String
    public let value: String

    public init(label: String, value: String) {
        self.label = label
        self.value = value
    }
}

public enum MockupAccent: String, Equatable, Hashable, Sendable {
    case cyan
    case green
    case orange
    case yellow
}

public struct MockupSafetyBar: Equatable, Hashable, Sendable {
    public let label: String
    public let value: String
    public let progress: Double
    public let accent: MockupAccent

    public init(label: String, value: String, progress: Double, accent: MockupAccent) {
        self.label = label
        self.value = value
        self.progress = progress
        self.accent = accent
    }
}

public struct MockupWarningCard: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String

    public init(title: String, detail: String) {
        self.title = title
        self.detail = detail
    }
}

public struct MockupDashboardTile: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { label }

    public let label: String
    public let value: String
    public let unit: String
    public let detail: String
    public let accent: MockupAccent

    public init(label: String, value: String, unit: String, detail: String, accent: MockupAccent) {
        self.label = label
        self.value = value
        self.unit = unit
        self.detail = detail
        self.accent = accent
    }
}

public struct MockupScreenTab: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { title }

    public let title: String
    public let isSelected: Bool

    public init(title: String, isSelected: Bool) {
        self.title = title
        self.isSelected = isSelected
    }
}

public enum MockupPickerRowState: Equatable, Hashable, Sendable {
    case supported(action: String)
    case unsupported(action: String)
    case manual(action: String)
}

public struct MockupPickerRow: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { title }

    public let title: String
    public let subtitle: String
    public let detail: String
    public let state: MockupPickerRowState
    public let symbolName: String

    public init(
        title: String,
        subtitle: String,
        detail: String,
        state: MockupPickerRowState,
        symbolName: String
    ) {
        self.title = title
        self.subtitle = subtitle
        self.detail = detail
        self.state = state
        self.symbolName = symbolName
    }
}

public extension MockupPickerRow {
    var isSupported: Bool {
        if case .supported = state { true } else { false }
    }

    var isUnsupported: Bool {
        if case .unsupported = state { true } else { false }
    }

    var isManual: Bool {
        if case .manual = state { true } else { false }
    }
}

public extension MockupPickerRowState {
    var actionTitle: String {
        switch self {
        case .supported(let action), .unsupported(let action), .manual(let action):
            action
        }
    }

    var isSupported: Bool {
        if case .supported = self { true } else { false }
    }
}

public struct MockupPickerSections: Equatable, Hashable, Sendable {
    public let supported: [MockupPickerRow]
    public let unsupported: [MockupPickerRow]
    public let manual: MockupPickerRow?

    public init(rows: [MockupPickerRow]) {
        supported = rows.filter { $0.isSupported }
        unsupported = rows.filter { $0.isUnsupported }
        manual = rows.first { $0.isManual }
    }
}

public enum DevicePickerCandidateSupport: Equatable, Hashable, Sendable {
    case supported(connectionRoute: String)
    case unsupported(disabledReason: String)
}

public extension DevicePickerCandidateSupport {
    init(_ dto: MobileDiscoveryCandidateDto) {
        if let connectionRoute = dto.connectionRoute {
            self = .supported(connectionRoute: connectionRoute)
        } else {
            self = .unsupported(disabledReason: dto.disabledReason ?? dto.detail)
        }
    }
}

public struct DevicePickerDiscoveryCandidate: Equatable, Hashable, Sendable {
    public let platformIdentifier: String
    public let displayName: String
    public let productCategory: String
    public let evidence: String
    public let detail: String
    public let support: DevicePickerCandidateSupport
    public let symbolName: String

    public init(
        platformIdentifier: String,
        displayName: String,
        productCategory: String,
        evidence: String,
        detail: String,
        support: DevicePickerCandidateSupport,
        symbolName: String
    ) {
        self.platformIdentifier = platformIdentifier
        self.displayName = displayName
        self.productCategory = productCategory
        self.evidence = evidence
        self.detail = detail
        self.support = support
        self.symbolName = symbolName
    }

    public init(advertisement: CoreBluetoothAdvertisement) {
        let candidate = mobileDiscoveryCandidateFromAdvertisement(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            localName: advertisement.localName,
            advertisedServiceUuids: advertisement.advertisedServiceUuids.compactMap(\.bluetooth16Value)
        )
        self.init(
            platformIdentifier: candidate.platformIdentifier,
            displayName: candidate.displayName,
            productCategory: candidate.productCategory,
            evidence: candidate.evidence,
            detail: candidate.detail,
            support: DevicePickerCandidateSupport(candidate),
            symbolName: candidate.support == .supported ? "circle.hexagongrid.circle" : "questionmark.circle"
        )
    }

    public var pickerRow: MockupPickerRow {
        MockupPickerRow(
            title: displayName,
            subtitle: "\(productCategory) - \(evidence)",
            detail: detail,
            state: support.pickerRowState,
            symbolName: symbolName
        )
    }
}

public extension DevicePickerCandidateSupport {
    var pickerRowState: MockupPickerRowState {
        switch self {
        case .supported:
            .supported(action: "Pair")
        case .unsupported(let disabledReason):
            .unsupported(action: disabledReason)
        }
    }
}

public enum DevicePickerScanStatus: Equatable, Hashable, Sendable {
    case scanning
    case idle
    case bluetoothUnavailable
    case permissionDenied
}

public struct DevicePickerScanState: Equatable, Hashable, Sendable {
    public let status: DevicePickerScanStatus
    public let rows: [MockupPickerRow]

    public init(status: DevicePickerScanStatus, rows: [MockupPickerRow]) {
        self.status = status
        self.rows = rows
    }

    public init(status: DevicePickerScanStatus, advertisements: [CoreBluetoothAdvertisement]) {
        self.init(
            status: status,
            rows: advertisements.map { DevicePickerDiscoveryCandidate(advertisement: $0).pickerRow }
        )
    }

    public var sections: MockupPickerSections {
        MockupPickerSections(rows: rows)
    }

    public var statusText: String {
        switch status {
        case .scanning:
            "Scanning Bluetooth"
        case .idle where rows.isEmpty:
            "No rideable devices found"
        case .idle:
            "Bluetooth scan complete"
        case .bluetoothUnavailable:
            "Bluetooth unavailable"
        case .permissionDenied:
            "Bluetooth permission denied"
        }
    }

    public static let bluetoothUnavailable = DevicePickerScanState(status: .bluetoothUnavailable, rows: [])
    public static let permissionDenied = DevicePickerScanState(status: .permissionDenied, rows: [])
}

public struct MockupScreen: Equatable, Hashable, Sendable, Identifiable {
    public let id: MockupScreenID
    public let title: String
    public let subtitle: String
    public let primaryValue: String
    public let secondaryValue: String
    public let warning: String?
    public let metrics: [MockupMetric]
    public let pickerRows: [MockupPickerRow]
    public let discoveryCandidates: [DevicePickerDiscoveryCandidate]
    public let safetyBars: [MockupSafetyBar]
    public let warningCard: MockupWarningCard?
    public let dashboardTiles: [MockupDashboardTile]
    public let tabs: [MockupScreenTab]
    public let isFixtureOnly: Bool

    public init(
        id: MockupScreenID,
        title: String,
        subtitle: String,
        primaryValue: String,
        secondaryValue: String,
        warning: String?,
        metrics: [MockupMetric],
        pickerRows: [MockupPickerRow] = [],
        discoveryCandidates: [DevicePickerDiscoveryCandidate] = [],
        safetyBars: [MockupSafetyBar] = [],
        warningCard: MockupWarningCard? = nil,
        dashboardTiles: [MockupDashboardTile] = [],
        tabs: [MockupScreenTab] = [],
        isFixtureOnly: Bool = true
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.primaryValue = primaryValue
        self.secondaryValue = secondaryValue
        self.warning = warning
        self.metrics = metrics
        self.pickerRows = pickerRows
        self.discoveryCandidates = discoveryCandidates
        self.safetyBars = safetyBars
        self.warningCard = warningCard
        self.dashboardTiles = dashboardTiles
        self.tabs = tabs
        self.isFixtureOnly = isFixtureOnly
    }
}

public struct MockupScreenCatalog: Equatable, Hashable, Sendable {
    public let screens: [MockupScreen]

    public init(screens: [MockupScreen]) {
        self.screens = screens
    }

    public func screen(id: MockupScreenID) -> MockupScreen? {
        screens.first { $0.id == id }
    }

    private static let devicePickerDiscoveryCandidates = [
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "fixture:aero",
            displayName: "Aero-126V",
            productCategory: "Electric unicycle",
            evidence: "telemetry profile found",
            detail: "126.0 V - strong signal",
            support: .supported(connectionRoute: "electric_unicycle"),
            symbolName: "circle.hexagongrid.circle"
        ),
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "fixture:little-focer",
            displayName: "Little FOCer BT",
            productCategory: "VESC Onewheel",
            evidence: "UART bridge detected",
            detail: "75.4 V - moderate signal",
            support: .supported(connectionRoute: "vesc_onewheel"),
            symbolName: "oval.portrait"
        ),
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "fixture:ninebot",
            displayName: "NINEBOT-7A31",
            productCategory: "Electric scooter",
            evidence: "known BLE advertisement",
            detail: "We can learn this later",
            support: .unsupported(disabledReason: "Not yet"),
            symbolName: "scooter"
        ),
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "fixture:hx-hoverboard",
            displayName: "HX Hoverboard",
            productCategory: "Hoverboard / self-balancing board",
            evidence: "candidate",
            detail: "Capture wizard later",
            support: .unsupported(disabledReason: "Not yet"),
            symbolName: "capsule"
        ),
    ]

    public static let v2 = MockupScreenCatalog(screens: [
        MockupScreen(
            id: .devicePicker,
            title: "Device picker",
            subtitle: "Scanning Bluetooth",
            primaryValue: "Aero-126V",
            secondaryValue: "Little FOCer BT",
            warning: "Unsupported rows remain disabled fixtures.",
            metrics: [
                MockupMetric(label: "Supported EUC", value: "Aero-126V"),
                MockupMetric(label: "Supported VESC OW", value: "Little FOCer BT"),
                MockupMetric(label: "Unsupported", value: "NINEBOT-7A31"),
                MockupMetric(label: "Unsupported", value: "HX Hoverboard"),
                MockupMetric(label: "Manual add", value: "disabled"),
            ],
            pickerRows: devicePickerDiscoveryCandidates.map(\.pickerRow) + [
                MockupPickerRow(
                    title: "Manual add / record unknown device",
                    subtitle: "",
                    detail: "",
                    state: .manual(action: "later"),
                    symbolName: "plus"
                ),
            ],
            discoveryCandidates: devicePickerDiscoveryCandidates
        ),
        MockupScreen(
            id: .eucRide,
            title: "Aero-126V",
            subtitle: "EUC - riding",
            primaryValue: "31 mph",
            secondaryValue: "PWM headroom 23%",
            warning: "Reduce acceleration - voltage sag under load: 9.4 V",
            metrics: [
                MockupMetric(label: "sag-adjusted energy", value: "62%"),
                MockupMetric(label: "pack", value: "115.8 V"),
                MockupMetric(label: "power", value: "4.2 kW"),
                MockupMetric(label: "thermal", value: "61 C"),
                MockupMetric(label: "limp-home", value: "14.2 mi"),
            ],
            safetyBars: [
                MockupSafetyBar(label: "PWM headroom", value: "23%", progress: 0.77, accent: .yellow),
                MockupSafetyBar(label: "sag-adjusted energy", value: "62%", progress: 0.62, accent: .cyan),
            ],
            warningCard: MockupWarningCard(
                title: "Reduce acceleration",
                detail: "Voltage sag under load: 9.4 V"
            ),
            dashboardTiles: [
                MockupDashboardTile(label: "pack", value: "115.8", unit: "V", detail: "-9.4 V sag", accent: .cyan),
                MockupDashboardTile(label: "power", value: "4.2", unit: "kW", detail: "regen -0.3 kW", accent: .yellow),
                MockupDashboardTile(label: "thermal", value: "61", unit: "°C", detail: "ESC 48 · motor 61", accent: .green),
                MockupDashboardTile(label: "limp-home", value: "14.2", unit: "mi", detail: "at this pace", accent: .cyan),
            ],
            tabs: [
                MockupScreenTab(title: "Ride", isSelected: true),
                MockupScreenTab(title: "Pack", isSelected: false),
                MockupScreenTab(title: "Map", isSelected: false),
                MockupScreenTab(title: "Tune", isSelected: false),
            ]
        ),
        MockupScreen(
            id: .eucGarage,
            title: "EUC health",
            subtitle: "Stationary diagnostics for wheel-specific data",
            primaryValue: "battery 85%",
            secondaryValue: "pack 115.8 V",
            warning: nil,
            metrics: [
                MockupMetric(label: "beep margin", value: "11.6 mph"),
                MockupMetric(label: "tiltback", value: "42 mph"),
                MockupMetric(label: "pedal mode", value: "72%"),
                MockupMetric(label: "cell delta", value: "0.018 V"),
                MockupMetric(label: "last fault", value: "none"),
            ]
        ),
        MockupScreen(
            id: .vescOnewheelRide,
            title: "Fungineers X7",
            subtitle: "VESC OW - armed",
            primaryValue: "19 mph",
            secondaryValue: "Duty headroom 18%",
            warning: "Pushback soon - duty and pack sag are both climbing.",
            metrics: [
                MockupMetric(label: "battery current", value: "38 A"),
                MockupMetric(label: "motor current", value: "71 A"),
                MockupMetric(label: "board angle", value: "-1.8 deg"),
                MockupMetric(label: "controller", value: "54 C"),
                MockupMetric(label: "motor", value: "49 C"),
            ]
        ),
        MockupScreen(
            id: .vescDebug,
            title: "VESC state",
            subtitle: "For tuning/debug. Not the riding screen",
            primaryValue: "duty cycle 82%",
            secondaryValue: "pack 75.4 V",
            warning: "Dangerous writes hidden until parked and confirmed.",
            metrics: [
                MockupMetric(label: "battery limit", value: "45 A"),
                MockupMetric(label: "motor limit", value: "90 A"),
                MockupMetric(label: "last fault", value: "FAULT_CODE_NONE"),
                MockupMetric(label: "input app", value: "ADC + balance"),
                MockupMetric(label: "logging", value: "local CSV armed"),
            ]
        ),
    ])
}
