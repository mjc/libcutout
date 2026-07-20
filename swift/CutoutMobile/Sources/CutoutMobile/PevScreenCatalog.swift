import Foundation

public enum PevScreenID: String, CaseIterable, Equatable, Hashable, Sendable {
    case eucRide
    case bmsOverview
    case bmsCellMap6S
    case bmsCellMap40S
    case bmsCellDetail
    case bmsUnknownTopology
    case bmsNoData
    case eucGarage
    case vescRide
    case vescDebug
}

public enum PevNavigationTarget: Equatable, Hashable, Sendable {
    case screen(PevScreenID)
    case vescRide
}

public enum DevicePickerConnectionRoute: String, Equatable, Hashable, Sendable {
    case electricUnicycle = "electric_unicycle"
    case vescOnewheel = "vesc_onewheel"
}

public typealias PevConnectionRoute = DevicePickerConnectionRoute

public enum PevAccent: String, Equatable, Hashable, Sendable {
    case cyan
    case green
    case orange
    case purple
    case yellow
}

public struct PevSafetyBar: Equatable, Hashable, Sendable {
    public let label: String
    public let value: String
    public let progress: Double
    public let accent: PevAccent

    public init(label: String, value: String, progress: Double, accent: PevAccent) {
        self.label = label
        self.value = value
        self.progress = progress
        self.accent = accent
    }
}

public struct PevWarningCard: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String

    public init(title: String, detail: String) {
        self.title = title
        self.detail = detail
    }
}

public enum PevDashboardTileKind: Equatable, Hashable, Sendable {
    case chargeEstimate
    case packVoltage
    case power
    case thermal
    case limpHomeRange
    case gpsSpeed
    case batteryVoltage
    case beepMargin
    case tiltback
    case pedalMode
    case batteryCurrent
    case motorCurrent
    case boardAngle
    case controller
    case dutyCycle
    case headroom
}

public struct PevDashboardTile: Equatable, Hashable, Sendable, Identifiable {
    public var id: PevDashboardTileKind { kind }

    public let kind: PevDashboardTileKind
    public let label: String
    public let value: String
    public let unit: String
    public let detail: String
    public let accent: PevAccent

    public init(
        kind: PevDashboardTileKind,
        label: String,
        value: String,
        unit: String,
        detail: String,
        accent: PevAccent
    ) {
        self.kind = kind
        self.label = label
        self.value = value
        self.unit = unit
        self.detail = detail
        self.accent = accent
    }
}

public enum PevScreenTabID: Equatable, Hashable, Sendable {
    case ride
    case pack
    case map
    case tune
    case debug
    case logs
}

public struct PevScreenTab: Equatable, Hashable, Sendable, Identifiable {
    public let id: PevScreenTabID

    public let title: String
    public let isSelected: Bool
    public let destinationScreenID: PevScreenID?
    public let destinationTarget: PevNavigationTarget?
    public let disabledReason: String?

    public var isEnabled: Bool {
        disabledReason == nil
    }

    public init(
        id: PevScreenTabID,
        title: String,
        isSelected: Bool,
        destinationScreenID: PevScreenID? = nil,
        destinationTarget: PevNavigationTarget? = nil,
        disabledReason: String? = nil
    ) {
        self.id = id
        self.title = title
        self.isSelected = isSelected
        self.destinationScreenID = destinationScreenID
        self.destinationTarget = destinationTarget ?? destinationScreenID.map { .screen($0) }
        self.disabledReason = disabledReason
    }
}

public enum PevBmsScreenKind: Equatable, Hashable, Sendable {
    case overview
    case cellMapInline
    case cellMapScrollable
    case cellDetail
    case unknownTopology
    case noData
}

public struct PevBmsChip: Identifiable, Equatable, Hashable, Sendable {
    public enum ID: String, Sendable {
        case topology
        case bmsStatus
        case liveReadback
        case selectedGroup
        case dataStatus
        case topologyStatus
        case captureStatus
        case availability
    }

