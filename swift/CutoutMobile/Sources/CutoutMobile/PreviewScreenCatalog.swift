import Foundation

public enum MockupScreenID: String, CaseIterable, Equatable, Hashable, Sendable {
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

public enum MockupConnectionRoute: String, Equatable, Hashable, Sendable {
    case electricUnicycle = "electric_unicycle"
    case vescOnewheel = "vesc_onewheel"

    public var destinationScreenID: MockupScreenID {
        switch self {
        case .electricUnicycle:
            .eucRide
        case .vescOnewheel:
            .vescOnewheelRide
        }
    }
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
    case purple
    case yellow
}

public struct MockupDeviceCard: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String
    public let status: String
    public let accent: MockupAccent

    public init(title: String, detail: String, status: String, accent: MockupAccent) {
        self.title = title
        self.detail = detail
        self.status = status
        self.accent = accent
    }
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

public enum MockupDashboardTileKind: Equatable, Hashable, Sendable {
    case metric
    case beepMargin
    case tiltback
    case pedalMode
}

public struct MockupDashboardTile: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { label }

    public let kind: MockupDashboardTileKind
    public let label: String
    public let value: String
    public let unit: String
    public let detail: String
    public let accent: MockupAccent

    public init(
        kind: MockupDashboardTileKind = .metric,
        label: String,
        value: String,
        unit: String,
        detail: String,
        accent: MockupAccent
    ) {
        self.kind = kind
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

public struct MockupSummaryRow: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { label }

    public let label: String
    public let value: String
    public let accent: MockupAccent?

    public init(label: String, value: String, accent: MockupAccent?) {
        self.label = label
        self.value = value
        self.accent = accent
    }
}

public struct MockupFaultCard: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String
    public let accent: MockupAccent

    public init(title: String, detail: String, accent: MockupAccent) {
        self.title = title
        self.detail = detail
        self.accent = accent
    }
}
public enum MockupBmsScreenKind: Equatable, Hashable, Sendable {
    case overview
    case cellMapInline
    case cellMapScrollable
    case cellDetail
    case unknownTopology
    case noData
}

public struct MockupBmsChip: Equatable, Hashable, Sendable {
    public let title: String
    public let accent: MockupAccent

    public init(title: String, accent: MockupAccent) {
        self.title = title
        self.accent = accent
    }
}

public struct MockupBmsContent: Equatable, Hashable, Sendable {
    public let kind: MockupBmsScreenKind
    public let snapshot: BmsSnapshot
    public let chips: [MockupBmsChip]
    public let highlightedGroupIndices: [Int]
    public let selectedGroupIndex: Int?
    public let modeTitles: [String]

    public init(
        kind: MockupBmsScreenKind,
        snapshot: BmsSnapshot,
        chips: [MockupBmsChip] = [],
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

    public func resolved(with liveSnapshot: BmsSnapshot, preferredScreenID: MockupScreenID) -> Self {
        let resolvedKind = MockupBmsScreenKind(liveSnapshot: liveSnapshot, preferredScreenID: preferredScreenID)
        let selectedGroupIndex = resolvedKind == .cellDetail
            ? liveSnapshot.lowestGroupIndex ?? liveSnapshot.groups.first?.index
            : nil
        let highlightedGroupIndices = selectedGroupIndex.map { [$0] } ?? liveSnapshot.lowestGroupIndex.map { [$0] } ?? []
        let chips = resolvedKind.liveChips(snapshot: liveSnapshot, selectedGroupIndex: selectedGroupIndex)

        return MockupBmsContent(
            kind: resolvedKind,
            snapshot: liveSnapshot,
            chips: chips,
            highlightedGroupIndices: highlightedGroupIndices,
            selectedGroupIndex: selectedGroupIndex
        )
    }
}

private extension MockupBmsScreenKind {
    init(liveSnapshot: BmsSnapshot, preferredScreenID: MockupScreenID) {
        if liveSnapshot.availability == .unsupported {
            self = .noData
            return
        }

        if !liveSnapshot.groups.isEmpty {
            if preferredScreenID == .bmsCellDetail {
                self = .cellDetail
            } else if liveSnapshot.groups.count <= 14 {
                self = .cellMapInline
            } else {
                self = .cellMapScrollable
            }
            return
        }

        if liveSnapshot.topology.confidence == .unverified, liveSnapshot.topology.seriesGroupCount == nil {
            self = .unknownTopology
            return
        }

        self = .overview
    }

