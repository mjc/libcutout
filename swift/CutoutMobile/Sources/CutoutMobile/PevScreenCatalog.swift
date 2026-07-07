import Foundation

public enum PevScreenID: String, CaseIterable, Equatable, Hashable, Sendable {
    case devicePicker
    case eucRide
    case eucMap
    case eucTune
    case liveActivity
    case bmsOverview
    case bmsCellMap6S
    case bmsCellMap40S
    case bmsCellDetail
    case bmsUnknownTopology
    case bmsNoData
    case eucGarage
    case vescDebug
    case vescMap
    case vescLogs
}

public enum DevicePickerConnectionRoute: String, Equatable, Hashable, Sendable {
    case electricUnicycle = "electric_unicycle"
    case vescOnewheel = "vesc_onewheel"
}

public typealias PevConnectionRoute = DevicePickerConnectionRoute

public struct PevMetric: Equatable, Hashable, Sendable {
    public let label: String
    public let value: String

    public init(label: String, value: String) {
        self.label = label
        self.value = value
    }
}

public enum PevAccent: String, Equatable, Hashable, Sendable {
    case cyan
    case green
    case orange
    case purple
    case yellow
}

public struct PevDeviceCard: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String
    public let status: String
    public let accent: PevAccent

    public init(title: String, detail: String, status: String, accent: PevAccent) {
        self.title = title
        self.detail = detail
        self.status = status
        self.accent = accent
    }
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
    case metric
    case beepMargin
    case tiltback
    case pedalMode
    case batteryCurrent
    case motorCurrent
    case boardAngle
    case controller
}

public struct PevDashboardTile: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { label }

    public let kind: PevDashboardTileKind
    public let label: String
    public let value: String
    public let unit: String
    public let detail: String
    public let accent: PevAccent

    public init(
        kind: PevDashboardTileKind = .metric,
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

public struct PevScreenTab: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { title }

    public let title: String
    public let isSelected: Bool
    public let destinationScreenID: PevScreenID?
    public let disabledReason: String?

    public var isEnabled: Bool {
        disabledReason == nil
    }

    public init(
        title: String,
        isSelected: Bool,
        destinationScreenID: PevScreenID? = nil,
        disabledReason: String? = nil
    ) {
        self.title = title
        self.isSelected = isSelected
        self.destinationScreenID = destinationScreenID
        self.disabledReason = disabledReason
    }
}

public struct PevSummaryRow: Equatable, Hashable, Sendable, Identifiable {
    public var id: String { label }

    public let label: String
    public let value: String
    public let accent: PevAccent?

    public init(label: String, value: String, accent: PevAccent?) {
        self.label = label
        self.value = value
        self.accent = accent
    }
}

public struct PevFaultCard: Equatable, Hashable, Sendable {
    public let title: String
    public let detail: String
    public let accent: PevAccent

