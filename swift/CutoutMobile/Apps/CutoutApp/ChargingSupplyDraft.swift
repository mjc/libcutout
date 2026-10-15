import CutoutMobileFFI
import Foundation
import Observation

@MainActor
@Observable
final class ChargingOtherConnectorDraft: Identifiable {
    let id = UUID()
    var label: String { didSet { changed() } }
    var onChange: (@MainActor () -> Void)?

    init(label: String) { self.label = label }
    private func changed() { onChange?() }
}

@MainActor
@Observable
final class ChargingSupplyDraft: Identifiable {
    let id = UUID()
    var voltageText: String { didSet { syncManualEdit() } }
    private(set) var connectors: [MobileChargingConnectorDto]
    private(set) var otherConnectors: [ChargingOtherConnectorDraft]
    private var powerDraft: MobileChargingPowerDraft
    private var isApplyingSnapshot = false
    var onChange: (@MainActor () -> Void)?

    /// Starts an empty manual row. Incomplete text remains local until it forms a valid supply.
    init() {
        self.voltageText = ""
        self.connectors = []
        self.otherConnectors = []
        self.powerDraft = mobileChargingPowerDraft(countryCode: nil, evStation: false).edit(supply: nil)
    }

    /// Starts a country or venue suggestion. Rust owns its provenance and confirmation state.
    init(countryCode: String?, evStation: Bool) {
        self.voltageText = ""
        self.connectors = []
        self.otherConnectors = []
        self.powerDraft = mobileChargingPowerDraft(countryCode: countryCode, evStation: evStation)
        loadSelection(powerDraft.snapshot().selection)
    }

    /// Restores an observed supply as confirmed manual power, preserving its connector order.
    init(supply: MobileChargingSupplyDto, countryCode: String?, evStation: Bool) {
        self.voltageText = ""
        self.connectors = []
        self.otherConnectors = []
        self.powerDraft = mobileChargingPowerDraft(countryCode: countryCode, evStation: evStation)
            .edit(supply: supply)
        let snapshot = powerDraft.snapshot()
        loadSelection(snapshot.includedSupply == nil ? supply : snapshot.selection)
    }

    var snapshot: MobileChargingPowerDraftSnapshot { powerDraft.snapshot() }
    var isConfirmed: Bool { snapshot.includedSupply != nil }
    var hasIncompleteEdit: Bool { !isEmpty && snapshot.origin == .manual && !isConfirmed }
    var requiresConfirmation: Bool {
        snapshot.origin == .suggested && !isConfirmed && !hasIncompleteEdit && !hasInvalidVoltage && !isEmpty
    }

