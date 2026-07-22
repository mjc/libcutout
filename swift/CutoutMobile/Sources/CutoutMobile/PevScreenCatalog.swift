import CutoutMobileFFI
import Foundation

public enum PevScreenID: String, CaseIterable, Equatable, Hashable, Sendable {
    case eucRide
    case bmsOverview
    case bmsCellMap6S
    case bmsCellMap40S
    case bmsCellDetail
    case bmsUnknownTopology
    case bmsNoData
    case vescRide
    case vescDebug
}

public enum PevNavigationTarget: Equatable, Hashable, Sendable {
    case screen(PevScreenID)
    case eucPack
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

public enum PevSafetyBarID: Equatable, Hashable, Sendable {
    case pwmHeadroom
    case sagAdjustedEnergy
}

public struct PevSafetyBar: Equatable, Hashable, Sendable, Identifiable {
    public let id: PevSafetyBarID
    public let label: String
    public let value: String
    public let progress: Double
    public let accent: PevAccent

    public init(id: PevSafetyBarID, label: String, value: String, progress: Double, accent: PevAccent) {
        self.id = id
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

public enum PevDashboardMetricValue: Equatable, Hashable, Sendable {
    case available(display: String, accessibility: String)
    case unavailable

    public init(display: String, accessibility: String? = nil) {
        self = display == "--"
            ? .unavailable
            : .available(display: display, accessibility: accessibility ?? display)
    }

    public var displayText: String {
        switch self {
        case .available(let display, _): display
        case .unavailable: "--"
        }
    }

    public var accessibilityText: String {
        switch self {
        case .available(_, let accessibility): accessibility
        case .unavailable: "unavailable"
        }
    }

    public func accessibilityValue(unit: String, detail: String) -> String {
        guard case .available = self else { return accessibilityText }
        return [accessibilityText, unit, detail]
            .filter { !$0.isEmpty }
            .joined(separator: ", ")
    }
}

public struct PevDashboardTile: Equatable, Hashable, Sendable, Identifiable {
    public var id: PevDashboardTileKind { kind }

    public let kind: PevDashboardTileKind
    public let label: String
    public let metricValue: PevDashboardMetricValue
    public var value: String { metricValue.displayText }
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
        self.init(
            kind: kind,
            label: label,
            metricValue: .available(display: value, accessibility: value),
            unit: unit,
            detail: detail,
            accent: accent
        )
    }

    public init(
        kind: PevDashboardTileKind,
        label: String,
        metricValue: PevDashboardMetricValue,
        unit: String,
        detail: String,
        accent: PevAccent
    ) {
        self.kind = kind
        self.label = label
        self.metricValue = metricValue
        self.unit = unit
        self.detail = detail
        self.accent = accent
    }
}

public enum PevScreenTabID: String, Equatable, Hashable, Sendable {
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

    public var accessibilityIdentifier: String {
        "dashboard.nav.\(id.rawValue)"
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

public enum PevBmsMode: String, Identifiable, Equatable, Hashable, Sendable {
    case balanceView
    case temperatures
    case faults
    case overview
    case strip
    case rawTable

    public var id: Self { self }

    public var title: String {
        switch self {
        case .balanceView: "balance view"
        case .temperatures: "temps"
        case .faults: "faults"
        case .overview: "overview"
        case .strip: "strip"
        case .rawTable: "raw table"
        }
    }
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
        modes: [PevBmsMode] = []
    ) {
        self.kind = kind
        self.snapshot = snapshot
        self.chips = chips
        self.highlightedGroupIndices = highlightedGroupIndices
        self.selectedGroupIndex = selectedGroupIndex
        self.modes = modes
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
        let modes = resolvedKind.liveModes(snapshot: liveSnapshot)

        return Self(
            kind: resolvedKind,
            snapshot: liveSnapshot,
            chips: chips,
            highlightedGroupIndices: highlightedGroupIndices,
            selectedGroupIndex: selectedGroupIndex,
            modes: modes
        )
    }
}

private extension PevBmsScreenKind {
    init(liveSnapshot: BmsSnapshot, preferredScreenID: PevScreenID? = nil) {
        if let preferredScreenID, let explicitKind = Self(explicitScreenID: preferredScreenID) {
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
        case .eucRide, .vescRide, .vescDebug:
            return nil
        }
    }