    public let id: ID
    public let title: String
    public let accent: PevAccent

    public init(id: ID, title: String, accent: PevAccent) {
        self.id = id
        self.title = title
        self.accent = accent
    }
}

public struct PevBmsMode: Identifiable, Equatable, Hashable, Sendable {
    public let id: Int
    public let title: String
}

public struct PevBmsContent: Equatable, Hashable, Sendable {
    public let kind: PevBmsScreenKind
    public let snapshot: BmsSnapshot
    public let chips: [PevBmsChip]
    public let highlightedGroupIndices: [Int]
    public let selectedGroupIndex: Int?
    public let modes: [PevBmsMode]

    public var modeTitles: [String] {
        modes.map(\.title)
    }

    public init(
        kind: PevBmsScreenKind,
        snapshot: BmsSnapshot,
        chips: [PevBmsChip] = [],
        highlightedGroupIndices: [Int] = [],
        selectedGroupIndex: Int? = nil,
        modeTitles: [String] = []
    ) {
        self.kind = kind
        self.snapshot = snapshot
        self.chips = chips
        self.highlightedGroupIndices = highlightedGroupIndices
        self.selectedGroupIndex = selectedGroupIndex
        self.modes = modeTitles.enumerated().map { PevBmsMode(id: $0.offset, title: $0.element) }
    }

    public func resolved(with liveSnapshot: BmsSnapshot, preferredScreenID: PevScreenID) -> Self {
        Self.live(with: liveSnapshot, preferredScreenID: preferredScreenID)
    }

    public static func live(with liveSnapshot: BmsSnapshot, preferredScreenID: PevScreenID) -> Self {
        let resolvedKind = PevBmsScreenKind(liveSnapshot: liveSnapshot, preferredScreenID: preferredScreenID)
        let selectedGroupIndex = resolvedKind == .cellDetail
            ? liveSnapshot.lowestGroupIndex ?? liveSnapshot.groups.first?.index
            : nil
        let highlightedGroupIndices = selectedGroupIndex.map { [$0] } ?? liveSnapshot.lowestGroupIndex.map { [$0] } ?? []
        let chips = resolvedKind.liveChips(snapshot: liveSnapshot, selectedGroupIndex: selectedGroupIndex)
        let modeTitles = resolvedKind.liveModeTitles(snapshot: liveSnapshot)

        return Self(
            kind: resolvedKind,
            snapshot: liveSnapshot,
            chips: chips,
            highlightedGroupIndices: highlightedGroupIndices,
            selectedGroupIndex: selectedGroupIndex,
            modeTitles: modeTitles
        )
    }
}