    var isEmpty: Bool {
        voltageText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && connectors.isEmpty
            && otherConnectors.allSatisfy { $0.label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }

    var connectorSummary: [MobileChargingConnectorDto] {
        if isEmpty { return [] }
        if !hasIncompleteEdit { return snapshot.selection.connectors }
        if connectors.contains(.unknown) { return [.unknown] }
        var selected = connectors
        for connector in otherConnectors {
            let label = connector.label.trimmingCharacters(in: .whitespacesAndNewlines)
            if !label.isEmpty, !selected.contains(.other(label: label)) { selected.append(.other(label: label)) }
        }
        return selected
    }

    var hasInvalidVoltage: Bool {
        let value = voltageText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { return false }
        guard let volts = UInt16(value) else { return true }
        return !mobileChargingPowerVoltageIsValid(voltageVolts: volts)
    }

    func setCountryCode(_ countryCode: String?) {
        self.powerDraft = powerDraft.withCountry(countryCode: countryCode)
        if !hasIncompleteEdit && !isEmpty { loadSelection(self.powerDraft.snapshot().selection) }
        onChange?()
    }

    func setVenue(isEV: Bool) {
        self.powerDraft = powerDraft.withKind(evStation: isEV)
        if !hasIncompleteEdit && !isEmpty { loadSelection(self.powerDraft.snapshot().selection) }
        onChange?()
    }

    func confirm() {
        guard !isEmpty, !hasInvalidVoltage, !hasIncompleteEdit else { return }
        self.powerDraft = powerDraft.confirm()
        onChange?()
    }

    private func loadSelection(_ supply: MobileChargingSupplyDto) {
        isApplyingSnapshot = true
        defer { isApplyingSnapshot = false }
        voltageText = supply.voltageVolts.map(String.init) ?? ""
        connectors = supply.connectors.filter { if case .other = $0 { false } else { true } }
        otherConnectors = supply.connectors.compactMap {
            if case .other(let label) = $0 { ChargingOtherConnectorDraft(label: label) } else { nil }
        }
        for connector in otherConnectors { connector.onChange = { [weak self] in self?.syncManualEdit() } }
    }

    private func currentSupply() -> MobileChargingSupplyDto? {
        let trimmedVoltage = voltageText.trimmingCharacters(in: .whitespacesAndNewlines)
        let voltage = trimmedVoltage.isEmpty ? nil : UInt16(trimmedVoltage)
        guard trimmedVoltage.isEmpty || voltage != nil,
            voltage.map({ mobileChargingPowerVoltageIsValid(voltageVolts: $0) }) ?? true
        else { return nil }
        if connectors.contains(.unknown) {
            return MobileChargingSupplyDto(voltageVolts: voltage, connectors: [.unknown])
        }
        var selected = connectors
        for connector in otherConnectors {
            let label = connector.label.trimmingCharacters(in: .whitespacesAndNewlines)
            if !label.isEmpty, !selected.contains(.other(label: label)) { selected.append(.other(label: label)) }
        }
        guard voltage != nil || !selected.isEmpty else { return nil }
        return MobileChargingSupplyDto(voltageVolts: voltage, connectors: selected)
    }

    private func syncManualEdit() {
        guard !isApplyingSnapshot else { return }
        defer { onChange?() }
        powerDraft = powerDraft.edit(supply: currentSupply())
    }

    func set(_ connector: MobileChargingConnectorDto, selected: Bool) {
        if connector == .unknown {
            connectors = selected ? [.unknown] : []
            syncManualEdit()
            return
        }
        connectors.removeAll { $0 == .unknown }
        if selected, !connectors.contains(connector) { connectors.append(connector) }
        if !selected { connectors.removeAll { $0 == connector } }
        syncManualEdit()
    }

    func addOtherConnector() {
        connectors.removeAll { $0 == .unknown }
        let draft = ChargingOtherConnectorDraft(label: "")
        draft.onChange = { [weak self] in self?.syncManualEdit() }
        otherConnectors.append(draft)
        syncManualEdit()
    }

    func removeOtherConnector(id: UUID) {
        otherConnectors.removeAll { $0.id == id }
        syncManualEdit()
    }

    func value() -> MobileChargingSupplyDto? {
        guard !isEmpty, !hasInvalidVoltage, !hasIncompleteEdit else { return nil }
        return powerDraft.snapshot().includedSupply
    }

}

enum ChargingPriceDraft {
    static func makePrice(
        amount: String, currency: String, basis: MobileChargingPriceBasisDto?, locale: Locale = .current
    )
        -> MobileChargingPriceDto?
    {
        let amount = amount.trimmingCharacters(in: .whitespacesAndNewlines)
        let currency = currency.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        guard !amount.isEmpty, currency.count == 3,
            currency.unicodeScalars.allSatisfy({ (65...90).contains(Int($0.value)) }),
            Locale.Currency.isoCurrencies.contains(where: { $0.identifier == currency }),
            let basis
        else { return nil }
        let formatter = NumberFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.numberStyle = .currency
        formatter.currencyCode = currency
        let exponent = formatter.maximumFractionDigits
        guard (0...4).contains(exponent) else { return nil }
        let separator = locale.decimalSeparator ?? "."
        guard !separator.isEmpty else { return nil }
        let parts = amount.components(separatedBy: separator)
        guard parts.count <= 2, !parts[0].isEmpty,
            let whole = decimalDigits(parts[0])
        else { return nil }
        let fraction: String
        if parts.count == 2 {
            guard !parts[1].isEmpty, let parsedFraction = decimalDigits(parts[1]) else { return nil }
            fraction = parsedFraction
        } else {
            fraction = ""
        }
        guard fraction.count <= exponent || fraction.dropFirst(exponent).allSatisfy({ $0 == "0" }) else { return nil }
        let minorDigits = String(fraction.prefix(exponent)).padding(toLength: exponent, withPad: "0", startingAt: 0)
        guard let minor = UInt64(whole + minorDigits), minor > 0,
            minor <= 9_000_000_000_000_000
        else { return nil }
        return MobileChargingPriceDto(
            amountMinor: minor, currency: currency, minorUnitExponent: UInt8(exponent), basis: basis
        )
    }

    private static func decimalDigits(_ value: String) -> String? {
        var digits = ""
        for character in value {
            guard character.unicodeScalars.count == 1,
                character.unicodeScalars.first?.properties.numericType == .decimal,
                let digit = character.wholeNumberValue, (0...9).contains(digit)
            else { return nil }
            digits.append(String(digit))
        }
        return digits
    }
}