    func liveTitle(snapshot: BmsSnapshot) -> String {
        switch self {
        case .overview:
            pevLocalizedText("bms.title.pack_overview")
        case .cellMapInline, .cellMapScrollable:
            snapshot.topology.seriesGroupCount.map { pevLocalizedText("bms.title.cell_map_series", Int64($0)) }
                ?? pevLocalizedText("bms.title.cell_map")
        case .cellDetail:
            pevLocalizedText("bms.title.cell_detail")
        case .unknownTopology:
            pevLocalizedText("bms.title.unknown")
        case .noData:
            pevLocalizedText("bms.title.battery")
        }
    }

    func liveSubtitle(snapshot: BmsSnapshot, fallback: String) -> String {
        switch self {
        case .noData:
            pevLocalizedText("bms.subtitle.controller_estimate", snapshot.topology.layoutLabel)
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
                chips.append(PevBmsChip(id: .bmsStatus, title: pevLocalizedText("bms.chip.bms_online", Int64(snapshot.topology.bmsCount)), accent: .green))
            }
            return chips
        case .cellMapInline, .cellMapScrollable:
            return [
                PevBmsChip(id: .liveReadback, title: pevLocalizedText("bms.chip.live_readback"), accent: .cyan),
                PevBmsChip(id: .topology, title: snapshot.topology.layoutLabel, accent: .yellow),
            ]
        case .cellDetail:
            return [
                PevBmsChip(id: .liveReadback, title: pevLocalizedText("bms.chip.live_readback"), accent: .cyan),
                PevBmsChip(id: .selectedGroup, title: selectedGroupIndex.map { pevLocalizedText("bms.chip.group", Int64($0)) } ?? pevLocalizedText("bms.chip.selected_group"), accent: .orange),
            ]
        case .unknownTopology:
            return [
                PevBmsChip(id: .dataStatus, title: pevLocalizedText("bms.chip.partial_data"), accent: .orange),
                PevBmsChip(id: .topologyStatus, title: pevLocalizedText("bms.chip.topology_unverified"), accent: .green),
            ]
        case .noData:
            return [
                PevBmsChip(id: .captureStatus, title: snapshot.captureActionState ?? pevLocalizedText("bms.chip.limited_data"), accent: .yellow),
            ]
        }
    }

