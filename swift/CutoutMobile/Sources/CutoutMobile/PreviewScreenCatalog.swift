import Foundation

public enum PreviewScreenID: String, CaseIterable, Equatable, Hashable, Sendable {
    case devicePicker
    case eucRide
    case bmsOverview
    case bmsCellMap6S
    case bmsCellMap40S
    case bmsCellDetail
    case bmsUnknownTopology
    case bmsNoData
    case eucGarage
    case vescOnewheelRide
    case vescDebug
}

public enum PreviewConnectionRoute: String, Equatable, Hashable, Sendable {
    case electricUnicycle = "electric_unicycle"
    case vescOnewheel = "vesc_onewheel"

    public var destinationScreenID: PreviewScreenID {
        switch self {
        case .electricUnicycle:
            .eucRide
        case .vescOnewheel:
            .vescOnewheelRide
        }
    }
}

public struct PreviewMetric: Equatable, Hashable, Sendable {
    public let label: String
    public let value: String

    public init(label: String, value: String) {
        self.label = label
        self.value = value
    }
}

public enum PreviewAccent: String, Equatable, Hashable, Sendable {
    case cyan
    case green
    case orange
    case purple
    case yellow
}

public struct PreviewDeviceCard: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String
    public let status: String
    public let accent: PreviewAccent

    public init(title: String, detail: String, status: String, accent: PreviewAccent) {
        self.title = title
        self.detail = detail
        self.status = status
        self.accent = accent
    }
}

public struct PreviewSafetyBar: Equatable, Hashable, Sendable {
    public let label: String
    public let value: String
    public let progress: Double
    public let accent: PreviewAccent

    public init(label: String, value: String, progress: Double, accent: PreviewAccent) {
        self.label = label
        self.value = value
        self.progress = progress
        self.accent = accent
    }
}

public struct PreviewWarningCard: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String

    public init(title: String, detail: String) {
        self.title = title
        self.detail = detail
    }
}

public struct PreviewDashboardTile: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { label }

    public let label: String
    public let value: String
    public let unit: String
    public let detail: String
    public let accent: PreviewAccent

    public init(label: String, value: String, unit: String, detail: String, accent: PreviewAccent) {
        self.label = label
        self.value = value
        self.unit = unit
        self.detail = detail
        self.accent = accent
    }
}

public struct PreviewScreenTab: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { title }

    public let title: String
    public let isSelected: Bool

    public init(title: String, isSelected: Bool) {
        self.title = title
        self.isSelected = isSelected
    }
}

public struct PreviewSummaryRow: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { label }

    public let label: String
    public let value: String
    public let accent: PreviewAccent?

    public init(label: String, value: String, accent: PreviewAccent?) {
        self.label = label
        self.value = value
        self.accent = accent
    }
}

public struct PreviewFaultCard: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String
    public let accent: PreviewAccent

    public init(title: String, detail: String, accent: PreviewAccent) {
        self.title = title
        self.detail = detail
        self.accent = accent
    }
}
public enum PreviewBmsScreenKind: Equatable, Hashable, Sendable {
    case overview
    case cellMapInline
    case cellMapScrollable
    case cellDetail
    case unknownTopology
    case noData
}

public struct PreviewBmsChip: Equatable, Hashable, Sendable, Identifiable {
    public let id: UUID

    public let title: String
    public let accent: PreviewAccent

    public init(id: UUID = UUID(), title: String, accent: PreviewAccent) {
        self.id = id
        self.title = title
        self.accent = accent
    }
}

public struct PreviewBmsContent: Equatable, Hashable, Sendable {
    public let kind: PreviewBmsScreenKind
    public let snapshot: BmsSnapshot
    public let chips: [PreviewBmsChip]
    public let highlightedGroupIndices: [Int]
    public let selectedGroupIndex: Int?
    public let modeTitles: [String]

    public init(
        kind: PreviewBmsScreenKind,
        snapshot: BmsSnapshot,
        chips: [PreviewBmsChip] = [],
        highlightedGroupIndices: [Int] = [],
        selectedGroupIndex: Int? = nil,
        modeTitles: [String] = []
    ) {
        self.kind = kind
        self.snapshot = snapshot
        self.chips = chips
        self.highlightedGroupIndices = highlightedGroupIndices
        self.selectedGroupIndex = selectedGroupIndex
        self.modeTitles = modeTitles
    }
}
public enum PreviewPickerRowState: Equatable, Hashable, Sendable {
    case supported(action: String)
    case unsupported(action: String)
    case manual(action: String)
}