    func liveTitle(snapshot: BmsSnapshot) -> String {
        switch self {
        case .overview:
            "Pack overview"
        case .cellMapInline, .cellMapScrollable:
            snapshot.topology.seriesGroupCount.map { "\($0)S cell map" } ?? "Cell map"
        case .cellDetail:
            "Cell detail"
        case .unknownTopology:
            "Unknown BMS"
        case .noData:
            "Battery"
        }
    }

    func liveSubtitle(snapshot: BmsSnapshot, fallback: String) -> String {
        switch self {
        case .noData:
            "\(snapshot.topology.layoutLabel) · controller-only estimate"
        default:
            fallback
        }
    }

    func liveSecondaryValue(snapshot: BmsSnapshot, fallback: String) -> String {
        switch self {
        case .noData:
            snapshot.captureActionState ?? fallback
        default:
            fallback
        }
    }

    func liveChips(snapshot: BmsSnapshot, selectedGroupIndex: Int?) -> [MockupBmsChip] {
        switch self {
        case .overview:
            var chips = [MockupBmsChip(title: snapshot.topology.layoutLabel, accent: .yellow)]
            if snapshot.topology.bmsCount > 0 {
                chips.append(MockupBmsChip(title: "\(snapshot.topology.bmsCount) BMS online", accent: .green))
            }
            return chips
        case .cellMapInline, .cellMapScrollable:
            return [
                MockupBmsChip(title: "live readback", accent: .cyan),
                MockupBmsChip(title: snapshot.topology.layoutLabel, accent: .yellow),
            ]
        case .cellDetail:
            return [
                MockupBmsChip(title: "live readback", accent: .cyan),
                MockupBmsChip(title: selectedGroupIndex.map { "group \($0)" } ?? "selected group", accent: .orange),
            ]
        case .unknownTopology:
            return [
                MockupBmsChip(title: "partial data", accent: .orange),
                MockupBmsChip(title: "topology unverified", accent: .green),
            ]
        case .noData:
            return [
                MockupBmsChip(title: snapshot.captureActionState ?? "limited data", accent: .yellow),
            ]
        }
    }
}
public enum MockupPickerRowState: Equatable, Hashable, Sendable {
    case supported(action: String)
    case unsupported(action: String)
    case manual(action: String)
}

public struct MockupPickerRow: Equatable, Hashable, Sendable, Identifiable {
    public let id: String

    public let title: String
    public let subtitle: String
    public let detail: String
    public let state: MockupPickerRowState
    public let symbolName: String
    public let connectionRoute: MockupConnectionRoute?