private extension PevBmsScreenKind {
    init(liveSnapshot: BmsSnapshot, preferredScreenID: PevScreenID) {
        if let explicitKind = Self(explicitScreenID: preferredScreenID) {
            self = explicitKind
            return
        }

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

    init?(explicitScreenID: PevScreenID) {
        switch explicitScreenID {
        case .bmsOverview:
            self = .overview
        case .bmsCellMap6S:
            self = .cellMapInline
        case .bmsCellMap40S:
            self = .cellMapScrollable
        case .bmsCellDetail:
            self = .cellDetail
        case .bmsUnknownTopology:
            self = .unknownTopology
        case .bmsNoData:
            self = .noData
        case .eucRide, .eucGarage, .vescRide, .vescDebug:
            return nil
        }
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

    func liveChips(snapshot: BmsSnapshot, selectedGroupIndex: Int?) -> [PevBmsChip] {
        switch self {
        case .overview:
            var chips = [PevBmsChip(id: .topology, title: snapshot.topology.layoutLabel, accent: .yellow)]
            if snapshot.topology.bmsCount > 0 {
                chips.append(PevBmsChip(id: .bmsStatus, title: "\(snapshot.topology.bmsCount) BMS online", accent: .green))
            }
            return chips
        case .cellMapInline, .cellMapScrollable:
            return [
                PevBmsChip(id: .liveReadback, title: "live readback", accent: .cyan),
                PevBmsChip(id: .topology, title: snapshot.topology.layoutLabel, accent: .yellow),
            ]
        case .cellDetail:
            return [
                PevBmsChip(id: .liveReadback, title: "live readback", accent: .cyan),
                PevBmsChip(id: .selectedGroup, title: selectedGroupIndex.map { "group \($0)" } ?? "selected group", accent: .orange),
            ]
        case .unknownTopology:
            return [
                PevBmsChip(id: .dataStatus, title: "partial data", accent: .orange),
                PevBmsChip(id: .topologyStatus, title: "topology unverified", accent: .green),
            ]
        case .noData:
            return [
                PevBmsChip(id: .captureStatus, title: snapshot.captureActionState ?? "limited data", accent: .yellow),
            ]
        }
    }

    func liveModeTitles(snapshot: BmsSnapshot) -> [String] {
        switch self {
        case .cellMapInline:
            snapshot.inlineCellMapModeTitles
        case .cellMapScrollable:
            snapshot.scrollableCellMapModeTitles
        case .overview, .cellDetail, .unknownTopology, .noData:
            []
        }
    }
}
public enum DevicePickerRowState: Equatable, Hashable, Sendable {
    case supported(action: String)
    case probeRecommended(action: String)
    case unsupported(action: String)
    case manual(action: String)
}

public typealias PevPickerRowState = DevicePickerRowState

public enum DevicePickerRowSection: Equatable, Hashable, Sendable {
    case supported
    case probeFirst
    case recordOnly
    case manual
}

public enum DevicePickerGlyphKind: Equatable, Hashable, Sendable {
    case electricUnicycle
    case onewheel
    case scooter
    case hoverboard
    case systemSymbol

    public init(symbolName: String) {
        switch symbolName {
        case "circle.hexagongrid.circle":
            self = .electricUnicycle
        case "oval.portrait":
            self = .onewheel
        case "scooter":
            self = .scooter
        case "capsule":
            self = .hoverboard
        default:
            self = .systemSymbol
        }
    }
}

public struct DevicePickerRow: Equatable, Hashable, Sendable, Identifiable {
    public let id: String

    public let title: String
    public let subtitle: String
    public let detail: String
    public let state: DevicePickerRowState
    public let section: DevicePickerRowSection
    public let symbolName: String
    public let glyphKind: DevicePickerGlyphKind
    public let connectionRoute: DevicePickerConnectionRoute?
    public let electricUnicycleModel: ElectricUnicycleModel?

    public init(
        id: String? = nil,
        title: String,
        subtitle: String,
        detail: String,
        state: DevicePickerRowState,
        section: DevicePickerRowSection? = nil,
        symbolName: String,
        glyphKind: DevicePickerGlyphKind? = nil,
        connectionRoute: DevicePickerConnectionRoute? = nil,
        electricUnicycleModel: ElectricUnicycleModel? = nil
    ) {
        self.id = id ?? title
        self.title = title
        self.subtitle = subtitle
        self.detail = detail
        self.state = state
        self.section = section ?? DevicePickerRowSection(state: state)
        self.symbolName = symbolName
        self.glyphKind = glyphKind ?? DevicePickerGlyphKind(symbolName: symbolName)
        self.connectionRoute = connectionRoute
        self.electricUnicycleModel = electricUnicycleModel
    }
}

public typealias PevPickerRow = DevicePickerRow

public extension DevicePickerRow {
    var isSupported: Bool {
        section == .supported
    }

    var isUnsupported: Bool {
        section == .recordOnly
    }

    var isManual: Bool {
        section == .manual
    }

    var isProbeRecommended: Bool {
        section == .probeFirst
    }

    var captureActionTitle: String {
        isProbeRecommended ? "Start probe" : "Start capture"
    }
}

public extension DevicePickerRowState {
    init(action: DiscoveryCandidateAction) {
        switch action {
        case .use:
            self = .supported(action: "Use")
        case .probe:
            self = .probeRecommended(action: "Probe")
        case .record:
            self = .unsupported(action: "Record")
        case .confirm:
            self = .unsupported(action: "Confirm")
        case .review:
            self = .unsupported(action: "Review")
        case .later:
            self = .manual(action: "later")
        }
    }

    var actionTitle: String {
        switch self {
        case .supported(let action), .probeRecommended(let action), .unsupported(let action), .manual(let action):
            action
        }
    }

    var isSupported: Bool {
        if case .supported = self { true } else { false }
    }
}

public extension DevicePickerRowSection {
    init(state: DevicePickerRowState) {
        switch state {
        case .supported:
            self = .supported
        case .probeRecommended:
            self = .probeFirst
        case .unsupported:
            self = .recordOnly
        case .manual:
            self = .manual
        }
    }

    init(section: DiscoveryCandidateSection) {
        switch section {
        case .supported:
            self = .supported
        case .probeFirst:
            self = .probeFirst
        case .recordOnly:
            self = .recordOnly
        case .manual:
            self = .manual
        }
    }
}

public struct DevicePickerSections: Equatable, Hashable, Sendable {
    public let supported: [DevicePickerRow]
    public let probeRecommended: [DevicePickerRow]
    public let unsupported: [DevicePickerRow]
    public let manual: DevicePickerRow?

    public init(rows: [DevicePickerRow]) {
        supported = rows.filter { $0.section == .supported }
        probeRecommended = rows.filter { $0.section == .probeFirst }
        unsupported = rows.filter { $0.section == .recordOnly }
        manual = rows.first { $0.section == .manual }
    }
}

public typealias PevPickerSections = DevicePickerSections

public enum DevicePickerCandidateSupport: Equatable, Hashable, Sendable {
    case supported(connectionRoute: DevicePickerConnectionRoute?, electricUnicycleModel: ElectricUnicycleModel?)
    case provisionalRoute(connectionRoute: DevicePickerConnectionRoute?, electricUnicycleModel: ElectricUnicycleModel?)
    case probeRecommended(disabledReason: String)
    case unknownRecordable(disabledReason: String)
    case knownUnsupported(disabledReason: String)
    case ambiguous(disabledReason: String)
    case conflicting(disabledReason: String)
    case rejectedNoise(disabledReason: String)
    case manualEntry(disabledReason: String)
    case unsupported(disabledReason: String)
}

public extension DevicePickerCandidateSupport {
    init(_ dto: DiscoveryCandidate) {
        switch dto.support {
        case .supported:
            self = .supported(
                connectionRoute: dto.connectionRoute.map(DevicePickerConnectionRoute.init),
                electricUnicycleModel: dto.electricUnicycleModel.map(ElectricUnicycleModel.init)
            )
        case .provisionalRoute:
            self = .provisionalRoute(
                connectionRoute: dto.connectionRoute.map(DevicePickerConnectionRoute.init),
                electricUnicycleModel: dto.electricUnicycleModel.map(ElectricUnicycleModel.init)
            )
        case .probeRecommended:
            self = .probeRecommended(disabledReason: dto.disabledReason ?? dto.detail)
        case .unknownRecordable:
            self = .unknownRecordable(disabledReason: dto.disabledReason ?? dto.detail)
        case .knownUnsupported:
            self = .knownUnsupported(disabledReason: dto.disabledReason ?? dto.detail)
        case .ambiguous:
            self = .ambiguous(disabledReason: dto.disabledReason ?? dto.detail)
        case .conflicting:
            self = .conflicting(disabledReason: dto.disabledReason ?? dto.detail)
        case .rejectedNoise:
            self = .rejectedNoise(disabledReason: dto.disabledReason ?? dto.detail)
        case .manualPlaceholder:
            self = .manualEntry(disabledReason: dto.disabledReason ?? dto.detail)
        case .unsupported:
            self = .unsupported(disabledReason: dto.disabledReason ?? dto.detail)
        }
    }
}

private extension DevicePickerConnectionRoute {
    init(_ route: DiscoveryConnectionRoute) {
        switch route {
        case .electricUnicycle:
            self = .electricUnicycle
        case .vescOnewheel:
            self = .vescOnewheel
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
    public let glyphKind: DevicePickerGlyphKind
    public let rowState: DevicePickerRowState
    public let section: DevicePickerRowSection

    public init(
        platformIdentifier: String,
        displayName: String,
        productCategory: String,
        evidence: String,
        detail: String,
        support: DevicePickerCandidateSupport,
        symbolName: String,
        glyphKind: DevicePickerGlyphKind? = nil,
        rowState: DevicePickerRowState? = nil,
        section: DevicePickerRowSection? = nil
    ) {
        let state = rowState ?? support.pickerRowState
        self.platformIdentifier = platformIdentifier
        self.displayName = displayName
        self.productCategory = productCategory
        self.evidence = evidence
        self.detail = detail
        self.support = support
        self.symbolName = symbolName
        self.glyphKind = glyphKind ?? DevicePickerGlyphKind(symbolName: symbolName)
        self.rowState = state
        self.section = section ?? DevicePickerRowSection(state: state)
    }

    public init(advertisement: CoreBluetoothAdvertisement) {
        self.init(candidate: mobileDiscoveryCandidateFromAdvertisement(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            localName: advertisement.localName,
            advertisedServiceUuids: advertisement.advertisedServiceUuids.compactMap(\.bluetooth16Value)
        ))
    }

    public init(candidate: DiscoveryCandidate) {
        let support = DevicePickerCandidateSupport(candidate)
        self.init(
            platformIdentifier: candidate.platformIdentifier,
            displayName: candidate.displayName,
            productCategory: candidate.productCategory,
            evidence: candidate.evidence,
            detail: candidate.detail,
            support: support,
            symbolName: support.isSupported ? "circle.hexagongrid.circle" : "questionmark.circle",
            rowState: DevicePickerRowState(action: candidate.recommendedAction),
            section: DevicePickerRowSection(section: candidate.section)
        )
    }

    public static func pickerRow(advertisement: CoreBluetoothAdvertisement) -> DevicePickerRow? {
        let candidate = mobileDiscoveryCandidateFromAdvertisement(
            platformIdentifier: advertisement.peripheralIdentifier.rawValue,
            localName: advertisement.localName,
            advertisedServiceUuids: advertisement.advertisedServiceUuids.compactMap(\.bluetooth16Value)
        )
        guard candidate.isPickerCandidate else { return nil }
        return DevicePickerDiscoveryCandidate(candidate: candidate).pickerRow
    }

    public var pickerRow: DevicePickerRow {
        DevicePickerRow(
            id: platformIdentifier,
            title: displayName,
            subtitle: "\(productCategory) - \(evidence)",
            detail: detail,
            state: rowState,
            section: section,
            symbolName: symbolName,
            glyphKind: glyphKind,
            connectionRoute: support.connectionRoute,
            electricUnicycleModel: support.electricUnicycleModel
        )
    }
}

public extension DevicePickerCandidateSupport {
    var isSupported: Bool {
        switch self {
        case .supported, .provisionalRoute:
            true
        case .probeRecommended, .unknownRecordable, .knownUnsupported, .ambiguous, .conflicting, .rejectedNoise, .manualEntry, .unsupported:
            false
        }
    }

    var connectionRoute: DevicePickerConnectionRoute? {
        switch self {
        case .supported(let connectionRoute, _), .provisionalRoute(let connectionRoute, _):
            connectionRoute
        case .probeRecommended, .unknownRecordable, .knownUnsupported, .ambiguous, .conflicting, .rejectedNoise, .manualEntry, .unsupported:
            nil
        }
    }

    var electricUnicycleModel: ElectricUnicycleModel? {
        switch self {
        case .supported(_, let electricUnicycleModel), .provisionalRoute(_, let electricUnicycleModel):
            electricUnicycleModel
        case .probeRecommended, .unknownRecordable, .knownUnsupported, .ambiguous, .conflicting, .rejectedNoise, .manualEntry, .unsupported:
            nil
        }
    }

    var pickerRowState: DevicePickerRowState {
        switch self {
        case .supported, .provisionalRoute:
            .supported(action: "Use")
        case .probeRecommended:
            .probeRecommended(action: "Probe")
        case .unknownRecordable:
            .unsupported(action: "Record")
        case .knownUnsupported:
            .unsupported(action: "Record")
        case .ambiguous:
            .unsupported(action: "Confirm")
        case .conflicting:
            .unsupported(action: "Review")
        case .rejectedNoise:
            .unsupported(action: "Record")
        case .manualEntry:
            .manual(action: "later")
        case .unsupported:
            .unsupported(action: "Record")
        }
    }
}

public enum DevicePickerScanStatus: Equatable, Hashable, Sendable {
    case scanning
    case idle
    case bluetoothUnavailable
    case permissionDenied
    case failed(message: String)
}

public struct DevicePickerScanState: Equatable, Hashable, Sendable {
    public let status: DevicePickerScanStatus
    public let rows: [DevicePickerRow]

    public init(status: DevicePickerScanStatus, rows: [DevicePickerRow]) {
        self.status = status
        self.rows = rows
    }

    public init(status: DevicePickerScanStatus, advertisements: [CoreBluetoothAdvertisement]) {
        self.init(
            status: status,
            rows: advertisements.compactMap(DevicePickerDiscoveryCandidate.pickerRow(advertisement:))
        )
    }

    public init(status: DevicePickerScanStatus, discoverySnapshot: DiscoverySnapshot) {
        self.init(
            status: status,
            rows: discoverySnapshot.pickerCandidates.map {
                DevicePickerDiscoveryCandidate(candidate: $0).pickerRow
            }
        )
    }

    public var sections: DevicePickerSections {
        DevicePickerSections(rows: rows)
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
        case .failed(let message):
            message
        }
    }

    public static let scanning = DevicePickerScanState(status: .scanning, rows: [])
    public static let bluetoothUnavailable = DevicePickerScanState(status: .bluetoothUnavailable, rows: [])
    public static let permissionDenied = DevicePickerScanState(status: .permissionDenied, rows: [])

    public static func failed(_ message: String, rows: [DevicePickerRow] = []) -> DevicePickerScanState {
        DevicePickerScanState(status: .failed(message: message), rows: rows)
    }
}

public extension DevicePickerScanState {
    func storedSupportedRow(platformIdentifier: String?) -> DevicePickerRow? {
        guard let platformIdentifier else { return nil }
        return rows.first { $0.id == platformIdentifier && $0.isSupported }
    }
}

public struct PevScreen: Equatable, Hashable, Sendable, Identifiable {
    public let id: PevScreenID
    public let title: String
    public let subtitle: String
    public let secondaryValue: String
    public let bmsContent: PevBmsContent?

    public init(
        id: PevScreenID,
        title: String,
        subtitle: String,
        secondaryValue: String,
        bmsContent: PevBmsContent? = nil
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.secondaryValue = secondaryValue
        self.bmsContent = bmsContent
    }

    public func resolvedBmsContent(liveSnapshot: BmsSnapshot?) -> PevBmsContent? {
        guard let bmsContent else {
            return nil
        }
        guard let liveSnapshot else {
            return bmsContent
        }
        return bmsContent.resolved(with: liveSnapshot, preferredScreenID: id)
    }
}

public struct PevScreenCatalog: Equatable, Hashable, Sendable {
    public let screens: [PevScreen]

    public init(screens: [PevScreen]) {
        self.screens = screens
    }

    public func screen(id: PevScreenID) -> PevScreen? {
        screens.first { $0.id == id }
    }

    public func presentedScreen(
        for screen: PevScreen,
        liveBmsSnapshot: BmsSnapshot?
    ) -> PevScreen {
        guard let liveBmsSnapshot else {
            guard screen.id == .eucGarage || screen.id.isBmsScreen else {
                return screen
            }
            return Self.noLiveBmsReadbackScreen()
        }

        let preferredScreenID = screen.id == .eucGarage ? PevScreenID.eucGarage : screen.id
        let isBmsPresentation = screen.id == .eucGarage || screen.id.isBmsScreen

        guard isBmsPresentation else {
            return screen
        }

        let resolvedKind = PevBmsScreenKind(liveSnapshot: liveBmsSnapshot, preferredScreenID: preferredScreenID)
        let bmsScreenID = screen.id == .eucGarage ? resolvedKind.presentationScreenID : screen.id

        let metadata = self.screen(id: bmsScreenID) ?? screen
        let resolvedContent = PevBmsContent.live(with: liveBmsSnapshot, preferredScreenID: bmsScreenID)

        return PevScreen(
            id: metadata.id,
            title: resolvedKind.liveTitle(snapshot: liveBmsSnapshot),
            subtitle: resolvedKind.liveSubtitle(snapshot: liveBmsSnapshot, fallback: metadata.subtitle),
            secondaryValue: resolvedKind.liveSecondaryValue(snapshot: liveBmsSnapshot, fallback: "unavailable"),
            bmsContent: resolvedContent
        )
    }

    private static func noLiveBmsReadbackScreen() -> PevScreen {
        let snapshot = BmsSnapshot(
            availability: .unavailable,
            topology: BmsTopology(
                layoutLabel: "live BMS readback unavailable",
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 0,
                bmsCount: 0,
                confidence: .unverified
            )
        )

        return PevScreen(
            id: .bmsNoData,
            title: "Battery",
            subtitle: "live BMS readback unavailable",
            secondaryValue: "no live BMS",
            bmsContent: PevBmsContent(
                kind: .noData,
                snapshot: snapshot,
                chips: [
                    PevBmsChip(id: .availability, title: "no live BMS", accent: .yellow),
                ]
            ),
        )
    }

    public static let live = PevScreenCatalog(screens: [
        liveScreen(id: .eucRide, title: "EUC ride", subtitle: "Live telemetry"),
        liveScreen(id: .bmsOverview, title: "Battery", subtitle: "Live BMS readback"),
        liveScreen(id: .bmsCellMap6S, title: "Cell map", subtitle: "Live BMS readback"),
        liveScreen(id: .bmsCellMap40S, title: "Cell map", subtitle: "Live BMS readback"),
        liveScreen(id: .bmsCellDetail, title: "Cell detail", subtitle: "Live BMS readback"),
        liveScreen(id: .bmsUnknownTopology, title: "Battery", subtitle: "Topology unavailable"),
        liveScreen(id: .bmsNoData, title: "Battery", subtitle: "Live BMS readback unavailable"),
        liveScreen(id: .eucGarage, title: "EUC health", subtitle: "Live readbacks"),
        liveScreen(id: .vescRide, title: "VESC ride", subtitle: "Live telemetry"),
        liveScreen(id: .vescDebug, title: "VESC state", subtitle: "Live telemetry")
    ])

    private static func liveScreen(id: PevScreenID, title: String, subtitle: String) -> PevScreen {
        PevScreen(
            id: id,
            title: title,
            subtitle: subtitle,
            secondaryValue: "unavailable"
        )
    }
}

private extension PevScreenID {
    var isBmsScreen: Bool {
        switch self {
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData:
            true
        case .eucRide, .eucGarage, .vescRide, .vescDebug:
            false
        }
    }
}

private extension PevBmsScreenKind {
    var presentationScreenID: PevScreenID {
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