public struct PreviewPickerRow: Equatable, Hashable, Sendable, Identifiable {
    public let id: String

    public let title: String
    public let subtitle: String
    public let detail: String
    public let state: PreviewPickerRowState
    public let symbolName: String
    public let connectionRoute: PreviewConnectionRoute?

    public init(
        id: String? = nil,
        title: String,
        subtitle: String,
        detail: String,
        state: PreviewPickerRowState,
        symbolName: String,
        connectionRoute: PreviewConnectionRoute? = nil
    ) {
        self.id = id ?? title
        self.title = title
        self.subtitle = subtitle
        self.detail = detail
        self.state = state
        self.symbolName = symbolName
        self.connectionRoute = connectionRoute
    }
}

public extension PreviewPickerRow {
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

public extension PreviewPickerRowState {
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

public struct PreviewPickerSections: Equatable, Hashable, Sendable {
    public let supported: [PreviewPickerRow]
    public let unsupported: [PreviewPickerRow]
    public let manual: PreviewPickerRow?

    public init(rows: [PreviewPickerRow]) {
        supported = rows.filter { $0.isSupported }
        unsupported = rows.filter { $0.isUnsupported }
        manual = rows.first { $0.isManual }
    }
}

public enum DevicePickerCandidateSupport: Equatable, Hashable, Sendable {
    case supported(connectionRoute: PreviewConnectionRoute?, electricUnicycleModel: ElectricUnicycleModel?)
    case unsupported(disabledReason: String)
}

public extension DevicePickerCandidateSupport {
    init(_ dto: MobileDiscoveryCandidateDto) {
        switch dto.support {
        case .supported:
            self = .supported(
                connectionRoute: dto.connectionRoute.flatMap(PreviewConnectionRoute.init(rawValue:)),
                electricUnicycleModel: dto.electricUnicycleModel.map(ElectricUnicycleModel.init)
            )
        case .unsupported:
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
        self.init(candidate: mobileDiscoveryCandidateFromAdvertisement(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            localName: advertisement.localName,
            advertisedServiceUuids: advertisement.advertisedServiceUuids.compactMap(\.bluetooth16Value)
        ))
    }

    public init(candidate: MobileDiscoveryCandidateDto) {
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

    public static func pickerRow(advertisement: CoreBluetoothAdvertisement) -> PreviewPickerRow? {
        let candidate = mobileDiscoveryCandidateFromAdvertisement(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            localName: advertisement.localName,
            advertisedServiceUuids: advertisement.advertisedServiceUuids.compactMap(\.bluetooth16Value)
        )
        guard candidate.isPickerCandidate else { return nil }
        return DevicePickerDiscoveryCandidate(candidate: candidate).pickerRow
    }

    public var pickerRow: PreviewPickerRow {
        PreviewPickerRow(
            id: platformIdentifier,
            title: displayName,
            subtitle: "\(productCategory) - \(evidence)",
            detail: detail,
            state: support.pickerRowState,
            symbolName: symbolName,
            connectionRoute: support.connectionRoute
        )
    }
}

public extension DevicePickerCandidateSupport {
    var isSupported: Bool {
        if case .supported = self { true } else { false }
    }

    var connectionRoute: PreviewConnectionRoute? {
        if case .supported(let connectionRoute, _) = self { connectionRoute } else { nil }
    }

    var electricUnicycleModel: ElectricUnicycleModel? {
        if case .supported(_, let electricUnicycleModel) = self { electricUnicycleModel } else { nil }
    }

    var pickerRowState: PreviewPickerRowState {
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
    public let rows: [PreviewPickerRow]

    public init(status: DevicePickerScanStatus, rows: [PreviewPickerRow]) {
        self.status = status
        self.rows = rows
    }

    public init(status: DevicePickerScanStatus, advertisements: [CoreBluetoothAdvertisement]) {
        self.init(
            status: status,
            rows: advertisements.compactMap(DevicePickerDiscoveryCandidate.pickerRow(advertisement:))
        )
    }

    public var sections: PreviewPickerSections {
        PreviewPickerSections(rows: rows)
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

public struct PreviewScreen: Equatable, Hashable, Sendable, Identifiable {
    public let id: PreviewScreenID
    public let title: String
    public let subtitle: String
    public let primaryValue: String
    public let secondaryValue: String
    public let warning: String?
    public let metrics: [PreviewMetric]
    public let pickerRows: [PreviewPickerRow]
    public let discoveryCandidates: [DevicePickerDiscoveryCandidate]
    public let deviceCard: PreviewDeviceCard?
    public let safetyBars: [PreviewSafetyBar]
    public let warningCard: PreviewWarningCard?
    public let dashboardTiles: [PreviewDashboardTile]
    public let summaryTitle: String?
    public let summaryRows: [PreviewSummaryRow]
    public let faultCard: PreviewFaultCard?
    public let tabs: [PreviewScreenTab]
    public let bmsContent: PreviewBmsContent?
    public let isFixtureOnly: Bool

    public init(
        id: PreviewScreenID,
        title: String,
        subtitle: String,
        primaryValue: String,
        secondaryValue: String,
        warning: String?,
        metrics: [PreviewMetric],
        pickerRows: [PreviewPickerRow] = [],
        discoveryCandidates: [DevicePickerDiscoveryCandidate] = [],
        deviceCard: PreviewDeviceCard? = nil,
        safetyBars: [PreviewSafetyBar] = [],
        warningCard: PreviewWarningCard? = nil,
        dashboardTiles: [PreviewDashboardTile] = [],
        summaryTitle: String? = nil,
        summaryRows: [PreviewSummaryRow] = [],
        faultCard: PreviewFaultCard? = nil,
        tabs: [PreviewScreenTab] = [],
        bmsContent: PreviewBmsContent? = nil,
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
        self.deviceCard = deviceCard
        self.safetyBars = safetyBars
        self.warningCard = warningCard
        self.dashboardTiles = dashboardTiles
        self.summaryTitle = summaryTitle
        self.summaryRows = summaryRows
        self.faultCard = faultCard
        self.tabs = tabs
        self.bmsContent = bmsContent
        self.isFixtureOnly = isFixtureOnly
    }
}

public struct PreviewScreenCatalog: Equatable, Hashable, Sendable {
    public let screens: [PreviewScreen]

    public init(screens: [PreviewScreen]) {
        self.screens = screens
    }

    public func screen(id: PreviewScreenID) -> PreviewScreen? {
        screens.first { $0.id == id }
    }

    private static let devicePickerDiscoveryCandidates = [
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "fixture:aero",
            displayName: "Aero-126V",
            productCategory: "Electric unicycle",
            evidence: "telemetry profile found",
            detail: "126.0 V - strong signal",
            support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
            symbolName: "circle.hexagongrid.circle"
        ),
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "fixture:little-focer",
            displayName: "Little FOCer BT",
            productCategory: "VESC Onewheel",
            evidence: "UART bridge detected",
            detail: "75.4 V - moderate signal",
            support: .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil),
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

    private static let bmsOverviewSnapshot = BmsSnapshot(
        topology: BmsTopology(
            layoutLabel: "20S4P split pack",
            seriesGroupCount: 20,
            parallelCount: 4,
            packCount: 2,
            bmsCount: 2,
            confidence: .verified
        ),
        energyPercent: .fixture(value: 72),
        voltage: .fixture(value: 81_600),
        cellDelta: .fixture(value: 18),
        lowestGroupIndex: 17,
        highestTemperature: .fixture(value: 37_800),
        highestTemperatureLabel: "right pack",
        balancingSummary: "idle • top groups only",
        balancingDetail: "3 groups bleeding: 03, 11, 19",
        faultSummary: "no active faults",
        faultDetail: "last: under-voltage warning · 3 days ago",
        groups: (1...20).map { index in
            BmsGroupSnapshot(
                index: index,
                voltage: .fixture(value: 4_089 - Int32(index % 5) * 4),
                alertLevel: index == 17 ? .warning : .nominal
            )
        }
    )

    private static let bmsInlineSnapshot = BmsSnapshot(
        topology: BmsTopology(
            layoutLabel: "skateboard pack",
            seriesGroupCount: 6,
            parallelCount: 2,
            packCount: 1,
            bmsCount: 1,
            confidence: .verified
        ),
        cellDelta: .fixture(value: 12),
        groups: [
            BmsGroupSnapshot(index: 1, voltage: .fixture(value: 4_104), alertLevel: .nominal),
            BmsGroupSnapshot(index: 2, voltage: .fixture(value: 4_101), alertLevel: .nominal),
            BmsGroupSnapshot(index: 3, voltage: .fixture(value: 4_096), alertLevel: .warning),
            BmsGroupSnapshot(index: 4, voltage: .fixture(value: 4_099), alertLevel: .nominal),
            BmsGroupSnapshot(index: 5, voltage: .fixture(value: 4_103), alertLevel: .nominal),
            BmsGroupSnapshot(index: 6, voltage: .fixture(value: 4_092), alertLevel: .warning),
        ]
    )

    private static let bmsScrollableSnapshot = BmsSnapshot(
        topology: BmsTopology(
            layoutLabel: "large EUC pack",
            seriesGroupCount: 40,
            parallelCount: 4,
            packCount: 1,
            bmsCount: 1,
            confidence: .verified
        ),
        cellDelta: .fixture(value: 18),
        highestTemperature: .fixture(value: 31_000),
        highestTemperatureLabel: "group 31",
        groups: (1...40).map { index in
            let alertLevel: BmsAlertLevel = [17, 18, 19].contains(index) ? .warning : index == 31 ? .critical : .nominal
            let base = 4_080 + Int32(index % 3) * 6
            let value = [17, 18, 19].contains(index) ? 4_080 : (index == 31 ? 4_072 : base)
            return BmsGroupSnapshot(index: index, voltage: .fixture(value: value), alertLevel: alertLevel)
        },
        faults: [
            BmsFault(code: "temp-sensor", label: "31 has temp sensor mismatch", level: .warning)
        ]
    )

    private static let bmsDetailSnapshot = BmsSnapshot(
        topology: BmsTopology(
            layoutLabel: "20S4P split pack",
            seriesGroupCount: 20,
            parallelCount: 4,
            packCount: 2,
            bmsCount: 2,
            confidence: .verified
        ),
        cellDelta: .fixture(value: 18),
        groups: (1...20).map { index in
            BmsGroupSnapshot(
                index: index,
                voltage: .fixture(value: index == 17 ? 4_071 : 4_086),
                temperature: .fixture(value: index == 17 ? 34_900 : 33_000),
                resistanceMilliohms: index == 17 ? 21 : 18,
                alertLevel: index == 17 ? .warning : .nominal,
                detail: index == 17 ? "drops first during acceleration" : nil
            )
        }
    )

    private static let bmsUnknownSnapshot = BmsSnapshot(
        topology: BmsTopology(
            layoutLabel: "topology unverified",
            seriesGroupCount: nil,
            parallelCount: nil,
            packCount: 1,
            bmsCount: 1,
            confidence: .unverified
        ),
        voltage: .fixture(value: 75_900),
        faultSummary: "BMS found, map unknown",
        faultDetail: "show raw-safe info until topology is confirmed",
        faults: [
            BmsFault(code: "0x0040", label: "needs decoder", level: .critical)
        ],
        captureActionTitle: "record unsupported pack",
        captureActionState: "disabled for launch"
    )

    private static let bmsNoDataSnapshot = BmsSnapshot(
        topology: BmsTopology(
            layoutLabel: "non-smart BMS",
            seriesGroupCount: nil,
            parallelCount: nil,
            packCount: 1,
            bmsCount: 0,
            confidence: .inferred
        ),
        energyPercent: .estimatedFixture(value: 71),
        voltage: .fixture(value: 117_600),
        current: .fixture(value: 38_000),
        captureActionTitle: "Trust sag, alarms, and headroom more than percent.",
        captureActionState: "limited data"
    )

    public static let v2 = PreviewScreenCatalog(screens: [
        PreviewScreen(
            id: .devicePicker,
            title: "Device picker",
            subtitle: "Scanning Bluetooth",
            primaryValue: "Aero-126V",
            secondaryValue: "Little FOCer BT",
            warning: "Unsupported rows remain disabled fixtures.",
            metrics: [
                PreviewMetric(label: "Supported EUC", value: "Aero-126V"),
                PreviewMetric(label: "Supported VESC OW", value: "Little FOCer BT"),
                PreviewMetric(label: "Unsupported", value: "NINEBOT-7A31"),
                PreviewMetric(label: "Unsupported", value: "HX Hoverboard"),
                PreviewMetric(label: "Manual add", value: "disabled"),
            ],
            pickerRows: devicePickerDiscoveryCandidates.map(\.pickerRow) + [
                PreviewPickerRow(
                    title: "Manual add / record unknown device",
                    subtitle: "",
                    detail: "",
                    state: .manual(action: "later"),
                    symbolName: "plus"
                ),
            ],
            discoveryCandidates: devicePickerDiscoveryCandidates
        ),
        PreviewScreen(
            id: .eucRide,
            title: "Aero-126V",
            subtitle: "EUC - riding",
            primaryValue: "31 mph",
            secondaryValue: "PWM headroom 23%",
            warning: "Reduce acceleration - voltage sag under load: 9.4 V",
            metrics: [
                PreviewMetric(label: "sag-adjusted energy", value: "62%"),
                PreviewMetric(label: "pack", value: "115.8 V"),
                PreviewMetric(label: "power", value: "4.2 kW"),
                PreviewMetric(label: "thermal", value: "61 C"),
                PreviewMetric(label: "limp-home", value: "14.2 mi"),
            ],
            safetyBars: [
                PreviewSafetyBar(label: "PWM headroom", value: "23%", progress: 0.77, accent: .yellow),
                PreviewSafetyBar(label: "sag-adjusted energy", value: "62%", progress: 0.62, accent: .cyan),
            ],
            warningCard: PreviewWarningCard(
                title: "Reduce acceleration",
                detail: "Voltage sag under load: 9.4 V"
            ),
            dashboardTiles: [
                PreviewDashboardTile(label: "pack", value: "115.8", unit: "V", detail: "-9.4 V sag", accent: .cyan),
                PreviewDashboardTile(label: "power", value: "4.2", unit: "kW", detail: "regen -0.3 kW", accent: .yellow),
                PreviewDashboardTile(label: "thermal", value: "61", unit: "°C", detail: "ESC 48 · motor 61", accent: .green),
                PreviewDashboardTile(label: "limp-home", value: "14.2", unit: "mi", detail: "at this pace", accent: .cyan),
            ],
            tabs: [
                PreviewScreenTab(title: "Ride", isSelected: true),
                PreviewScreenTab(title: "Pack", isSelected: false),
                PreviewScreenTab(title: "Map", isSelected: false),
                PreviewScreenTab(title: "Tune", isSelected: false),
            ]
        ),
        PreviewScreen(
            id: .bmsOverview,
            title: "Pack overview",
            subtitle: "CutOut · BMS",
            primaryValue: "72%",
            secondaryValue: "sag adjusted",
            warning: nil,
            metrics: [
                PreviewMetric(label: "topology", value: "20S4P split pack"),
                PreviewMetric(label: "BMS online", value: "2"),
                PreviewMetric(label: "lowest group", value: "group 17"),
                PreviewMetric(label: "highest temp", value: "37.8 °C"),
            ],
            bmsContent: PreviewBmsContent(
                kind: .overview,
                snapshot: bmsOverviewSnapshot,
                chips: [
                    PreviewBmsChip(title: "20S4P split pack", accent: .yellow),
                    PreviewBmsChip(title: "2 BMS online", accent: .green),
                ]
            )
        ),
        PreviewScreen(
            id: .bmsCellMap6S,
            title: "6S cell map",
            subtitle: "skateboard pack",
            primaryValue: "12 mV spread",
            secondaryValue: "no scrolling needed",
            warning: nil,
            metrics: [
                PreviewMetric(label: "topology", value: "6S2P"),
                PreviewMetric(label: "display", value: "all groups inline"),
            ],
            bmsContent: PreviewBmsContent(
                kind: .cellMapInline,
                snapshot: bmsInlineSnapshot,
                chips: [
                    PreviewBmsChip(title: "skateboard pack", accent: .cyan),
                    PreviewBmsChip(title: "6S2P", accent: .yellow),
                ],
                highlightedGroupIndices: [3, 6],
                modeTitles: ["balance view", "temps", "faults"]
            )
        ),
        PreviewScreen(
            id: .bmsCellMap40S,
            title: "40S cell map",
            subtitle: "large EUC pack",
            primaryValue: "17–19 sagging under load",
            secondaryValue: "scroll cells horizontally",
            warning: nil,
            metrics: [
                PreviewMetric(label: "topology", value: "40S4P"),
                PreviewMetric(label: "display", value: "overview first"),
            ],
            bmsContent: PreviewBmsContent(
                kind: .cellMapScrollable,
                snapshot: bmsScrollableSnapshot,
                chips: [
                    PreviewBmsChip(title: "large EUC pack", accent: .cyan),
                    PreviewBmsChip(title: "40S4P", accent: .yellow),
                    PreviewBmsChip(title: "scroll cells horizontally", accent: .orange),
                ],
                highlightedGroupIndices: [17, 18, 19, 31],
                modeTitles: ["overview", "strip", "full cell table", "popover"]
            )
        ),
        PreviewScreen(
            id: .bmsCellDetail,
            title: "Cell detail",
            subtitle: "from any map",
            primaryValue: "4.071 V",
            secondaryValue: "group 17",
            warning: nil,
            metrics: [
                PreviewMetric(label: "temp", value: "34.9 °C"),
                PreviewMetric(label: "IR est.", value: "21 mΩ"),
            ],
            bmsContent: PreviewBmsContent(
                kind: .cellDetail,
                snapshot: bmsDetailSnapshot,
                chips: [
                    PreviewBmsChip(title: "from any map", accent: .cyan),
                    PreviewBmsChip(title: "group 17", accent: .orange),
                ],
                highlightedGroupIndices: [17],
                selectedGroupIndex: 17
            )
        ),
        PreviewScreen(
            id: .bmsUnknownTopology,
            title: "Unknown BMS",
            subtitle: "partial data",
            primaryValue: "BMS found, map unknown",
            secondaryValue: "topology unverified",
            warning: nil,
            metrics: [
                PreviewMetric(label: "reported voltage", value: "75.9 V"),
                PreviewMetric(label: "fault bits", value: "0x0040"),
                PreviewMetric(label: "capture flow", value: "disabled for launch"),
            ],
            bmsContent: PreviewBmsContent(
                kind: .unknownTopology,
                snapshot: bmsUnknownSnapshot,
                chips: [
                    PreviewBmsChip(title: "partial data", accent: .orange),
                    PreviewBmsChip(title: "topology unverified", accent: .green),
                ]
            )
        ),
        PreviewScreen(
            id: .bmsNoData,
            title: "Battery",
            subtitle: "EX30 · non-smart BMS · controller-only estimate",
            primaryValue: "71%",
            secondaryValue: "limited data",
            warning: nil,
            metrics: [
                PreviewMetric(label: "pack voltage", value: "117.6 V"),
                PreviewMetric(label: "ride sag", value: "4.8 V"),
                PreviewMetric(label: "load now", value: "38 A"),
            ],
            bmsContent: PreviewBmsContent(
                kind: .noData,
                snapshot: bmsNoDataSnapshot,
                chips: [
                    PreviewBmsChip(title: "limited data", accent: .yellow),
                ]
            )
        ),
        PreviewScreen(
            id: .eucGarage,
            title: "EUC health",
            subtitle: "Stationary diagnostics for wheel-specific data",
            primaryValue: "battery 85%",
            secondaryValue: "pack 115.8 V",
            warning: nil,
            metrics: [
                PreviewMetric(label: "beep margin", value: "11.6 mph"),
                PreviewMetric(label: "tiltback", value: "42 mph"),
                PreviewMetric(label: "pedal mode", value: "72%"),
                PreviewMetric(label: "cell delta", value: "0.018 V"),
                PreviewMetric(label: "last fault", value: "none"),
            ],
            deviceCard: PreviewDeviceCard(
                title: "Aero-126V",
                detail: "126 V nominal · 20s? mapped profile · BLE",
                status: "Safe",
                accent: .green
            ),
            dashboardTiles: [
                PreviewDashboardTile(label: "battery", value: "85", unit: "%", detail: "115.8 V", accent: .cyan),
                PreviewDashboardTile(label: "beep margin", value: "11.6", unit: "mph", detail: "to configured alarm", accent: .yellow),
                PreviewDashboardTile(label: "tiltback", value: "42", unit: "mph", detail: "wheel setting", accent: .orange),
                PreviewDashboardTile(label: "pedal mode", value: "72", unit: "%", detail: "hardness normalized", accent: .purple),
            ],
            summaryTitle: "Cell / BMS summary",
            summaryRows: [
                PreviewSummaryRow(label: "high group", value: "4.18 V", accent: nil),
                PreviewSummaryRow(label: "low group", value: "4.13 V", accent: nil),
                PreviewSummaryRow(label: "delta", value: "0.05 V", accent: .green),
            ],
            faultCard: PreviewFaultCard(title: "Last fault", detail: "none since 38.2 mi ago", accent: .green)
        ),
        PreviewScreen(
            id: .vescOnewheelRide,
            title: "Fungineers X7",
            subtitle: "VESC OW · armed",
            primaryValue: "19",
            secondaryValue: "board speed",
            warning: "Pushback soon - duty and pack sag are both climbing.",
            metrics: [
                PreviewMetric(label: "battery current", value: "38 A"),
                PreviewMetric(label: "motor current", value: "71 A"),
                PreviewMetric(label: "board angle", value: "-1.8 deg"),
                PreviewMetric(label: "controller", value: "54 C"),
                PreviewMetric(label: "motor", value: "49 C"),
            ],
            safetyBars: [
                PreviewSafetyBar(label: "Duty headroom", value: "18%", progress: 0.82, accent: .orange),
            ],
            warningCard: PreviewWarningCard(
                title: "Pushback soon",
                detail: "Duty and pack sag are both climbing."
            ),
            dashboardTiles: [
                PreviewDashboardTile(label: "battery current", value: "38", unit: "A", detail: "limit 45 A", accent: .yellow),
                PreviewDashboardTile(label: "motor current", value: "71", unit: "A", detail: "phase estimate", accent: .orange),
                PreviewDashboardTile(label: "board angle", value: "-1.8", unit: "°", detail: "nose down", accent: .cyan),
                PreviewDashboardTile(label: "controller", value: "54", unit: "°C", detail: "motor 49 °C", accent: .green),
            ],
            tabs: [
                PreviewScreenTab(title: "Ride", isSelected: true),
                PreviewScreenTab(title: "VESC", isSelected: false),
                PreviewScreenTab(title: "Map", isSelected: false),
                PreviewScreenTab(title: "Logs", isSelected: false),
            ]
        ),
        PreviewScreen(
            id: .vescDebug,
            title: "VESC state",
            subtitle: "For tuning/debug. Not the riding screen",
            primaryValue: "duty cycle 82%",
            secondaryValue: "pack 75.4 V",
            warning: "Dangerous writes hidden until parked and confirmed.",
            metrics: [
                PreviewMetric(label: "battery limit", value: "45 A"),
                PreviewMetric(label: "motor limit", value: "90 A"),
                PreviewMetric(label: "last fault", value: "FAULT_CODE_NONE"),
                PreviewMetric(label: "input app", value: "ADC + balance"),
                PreviewMetric(label: "logging", value: "local CSV armed"),
            ],
            deviceCard: PreviewDeviceCard(
                title: "Profile: Street stable",
                detail: "VESC Express · FW 6.x · UART bridge",
                status: "",
                accent: .cyan
            ),
            dashboardTiles: [
                PreviewDashboardTile(label: "duty cycle", value: "82", unit: "%", detail: "max seen 87%", accent: .orange),
                PreviewDashboardTile(label: "pack", value: "75.4", unit: "V", detail: "20s lithium", accent: .cyan),
                PreviewDashboardTile(label: "battery limit", value: "45", unit: "A", detail: "current max", accent: .yellow),
                PreviewDashboardTile(label: "motor limit", value: "90", unit: "A", detail: "phase current", accent: .orange),
            ],
            summaryTitle: "Fault / app channels",
            summaryRows: [
                PreviewSummaryRow(label: "last fault", value: "FAULT_CODE_NONE", accent: .green),
                PreviewSummaryRow(label: "input app", value: "ADC + balance", accent: nil),
                PreviewSummaryRow(label: "CAN status", value: "single controller", accent: nil),
                PreviewSummaryRow(label: "logging", value: "local CSV armed", accent: .yellow),
            ],
            faultCard: PreviewFaultCard(
                title: "Guardrails",
                detail: "Hide dangerous writes until parked + confirmed.",
                accent: .orange
            )
        ),
    ])
}

extension TelemetryReading {
    static func fixture(value: Value) -> Self {
        Self(
            value: value,
            source: .reported,
            quality: .known,
            verification: .hardwareVerified
        )
    }

    static func estimatedFixture(value: Value) -> Self {
        Self(
            value: value,
            source: .estimated,
            quality: .inferred,
            verification: .inferred
        )
    }
}