    func liveModes(snapshot: BmsSnapshot) -> [PevBmsMode] {
        switch self {
        case .cellMapInline:
            snapshot.inlineCellMapModes
        case .cellMapScrollable:
            snapshot.scrollableCellMapModes
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
        pevLocalizedText(isProbeRecommended ? "picker.action.start_probe" : "picker.action.start_capture")
    }

    var useActionAccessibilityLabel: String {
        pevLocalizedText("picker.action.use_device", accessibilityDeviceName)
    }

    var captureActionAccessibilityLabel: String {
        pevLocalizedText("picker.action.capture_for_device", captureActionTitle, accessibilityDeviceName)
    }

    private var accessibilityDeviceName: String {
        guard id != title else { return title }
        return pevLocalizedText("picker.device.with_identifier", title, String(id.suffix(4).uppercased()))
    }
}

public extension DevicePickerRowState {
    init(action: DiscoveryCandidateAction) {
        switch action {
        case .use:
            self = .supported(action: pevLocalizedText("picker.row.action.use"))
        case .probe:
            self = .probeRecommended(action: pevLocalizedText("picker.row.action.probe"))
        case .record:
            self = .unsupported(action: pevLocalizedText("picker.row.action.record"))
        case .confirm:
            self = .unsupported(action: pevLocalizedText("picker.row.action.confirm"))
        case .review:
            self = .unsupported(action: pevLocalizedText("picker.row.action.review"))
        case .later:
            self = .manual(action: pevLocalizedText("picker.row.action.later"))
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
            DevicePickerRowState(action: .use)
        case .probeRecommended:
            DevicePickerRowState(action: .probe)
        case .unknownRecordable:
            DevicePickerRowState(action: .record)
        case .knownUnsupported:
            DevicePickerRowState(action: .record)
        case .ambiguous:
            DevicePickerRowState(action: .confirm)
        case .conflicting:
            DevicePickerRowState(action: .review)
        case .rejectedNoise:
            DevicePickerRowState(action: .record)
        case .manualEntry:
            DevicePickerRowState(action: .later)
        case .unsupported:
            DevicePickerRowState(action: .record)
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
            pevLocalizedText("picker.status.scanning")
        case .idle where rows.isEmpty:
            pevLocalizedText("picker.status.no_devices")
        case .idle:
            pevLocalizedText("picker.status.scan_complete")
        case .bluetoothUnavailable:
            pevLocalizedText("picker.status.bluetooth_unavailable")
        case .permissionDenied:
            pevLocalizedText("picker.status.permission_denied")
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

    public func presentedBmsScreen(liveBmsSnapshot: BmsSnapshot?) -> PevScreen {
        resolvedBmsScreen(liveBmsSnapshot: liveBmsSnapshot, preferredScreenID: nil)
    }

    public func presentedScreen(
        for screen: PevScreen,
        liveBmsSnapshot: BmsSnapshot?
    ) -> PevScreen {
        guard screen.id.isBmsScreen else { return screen }
        return resolvedBmsScreen(liveBmsSnapshot: liveBmsSnapshot, preferredScreenID: screen.id)
    }

    private func resolvedBmsScreen(
        liveBmsSnapshot: BmsSnapshot?,
        preferredScreenID: PevScreenID?
    ) -> PevScreen {
        guard let liveBmsSnapshot else { return Self.noLiveBmsReadbackScreen() }

        let resolvedKind = PevBmsScreenKind(
            liveSnapshot: liveBmsSnapshot,
            preferredScreenID: preferredScreenID
        )
        let bmsScreenID = preferredScreenID ?? resolvedKind.presentationScreenID

        let metadata = self.screen(id: bmsScreenID) ?? Self.noLiveBmsReadbackScreen()
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
                layoutLabel: pevLocalizedText("bms.layout.no_live_readback"),
                seriesGroupCount: nil,
                parallelCount: nil,
                packCount: 0,
                bmsCount: 0,
                confidence: .unverified
            )
        )

        return PevScreen(
            id: .bmsNoData,
            title: pevLocalizedText("bms.title.battery"),
            subtitle: pevLocalizedText("bms.layout.no_live_readback"),
            secondaryValue: pevLocalizedText("bms.secondary.no_live_bms"),
            bmsContent: PevBmsContent(
                kind: .noData,
                snapshot: snapshot,
                chips: [
                    PevBmsChip(id: .availability, title: pevLocalizedText("bms.chip.no_live_bms"), accent: .yellow),
                ]
            ),
        )
    }

    public static let live = PevScreenCatalog(screens: [
        liveScreen(id: .eucRide, title: pevLocalizedText("dashboard.title.euc_ride"), subtitle: pevLocalizedText("dashboard.subtitle.live_telemetry")),
        liveScreen(id: .bmsOverview, title: pevLocalizedText("bms.title.battery"), subtitle: pevLocalizedText("bms.subtitle.live_readback")),
        liveScreen(id: .bmsCellMap6S, title: pevLocalizedText("bms.title.cell_map"), subtitle: pevLocalizedText("bms.subtitle.live_readback")),
        liveScreen(id: .bmsCellMap40S, title: pevLocalizedText("bms.title.cell_map"), subtitle: pevLocalizedText("bms.subtitle.live_readback")),
        liveScreen(id: .bmsCellDetail, title: pevLocalizedText("bms.title.cell_detail"), subtitle: pevLocalizedText("bms.subtitle.live_readback")),
        liveScreen(id: .bmsUnknownTopology, title: pevLocalizedText("bms.title.battery"), subtitle: pevLocalizedText("bms.subtitle.topology_unavailable")),
        liveScreen(id: .bmsNoData, title: pevLocalizedText("bms.title.battery"), subtitle: pevLocalizedText("bms.layout.no_live_readback")),
        liveScreen(id: .vescRide, title: pevLocalizedText("dashboard.title.vesc_ride"), subtitle: pevLocalizedText("dashboard.subtitle.live_telemetry")),
        liveScreen(id: .vescDebug, title: pevLocalizedText("dashboard.title.vesc_state"), subtitle: pevLocalizedText("dashboard.subtitle.live_telemetry"))
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
        case .eucRide, .vescRide, .vescDebug:
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