    public init(
        id: String? = nil,
        title: String,
        subtitle: String,
        detail: String,
        state: MockupPickerRowState,
        symbolName: String,
        connectionRoute: MockupConnectionRoute? = nil
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
    case supported(connectionRoute: MockupConnectionRoute?, electricUnicycleModel: ElectricUnicycleModel?)
    case unsupported(disabledReason: String)
}

public extension DevicePickerCandidateSupport {
    init(_ dto: MobileDiscoveryCandidateDto) {
        switch dto.support {
        case .supported:
            self = .supported(
                connectionRoute: dto.connectionRoute.flatMap(MockupConnectionRoute.init(rawValue:)),
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

    public static func pickerRow(advertisement: CoreBluetoothAdvertisement) -> MockupPickerRow? {
        let candidate = mobileDiscoveryCandidateFromAdvertisement(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            localName: advertisement.localName,
            advertisedServiceUuids: advertisement.advertisedServiceUuids.compactMap(\.bluetooth16Value)
        )
        guard candidate.isPickerCandidate else { return nil }
        return DevicePickerDiscoveryCandidate(candidate: candidate).pickerRow
    }

    public var pickerRow: MockupPickerRow {
        MockupPickerRow(
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

    var connectionRoute: MockupConnectionRoute? {
        if case .supported(let connectionRoute, _) = self { connectionRoute } else { nil }
    }

    var electricUnicycleModel: ElectricUnicycleModel? {
        if case .supported(_, let electricUnicycleModel) = self { electricUnicycleModel } else { nil }
    }

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
            rows: advertisements.compactMap(DevicePickerDiscoveryCandidate.pickerRow(advertisement:))
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
    public let deviceCard: MockupDeviceCard?
    public let safetyBars: [MockupSafetyBar]
    public let warningCard: MockupWarningCard?
    public let dashboardTiles: [MockupDashboardTile]
    public let summaryTitle: String?
    public let summaryRows: [MockupSummaryRow]
    public let faultCard: MockupFaultCard?
    public let tabs: [MockupScreenTab]
    public let bmsContent: MockupBmsContent?
    public let eucGarageSnapshot: EucGarageSnapshot?
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
        deviceCard: MockupDeviceCard? = nil,
        safetyBars: [MockupSafetyBar] = [],
        warningCard: MockupWarningCard? = nil,
        dashboardTiles: [MockupDashboardTile] = [],
        summaryTitle: String? = nil,
        summaryRows: [MockupSummaryRow] = [],
        faultCard: MockupFaultCard? = nil,
        tabs: [MockupScreenTab] = [],
        bmsContent: MockupBmsContent? = nil,
        eucGarageSnapshot: EucGarageSnapshot? = nil,
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
        self.eucGarageSnapshot = eucGarageSnapshot
        self.isFixtureOnly = isFixtureOnly
    }

    public func resolvedBmsContent(liveSnapshot: BmsSnapshot?) -> MockupBmsContent? {
        guard let bmsContent else {
            return nil
        }
        guard let liveSnapshot else {
            return bmsContent
        }
        return bmsContent.resolved(with: liveSnapshot, preferredScreenID: id)
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

    public func presentedScreen(for screen: MockupScreen, liveBmsSnapshot: BmsSnapshot?) -> MockupScreen {
        guard let liveBmsSnapshot else {
            return screen
        }

        let preferredScreenID = screen.id == .eucGarage ? MockupScreenID.eucGarage : screen.id
        let isBmsPresentation = screen.id == .eucGarage || screen.id.isBmsScreen

        guard isBmsPresentation else {
            return screen
        }

        let resolvedKind = MockupBmsScreenKind(liveSnapshot: liveBmsSnapshot, preferredScreenID: preferredScreenID)
        let bmsScreenID = screen.id == .eucGarage ? resolvedKind.presentationScreenID : screen.id

        guard let fixtureScreen = self.screen(id: bmsScreenID) else {
            return screen
        }

        let resolvedContent = fixtureScreen.resolvedBmsContent(liveSnapshot: liveBmsSnapshot)

        return MockupScreen(
            id: fixtureScreen.id,
            title: resolvedKind.liveTitle(snapshot: liveBmsSnapshot),
            subtitle: resolvedKind.liveSubtitle(snapshot: liveBmsSnapshot, fallback: fixtureScreen.subtitle),
            primaryValue: fixtureScreen.primaryValue,
            secondaryValue: resolvedKind.liveSecondaryValue(snapshot: liveBmsSnapshot, fallback: fixtureScreen.secondaryValue),
            warning: fixtureScreen.warning,
            metrics: fixtureScreen.metrics,
            pickerRows: fixtureScreen.pickerRows,
            discoveryCandidates: fixtureScreen.discoveryCandidates,
            deviceCard: fixtureScreen.deviceCard,
            safetyBars: fixtureScreen.safetyBars,
            warningCard: fixtureScreen.warningCard,
            dashboardTiles: fixtureScreen.dashboardTiles,
            summaryTitle: fixtureScreen.summaryTitle,
            summaryRows: fixtureScreen.summaryRows,
            faultCard: fixtureScreen.faultCard,
            tabs: fixtureScreen.tabs,
            bmsContent: resolvedContent,
            eucGarageSnapshot: fixtureScreen.eucGarageSnapshot,
            isFixtureOnly: fixtureScreen.isFixtureOnly
        )
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
        energyPercent: BatteryLevel(value: 72),
        voltage: Voltage(value: 81_600),
        cellDelta: VoltageDelta(value: 18),
        lowestGroupIndex: 17,
        highestTemperature: Temperature(value: 37_800),
        highestTemperatureLabel: "right pack",
        balancingSummary: "idle • top groups only",
        balancingDetail: "3 groups bleeding: 03, 11, 19",
        faultSummary: "no active faults",
        faultDetail: "last: under-voltage warning · 3 days ago",
        groups: (1...20).map { index in
            let voltage = Voltage(value: 4_089 - Int32(index % 5) * 4)
            return BmsGroupSnapshot(
                index: index,
                voltage: voltage,
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
        cellDelta: VoltageDelta(value: 12),
        groups: [
            BmsGroupSnapshot(index: 1, voltage: Voltage(value: 4_104), alertLevel: .nominal),
            BmsGroupSnapshot(index: 2, voltage: Voltage(value: 4_101), alertLevel: .nominal),
            BmsGroupSnapshot(index: 3, voltage: Voltage(value: 4_096), alertLevel: .warning),
            BmsGroupSnapshot(index: 4, voltage: Voltage(value: 4_099), alertLevel: .nominal),
            BmsGroupSnapshot(index: 5, voltage: Voltage(value: 4_103), alertLevel: .nominal),
            BmsGroupSnapshot(index: 6, voltage: Voltage(value: 4_092), alertLevel: .warning),
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
        cellDelta: VoltageDelta(value: 18),
        highestTemperature: Temperature(value: 31_000),
        highestTemperatureLabel: "group 31",
        groups: (1...40).map { index in
            let alertLevel: BmsAlertLevel = [17, 18, 19].contains(index) ? .warning : index == 31 ? .critical : .nominal
            let base = 4_080 + Int32(index % 3) * 6
            let value = [17, 18, 19].contains(index) ? 4_080 : (index == 31 ? 4_072 : base)
            return BmsGroupSnapshot(index: index, voltage: Voltage(value: value), alertLevel: alertLevel)
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
        cellDelta: VoltageDelta(value: 18),
        groups: (1...20).map { index in
            let voltage = Voltage(value: index == 17 ? 4_071 : 4_086)
            let temperature = Temperature(value: index == 17 ? 34_900 : 33_000)
            let resistance = Resistance(value: index == 17 ? 21 : 18)
            return BmsGroupSnapshot(
                index: index,
                voltage: voltage,
                temperature: temperature,
                resistance: resistance,
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
        voltage: Voltage(value: 75_900),
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
        energyPercent: BatteryLevel(value: 71),
        voltage: Voltage(value: 117_600),
        current: BatteryCurrent(value: 38_000),
        captureActionTitle: "Trust sag, alarms, and headroom more than percent.",
        captureActionState: "limited data"
    )

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
            id: .bmsOverview,
            title: "Pack overview",
            subtitle: "CutOut · BMS",
            primaryValue: "72%",
            secondaryValue: "sag adjusted",
            warning: nil,
            metrics: [
                MockupMetric(label: "topology", value: "20S4P split pack"),
                MockupMetric(label: "BMS online", value: "2"),
                MockupMetric(label: "lowest group", value: "group 17"),
                MockupMetric(label: "highest temp", value: "37.8 °C"),
            ],
            bmsContent: MockupBmsContent(
                kind: .overview,
                snapshot: bmsOverviewSnapshot,
                chips: [
                    MockupBmsChip(title: "20S4P split pack", accent: .yellow),
                    MockupBmsChip(title: "2 BMS online", accent: .green),
                ]
            )
        ),
        MockupScreen(
            id: .bmsCellMap6S,
            title: "6S cell map",
            subtitle: "skateboard pack",
            primaryValue: "12 mV spread",
            secondaryValue: "no scrolling needed",
            warning: nil,
            metrics: [
                MockupMetric(label: "topology", value: "6S2P"),
                MockupMetric(label: "display", value: "all groups inline"),
            ],
            bmsContent: MockupBmsContent(
                kind: .cellMapInline,
                snapshot: bmsInlineSnapshot,
                chips: [
                    MockupBmsChip(title: "skateboard pack", accent: .cyan),
                    MockupBmsChip(title: "6S2P", accent: .yellow),
                ],
                highlightedGroupIndices: [3, 6],
                modeTitles: ["balance view", "temps", "faults"]
            )
        ),
        MockupScreen(
            id: .bmsCellMap40S,
            title: "40S cell map",
            subtitle: "large EUC pack",
            primaryValue: "17–19 sagging under load",
            secondaryValue: "scroll cells horizontally",
            warning: nil,
            metrics: [
                MockupMetric(label: "topology", value: "40S4P"),
                MockupMetric(label: "display", value: "overview first"),
            ],
            bmsContent: MockupBmsContent(
                kind: .cellMapScrollable,
                snapshot: bmsScrollableSnapshot,
                chips: [
                    MockupBmsChip(title: "large EUC pack", accent: .cyan),
                    MockupBmsChip(title: "40S4P", accent: .yellow),
                    MockupBmsChip(title: "scroll cells horizontally", accent: .orange),
                ],
                highlightedGroupIndices: [17, 18, 19, 31],
                modeTitles: ["overview", "strip", "full cell table", "popover"]
            )
        ),
        MockupScreen(
            id: .bmsCellDetail,
            title: "Cell detail",
            subtitle: "from any map",
            primaryValue: "4.071 V",
            secondaryValue: "group 17",
            warning: nil,
            metrics: [
                MockupMetric(label: "temp", value: "34.9 °C"),
                MockupMetric(label: "IR est.", value: "21 mΩ"),
            ],
            bmsContent: MockupBmsContent(
                kind: .cellDetail,
                snapshot: bmsDetailSnapshot,
                chips: [
                    MockupBmsChip(title: "from any map", accent: .cyan),
                    MockupBmsChip(title: "group 17", accent: .orange),
                ],
                highlightedGroupIndices: [17],
                selectedGroupIndex: 17
            )
        ),
        MockupScreen(
            id: .bmsUnknownTopology,
            title: "Unknown BMS",
            subtitle: "partial data",
            primaryValue: "BMS found, map unknown",
            secondaryValue: "topology unverified",
            warning: nil,
            metrics: [
                MockupMetric(label: "reported voltage", value: "75.9 V"),
                MockupMetric(label: "fault bits", value: "0x0040"),
                MockupMetric(label: "capture flow", value: "disabled for launch"),
            ],
            bmsContent: MockupBmsContent(
                kind: .unknownTopology,
                snapshot: bmsUnknownSnapshot,
                chips: [
                    MockupBmsChip(title: "partial data", accent: .orange),
                    MockupBmsChip(title: "topology unverified", accent: .green),
                ]
            )
        ),
        MockupScreen(
            id: .bmsNoData,
            title: "Battery",
            subtitle: "EX30 · non-smart BMS · controller-only estimate",
            primaryValue: "71%",
            secondaryValue: "limited data",
            warning: nil,
            metrics: [
                MockupMetric(label: "pack voltage", value: "117.6 V"),
                MockupMetric(label: "ride sag", value: "4.8 V"),
                MockupMetric(label: "load now", value: "38 A"),
            ],
            bmsContent: MockupBmsContent(
                kind: .noData,
                snapshot: bmsNoDataSnapshot,
                chips: [
                    MockupBmsChip(title: "limited data", accent: .yellow),
                ]
            )
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
            ],
            deviceCard: MockupDeviceCard(
                title: "Aero-126V",
                detail: "126 V nominal · 20s? mapped profile · BLE",
                status: "Safe",
                accent: .green
            ),
            dashboardTiles: [
                MockupDashboardTile(label: "battery", value: "85", unit: "%", detail: "115.8 V", accent: .cyan),
                MockupDashboardTile(kind: .beepMargin, label: "beep margin", value: "11.6", unit: "mph", detail: "to configured alarm", accent: .yellow),
                MockupDashboardTile(kind: .tiltback, label: "tiltback", value: "42", unit: "mph", detail: "wheel setting", accent: .orange),
                MockupDashboardTile(kind: .pedalMode, label: "pedal mode", value: "72", unit: "%", detail: "hardness normalized", accent: .purple),
            ],
            summaryTitle: "Cell / BMS summary",
            summaryRows: [
                MockupSummaryRow(label: "high group", value: "4.18 V", accent: nil),
                MockupSummaryRow(label: "low group", value: "4.13 V", accent: nil),
                MockupSummaryRow(label: "delta", value: "0.05 V", accent: .green),
            ],
            faultCard: MockupFaultCard(title: "Last fault", detail: "none since 38.2 mi ago", accent: .green),
            eucGarageSnapshot: EucGarageSnapshot(
                pack: EucPackHealthSnapshot(
                    energyPercent: BatteryLevel(value: 85),
                    voltage: Voltage(value: 115_800),
                    highGroupVoltage: Voltage(value: 4_180),
                    lowGroupVoltage: Voltage(value: 4_130),
                    cellDelta: VoltageDelta(value: 50)
                ),
                settings: EucGarageSettingsSnapshot(
                    beepMargin: .available(Speed(value: 5_186)),
                    tiltback: .available(Speed(value: 18_776)),
                    pedalMode: .available(PedalMode(hardnessPercent: 72))
                ),
                faultHistory: .none(sinceDistance: Distance(value: 61_456_941))
            )
        ),
        MockupScreen(
            id: .vescOnewheelRide,
            title: "Fungineers X7",
            subtitle: "VESC OW · armed",
            primaryValue: "19",
            secondaryValue: "board speed",
            warning: "Pushback soon - duty and pack sag are both climbing.",
            metrics: [
                MockupMetric(label: "battery current", value: "38 A"),
                MockupMetric(label: "motor current", value: "71 A"),
                MockupMetric(label: "board angle", value: "-1.8 deg"),
                MockupMetric(label: "controller", value: "54 C"),
                MockupMetric(label: "motor", value: "49 C"),
            ],
            safetyBars: [
                MockupSafetyBar(label: "Duty headroom", value: "18%", progress: 0.82, accent: .orange),
            ],
            warningCard: MockupWarningCard(
                title: "Pushback soon",
                detail: "Duty and pack sag are both climbing."
            ),
            dashboardTiles: [
                MockupDashboardTile(label: "battery current", value: "38", unit: "A", detail: "limit 45 A", accent: .yellow),
                MockupDashboardTile(label: "motor current", value: "71", unit: "A", detail: "phase estimate", accent: .orange),
                MockupDashboardTile(label: "board angle", value: "-1.8", unit: "°", detail: "nose down", accent: .cyan),
                MockupDashboardTile(label: "controller", value: "54", unit: "°C", detail: "motor 49 °C", accent: .green),
            ],
            tabs: [
                MockupScreenTab(title: "Ride", isSelected: true),
                MockupScreenTab(title: "VESC", isSelected: false),
                MockupScreenTab(title: "Map", isSelected: false),
                MockupScreenTab(title: "Logs", isSelected: false),
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
            ],
            deviceCard: MockupDeviceCard(
                title: "Profile: Street stable",
                detail: "VESC Express · FW 6.x · UART bridge",
                status: "",
                accent: .cyan
            ),
            dashboardTiles: [
                MockupDashboardTile(label: "duty cycle", value: "82", unit: "%", detail: "max seen 87%", accent: .orange),
                MockupDashboardTile(label: "pack", value: "75.4", unit: "V", detail: "20s lithium", accent: .cyan),
                MockupDashboardTile(label: "battery limit", value: "45", unit: "A", detail: "current max", accent: .yellow),
                MockupDashboardTile(label: "motor limit", value: "90", unit: "A", detail: "phase current", accent: .orange),
            ],
            summaryTitle: "Fault / app channels",
            summaryRows: [
                MockupSummaryRow(label: "last fault", value: "FAULT_CODE_NONE", accent: .green),
                MockupSummaryRow(label: "input app", value: "ADC + balance", accent: nil),
                MockupSummaryRow(label: "CAN status", value: "single controller", accent: nil),
                MockupSummaryRow(label: "logging", value: "local CSV armed", accent: .yellow),
            ],
            faultCard: MockupFaultCard(
                title: "Guardrails",
                detail: "Hide dangerous writes until parked + confirmed.",
                accent: .orange
            )
        ),
    ])
}

private extension MockupScreenID {
    var isBmsScreen: Bool {
        switch self {
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData:
            true
        case .devicePicker, .eucRide, .eucGarage, .vescOnewheelRide, .vescDebug:
            false
        }
    }
}

private extension MockupBmsScreenKind {
    var presentationScreenID: MockupScreenID {
        switch self {
        case .overview:
            .bmsOverview
        case .cellMapInline:
            .bmsCellMap6S
        case .cellMapScrollable, .cellDetail:
            .bmsCellMap40S
        case .unknownTopology:
            .bmsUnknownTopology
        case .noData:
            .bmsNoData
        }
    }
}