    public init(title: String, detail: String, accent: PevAccent) {
        self.title = title
        self.detail = detail
        self.accent = accent
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

public struct PevBmsChip: Equatable, Hashable, Sendable {
    public let title: String
    public let accent: PevAccent

    public init(title: String, accent: PevAccent) {
        self.title = title
        self.accent = accent
    }
}

public struct PevBmsContent: Equatable, Hashable, Sendable {
    public let kind: PevBmsScreenKind
    public let snapshot: BmsSnapshot
    public let chips: [PevBmsChip]
    public let highlightedGroupIndices: [Int]
    public let selectedGroupIndex: Int?
    public let modeTitles: [String]

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
        self.modeTitles = modeTitles
    }

    public func resolved(with liveSnapshot: BmsSnapshot, preferredScreenID: PevScreenID) -> Self {
        let resolvedKind = PevBmsScreenKind(liveSnapshot: liveSnapshot, preferredScreenID: preferredScreenID)
        let selectedGroupIndex = resolvedKind == .cellDetail
            ? liveSnapshot.lowestGroupIndex ?? liveSnapshot.groups.first?.index
            : nil
        let highlightedGroupIndices = selectedGroupIndex.map { [$0] } ?? liveSnapshot.lowestGroupIndex.map { [$0] } ?? []
        let chips = resolvedKind.liveChips(snapshot: liveSnapshot, selectedGroupIndex: selectedGroupIndex)
        let modeTitles = resolvedKind.liveModeTitles(snapshot: liveSnapshot)

        return PevBmsContent(
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
        case .devicePicker, .eucRide, .eucMap, .eucTune, .liveActivity, .eucGarage, .vescDebug, .vescMap, .vescLogs:
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
            var chips = [PevBmsChip(title: snapshot.topology.layoutLabel, accent: .yellow)]
            if snapshot.topology.bmsCount > 0 {
                chips.append(PevBmsChip(title: "\(snapshot.topology.bmsCount) BMS online", accent: .green))
            }
            return chips
        case .cellMapInline, .cellMapScrollable:
            return [
                PevBmsChip(title: "live readback", accent: .cyan),
                PevBmsChip(title: snapshot.topology.layoutLabel, accent: .yellow),
            ]
        case .cellDetail:
            return [
                PevBmsChip(title: "live readback", accent: .cyan),
                PevBmsChip(title: selectedGroupIndex.map { "group \($0)" } ?? "selected group", accent: .orange),
            ]
        case .unknownTopology:
            return [
                PevBmsChip(title: "partial data", accent: .orange),
                PevBmsChip(title: "topology unverified", accent: .green),
            ]
        case .noData:
            return [
                PevBmsChip(title: snapshot.captureActionState ?? "limited data", accent: .yellow),
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
    case manualPlaceholder(disabledReason: String)
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
            self = .manualPlaceholder(disabledReason: dto.disabledReason ?? dto.detail)
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
        case .probeRecommended, .unknownRecordable, .knownUnsupported, .ambiguous, .conflicting, .rejectedNoise, .manualPlaceholder, .unsupported:
            false
        }
    }

    var connectionRoute: DevicePickerConnectionRoute? {
        switch self {
        case .supported(let connectionRoute, _), .provisionalRoute(let connectionRoute, _):
            connectionRoute
        case .probeRecommended, .unknownRecordable, .knownUnsupported, .ambiguous, .conflicting, .rejectedNoise, .manualPlaceholder, .unsupported:
            nil
        }
    }

    var electricUnicycleModel: ElectricUnicycleModel? {
        switch self {
        case .supported(_, let electricUnicycleModel), .provisionalRoute(_, let electricUnicycleModel):
            electricUnicycleModel
        case .probeRecommended, .unknownRecordable, .knownUnsupported, .ambiguous, .conflicting, .rejectedNoise, .manualPlaceholder, .unsupported:
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
        case .manualPlaceholder:
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
    public let primaryValue: String
    public let secondaryValue: String
    public let warning: String?
    public let metrics: [PevMetric]
    public let pickerRows: [PevPickerRow]
    public let discoveryCandidates: [DevicePickerDiscoveryCandidate]
    public let deviceCard: PevDeviceCard?
    public let safetyBars: [PevSafetyBar]
    public let warningCard: PevWarningCard?
    public let dashboardTiles: [PevDashboardTile]
    public let summaryTitle: String?
    public let summaryRows: [PevSummaryRow]
    public let faultCard: PevFaultCard?
    public let tabs: [PevScreenTab]
    public let bmsContent: PevBmsContent?
    public let eucGarageSnapshot: EucGarageSnapshot?
    public let isPreviewOnly: Bool

    public init(
        id: PevScreenID,
        title: String,
        subtitle: String,
        primaryValue: String,
        secondaryValue: String,
        warning: String?,
        metrics: [PevMetric],
        pickerRows: [PevPickerRow] = [],
        discoveryCandidates: [DevicePickerDiscoveryCandidate] = [],
        deviceCard: PevDeviceCard? = nil,
        safetyBars: [PevSafetyBar] = [],
        warningCard: PevWarningCard? = nil,
        dashboardTiles: [PevDashboardTile] = [],
        summaryTitle: String? = nil,
        summaryRows: [PevSummaryRow] = [],
        faultCard: PevFaultCard? = nil,
        tabs: [PevScreenTab] = [],
        bmsContent: PevBmsContent? = nil,
        eucGarageSnapshot: EucGarageSnapshot? = nil,
        isPreviewOnly: Bool = true
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
        self.isPreviewOnly = isPreviewOnly
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
        liveBmsSnapshot: BmsSnapshot?,
        previewFallback: Bool = true
    ) -> PevScreen {
        guard let liveBmsSnapshot else {
            guard !previewFallback, screen.id == .eucGarage || screen.id.isBmsScreen else {
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

        guard let fixtureScreen = self.screen(id: bmsScreenID) else {
            return screen
        }

        let resolvedContent = fixtureScreen.resolvedBmsContent(liveSnapshot: liveBmsSnapshot)

        return PevScreen(
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
            isPreviewOnly: fixtureScreen.isPreviewOnly
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
            primaryValue: "--",
            secondaryValue: "no live BMS",
            warning: nil,
            metrics: [],
            bmsContent: PevBmsContent(
                kind: .noData,
                snapshot: snapshot,
                chips: [
                    PevBmsChip(title: "no live BMS", accent: .yellow),
                ]
            ),
            isPreviewOnly: false
        )
    }

    private static let devicePickerDiscoveryCandidates = [
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "demo:aero",
            displayName: "Aero-126V",
            productCategory: "Electric unicycle",
            evidence: "telemetry profile found",
            detail: "126.0 V - strong signal",
            support: .supported(connectionRoute: .electricUnicycle, electricUnicycleModel: .aero),
            symbolName: "circle.hexagongrid.circle"
        ),
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "demo:little-focer",
            displayName: "Little FOCer BT",
            productCategory: "VESC Onewheel",
            evidence: "UART bridge detected",
            detail: "75.4 V - moderate signal",
            support: .supported(connectionRoute: .vescOnewheel, electricUnicycleModel: nil),
            symbolName: "oval.portrait"
        ),
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "demo:ninebot",
            displayName: "NINEBOT-7A31",
            productCategory: "Electric scooter",
            evidence: "known BLE advertisement",
            detail: "We can learn this later",
            support: .unsupported(disabledReason: "Not yet"),
            symbolName: "scooter"
        ),
        DevicePickerDiscoveryCandidate(
            platformIdentifier: "demo:hx-hoverboard",
            displayName: "HX Hoverboard",
            productCategory: "Hoverboard / self-balancing board",
            evidence: "candidate",
            detail: "Capture wizard later",
            support: .unsupported(disabledReason: "Not yet"),
            symbolName: "capsule"
        ),
    ]

    private static let manualDiscoveryCandidate =
        DevicePickerDiscoveryCandidate(candidate: mobileManualDiscoveryCandidate())

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

    private static func placeholderScreen(
        id: PevScreenID,
        title: String,
        subtitle: String,
        reason: String
    ) -> PevScreen {
        PevScreen(
            id: id,
            title: title,
            subtitle: subtitle,
            primaryValue: "not wired",
            secondaryValue: "placeholder route",
            warning: reason,
            metrics: [
                PevMetric(label: "status", value: "placeholder"),
                PevMetric(label: "tracker", value: "LIBCU-423"),
            ]
        )
    }

    public static let v2 = PevScreenCatalog(screens: [
        PevScreen(
            id: .devicePicker,
            title: "Device picker",
            subtitle: "Scanning Bluetooth",
            primaryValue: "Aero-126V",
            secondaryValue: "Little FOCer BT",
            warning: "Unsupported rows remain disabled fixtures.",
            metrics: [
                PevMetric(label: "Supported EUC", value: "Aero-126V"),
                PevMetric(label: "Supported VESC OW", value: "Little FOCer BT"),
                PevMetric(label: "Unsupported", value: "NINEBOT-7A31"),
                PevMetric(label: "Unsupported", value: "HX Hoverboard"),
                PevMetric(label: "Manual add", value: "disabled"),
            ],
            pickerRows: (devicePickerDiscoveryCandidates + [manualDiscoveryCandidate]).map(\.pickerRow),
            discoveryCandidates: devicePickerDiscoveryCandidates + [manualDiscoveryCandidate]
        ),
        PevScreen(
            id: .eucRide,
            title: "Aero-126V",
            subtitle: "EUC - riding",
            primaryValue: "31 mph",
            secondaryValue: "PWM headroom 23%",
            warning: "Reduce acceleration - voltage sag under load: 9.4 V",
            metrics: [
                PevMetric(label: "sag-adjusted energy", value: "62%"),
                PevMetric(label: "pack", value: "115.8 V"),
                PevMetric(label: "power", value: "4.2 kW"),
                PevMetric(label: "thermal", value: "61 °C"),
                PevMetric(label: "limp-home", value: "14.2 mi"),
            ],
            safetyBars: [
                PevSafetyBar(label: "PWM headroom", value: "23%", progress: 0.77, accent: .yellow),
                PevSafetyBar(label: "sag-adjusted energy", value: "62%", progress: 0.62, accent: .cyan),
            ],
            warningCard: PevWarningCard(
                title: "Reduce acceleration",
                detail: "Voltage sag under load: 9.4 V"
            ),
            dashboardTiles: [
                PevDashboardTile(label: "pack", value: "115.8", unit: "V", detail: "-9.4 V sag", accent: .cyan),
                PevDashboardTile(label: "power", value: "4.2", unit: "kW", detail: "regen -0.3 kW", accent: .yellow),
                PevDashboardTile(label: "thermal", value: "61", unit: "°C", detail: "ESC 48 · motor 61", accent: .green),
                PevDashboardTile(label: "limp-home", value: "14.2", unit: "mi", detail: "at this pace", accent: .cyan),
            ],
            tabs: [
                PevScreenTab(title: "Ride", isSelected: true),
                PevScreenTab(title: "Pack", isSelected: false, destinationScreenID: .bmsOverview),
                PevScreenTab(title: "Map", isSelected: false, disabledReason: "LIBCU-423"),
                PevScreenTab(title: "Tune", isSelected: false, disabledReason: "LIBCU-423"),
            ]
        ),
        placeholderScreen(
            id: .eucMap,
            title: "Map",
            subtitle: "EUC map shell",
            reason: "EUC map view is not wired yet."
        ),
        placeholderScreen(
            id: .eucTune,
            title: "Tune",
            subtitle: "EUC tune/settings shell",
            reason: "EUC tune/settings view is not wired yet."
        ),
        PevScreen(
            id: .liveActivity,
            title: "Live Activity",
            subtitle: "preview-only surface for compact, expanded, and lock screen states",
            primaryValue: "3 layouts",
            secondaryValue: "matrix driven",
            warning: nil,
            metrics: [
                PevMetric(label: "preview states", value: "demo, populated, partial, waiting, stale, disconnected, parked"),
                PevMetric(label: "presentation modes", value: "compact, expanded, lock screen"),
            ],
            isPreviewOnly: true
        ),
        PevScreen(
            id: .bmsOverview,
            title: "Pack overview",
            subtitle: "CutOut · BMS",
            primaryValue: "72%",
            secondaryValue: "sag adjusted",
            warning: nil,
            metrics: [
                PevMetric(label: "topology", value: "20S4P split pack"),
                PevMetric(label: "BMS online", value: "2"),
                PevMetric(label: "lowest group", value: "group 17"),
                PevMetric(label: "highest temp", value: "37.8 °C"),
            ],
            bmsContent: PevBmsContent(
                kind: .overview,
                snapshot: bmsOverviewSnapshot,
                chips: [
                    PevBmsChip(title: "20S4P split pack", accent: .yellow),
                    PevBmsChip(title: "2 BMS online", accent: .green),
                ]
            )
        ),
        PevScreen(
            id: .bmsCellMap6S,
            title: "6S cell map",
            subtitle: "skateboard pack",
            primaryValue: "12 mV spread",
            secondaryValue: "no scrolling needed",
            warning: nil,
            metrics: [
                PevMetric(label: "topology", value: "6S2P"),
                PevMetric(label: "display", value: "all groups inline"),
            ],
            bmsContent: PevBmsContent(
                kind: .cellMapInline,
                snapshot: bmsInlineSnapshot,
                chips: [
                    PevBmsChip(title: "skateboard pack", accent: .cyan),
                    PevBmsChip(title: "6S2P", accent: .yellow),
                ],
                highlightedGroupIndices: [3, 6],
                modeTitles: ["balance view", "temps", "faults"]
            )
        ),
        PevScreen(
            id: .bmsCellMap40S,
            title: "40S cell map",
            subtitle: "large EUC pack",
            primaryValue: "17–19 sagging under load",
            secondaryValue: "scroll cells horizontally",
            warning: nil,
            metrics: [
                PevMetric(label: "topology", value: "40S4P"),
                PevMetric(label: "display", value: "overview first"),
            ],
            bmsContent: PevBmsContent(
                kind: .cellMapScrollable,
                snapshot: bmsScrollableSnapshot,
                chips: [
                    PevBmsChip(title: "large EUC pack", accent: .cyan),
                    PevBmsChip(title: "40S4P", accent: .yellow),
                    PevBmsChip(title: "scroll cells horizontally", accent: .orange),
                ],
                highlightedGroupIndices: [17, 18, 19, 31],
                modeTitles: ["overview", "strip", "full cell table", "popover"]
            )
        ),
        PevScreen(
            id: .bmsCellDetail,
            title: "Cell detail",
            subtitle: "from any map",
            primaryValue: "4.071 V",
            secondaryValue: "group 17",
            warning: nil,
            metrics: [
                PevMetric(label: "temp", value: "34.9 °C"),
                PevMetric(label: "IR est.", value: "21 mΩ"),
            ],
            bmsContent: PevBmsContent(
                kind: .cellDetail,
                snapshot: bmsDetailSnapshot,
                chips: [
                    PevBmsChip(title: "from any map", accent: .cyan),
                    PevBmsChip(title: "group 17", accent: .orange),
                ],
                highlightedGroupIndices: [17],
                selectedGroupIndex: 17
            )
        ),
        PevScreen(
            id: .bmsUnknownTopology,
            title: "Unknown BMS",
            subtitle: "partial data",
            primaryValue: "BMS found, map unknown",
            secondaryValue: "topology unverified",
            warning: nil,
            metrics: [
                PevMetric(label: "reported voltage", value: "75.9 V"),
                PevMetric(label: "fault bits", value: "0x0040"),
                PevMetric(label: "capture flow", value: "disabled for launch"),
            ],
            bmsContent: PevBmsContent(
                kind: .unknownTopology,
                snapshot: bmsUnknownSnapshot,
                chips: [
                    PevBmsChip(title: "partial data", accent: .orange),
                    PevBmsChip(title: "topology unverified", accent: .green),
                ]
            )
        ),
        PevScreen(
            id: .bmsNoData,
            title: "Battery",
            subtitle: "EX30 · non-smart BMS · controller-only estimate",
            primaryValue: "71%",
            secondaryValue: "limited data",
            warning: nil,
            metrics: [
                PevMetric(label: "pack voltage", value: "117.6 V"),
                PevMetric(label: "ride sag", value: "4.8 V"),
                PevMetric(label: "load now", value: "38 A"),
            ],
            bmsContent: PevBmsContent(
                kind: .noData,
                snapshot: bmsNoDataSnapshot,
                chips: [
                    PevBmsChip(title: "limited data", accent: .yellow),
                ]
            )
        ),
        PevScreen(
            id: .eucGarage,
            title: "EUC health",
            subtitle: "Stationary diagnostics for wheel-specific data",
            primaryValue: "battery 85%",
            secondaryValue: "pack 115.8 V",
            warning: nil,
            metrics: [
                PevMetric(label: "beep margin", value: "11.6 mph"),
                PevMetric(label: "tiltback", value: "42 mph"),
                PevMetric(label: "pedal mode", value: "72%"),
                PevMetric(label: "cell delta", value: "0.018 V"),
                PevMetric(label: "last fault", value: "none"),
            ],
            deviceCard: PevDeviceCard(
                title: "Aero-126V",
                detail: "126 V nominal · 20s? mapped profile · BLE",
                status: "Safe",
                accent: .green
            ),
            dashboardTiles: [
                PevDashboardTile(label: "battery", value: "85", unit: "%", detail: "115.8 V", accent: .cyan),
                PevDashboardTile(kind: .beepMargin, label: "beep margin", value: "11.6", unit: "mph", detail: "to configured alarm", accent: .yellow),
                PevDashboardTile(kind: .tiltback, label: "tiltback", value: "42", unit: "mph", detail: "wheel setting", accent: .orange),
                PevDashboardTile(kind: .pedalMode, label: "pedal mode", value: "72", unit: "%", detail: "hardness normalized", accent: .purple),
            ],
            summaryTitle: "Cell / BMS summary",
            summaryRows: [
                PevSummaryRow(label: "high group", value: "4.18 V", accent: nil),
                PevSummaryRow(label: "low group", value: "4.13 V", accent: nil),
                PevSummaryRow(label: "delta", value: "0.05 V", accent: .green),
            ],
            faultCard: PevFaultCard(title: "Last fault", detail: "none since 38.2 mi ago", accent: .green),
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
        PevScreen(
            id: .vescDebug,
            title: "VESC state",
            subtitle: "For tuning/debug. Not the riding screen",
            primaryValue: "duty cycle 82%",
            secondaryValue: "pack 75.4 V",
            warning: "Dangerous writes hidden until parked and confirmed.",
            metrics: [
                PevMetric(label: "battery limit", value: "45 A"),
                PevMetric(label: "motor limit", value: "90 A"),
                PevMetric(label: "last fault", value: "FAULT_CODE_NONE"),
                PevMetric(label: "input app", value: "ADC + balance"),
                PevMetric(label: "logging", value: "local CSV armed"),
            ],
            deviceCard: PevDeviceCard(
                title: "Profile: Street stable",
                detail: "VESC Express · FW 6.x · UART bridge",
                status: "",
                accent: .cyan
            ),
            dashboardTiles: [
                PevDashboardTile(label: "duty cycle", value: "82", unit: "%", detail: "max seen 87%", accent: .orange),
                PevDashboardTile(label: "pack", value: "75.4", unit: "V", detail: "20s lithium", accent: .cyan),
                PevDashboardTile(label: "battery limit", value: "45", unit: "A", detail: "current max", accent: .yellow),
                PevDashboardTile(label: "motor limit", value: "90", unit: "A", detail: "phase current", accent: .orange),
            ],
            summaryTitle: "Fault / app channels",
            summaryRows: [
                PevSummaryRow(label: "last fault", value: "FAULT_CODE_NONE", accent: .green),
                PevSummaryRow(label: "input app", value: "ADC + balance", accent: nil),
                PevSummaryRow(label: "CAN status", value: "single controller", accent: nil),
                PevSummaryRow(label: "logging", value: "local CSV armed", accent: .yellow),
            ],
            faultCard: PevFaultCard(
                title: "Guardrails",
                detail: "Hide dangerous writes until parked + confirmed.",
                accent: .orange
            )
        ),
        placeholderScreen(
            id: .vescMap,
            title: "Map",
            subtitle: "VESC map shell",
            reason: "VESC map view is not wired yet."
        ),
        placeholderScreen(
            id: .vescLogs,
            title: "Logs",
            subtitle: "VESC logs shell",
            reason: "VESC logs view is not wired yet."
        ),
    ])
}

private extension PevScreenID {
    var isBmsScreen: Bool {
        switch self {
        case .bmsOverview, .bmsCellMap6S, .bmsCellMap40S, .bmsCellDetail, .bmsUnknownTopology, .bmsNoData:
            true
        case .devicePicker, .eucRide, .eucMap, .eucTune, .liveActivity, .eucGarage, .vescDebug, .vescMap, .vescLogs:
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
