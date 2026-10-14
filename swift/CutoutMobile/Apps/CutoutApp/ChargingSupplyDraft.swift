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
    enum Origin: Equatable { case manual, countrySuggestion }

    let id = UUID()
    private(set) var origin: Origin
    var voltageText: String { didSet { markModified() } }
    private(set) var connectors: [MobileChargingConnectorDto]
    private(set) var otherConnectors: [ChargingOtherConnectorDraft]
    private var originalSupply: MobileChargingSupplyDto?
    private var isModified = false
    var onChange: (@MainActor () -> Void)?

    init(
        supply: MobileChargingSupplyDto = MobileChargingSupplyDto(voltageVolts: nil, connectors: []),
        origin: Origin = .manual
    ) {
        self.origin = origin
        self.voltageText = supply.voltageVolts.map(String.init) ?? ""
        self.connectors = supply.connectors.filter { if case .other = $0 { false } else { true } }
        self.originalSupply = supply.voltageVolts == nil && supply.connectors.isEmpty ? nil : supply
        self.otherConnectors = supply.connectors.compactMap {
            if case .other(let label) = $0 { ChargingOtherConnectorDraft(label: label) } else { nil }
        }
        for connector in otherConnectors { connector.onChange = { [weak self] in self?.markModified() } }
    }

    func set(_ connector: MobileChargingConnectorDto, selected: Bool) {
        if connector == .unknown {
            connectors = selected ? [.unknown] : []
            markModified()
            return
        }
        connectors.removeAll { $0 == .unknown }
        if selected, !connectors.contains(connector) { connectors.append(connector) }
        if !selected { connectors.removeAll { $0 == connector } }
        markModified()
    }

    func addOtherConnector() {
        connectors.removeAll { $0 == .unknown }
        let draft = ChargingOtherConnectorDraft(label: "")
        draft.onChange = { [weak self] in self?.markModified() }
        otherConnectors.append(draft)
        markModified()
    }

    func removeOtherConnector(id: UUID) {
        otherConnectors.removeAll { $0.id == id }
        markModified()
    }

    func updateFromCountrySuggestion(_ supply: MobileChargingSupplyDto) {
        guard origin == .countrySuggestion else { return }
        voltageText = supply.voltageVolts.map(String.init) ?? ""
        connectors = supply.connectors.filter { if case .other = $0 { false } else { true } }
        otherConnectors.removeAll()
        for case .other(let label) in supply.connectors {
            let item = ChargingOtherConnectorDraft(label: label)
            item.onChange = { [weak self] in self?.markModified() }
            otherConnectors.append(item)
        }
        origin = .countrySuggestion
        originalSupply = supply
        isModified = false
    }

    var isEmpty: Bool {
        voltageText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && connectors.isEmpty
            && otherConnectors.allSatisfy { $0.label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }

    var hasInvalidVoltage: Bool {
        let value = voltageText.trimmingCharacters(in: .whitespacesAndNewlines)
        return !value.isEmpty && (!(UInt16(value).map { (1...2_000).contains($0) } ?? false))
    }

    var connectorSummary: [MobileChargingConnectorDto] {
        if !isModified, let originalSupply {
            return originalSupply.connectors.contains(.unknown) ? [.unknown] : originalSupply.connectors
        }
        if connectors.contains(.unknown) { return [.unknown] }
        var selected = connectors
        for connector in otherConnectors {
            let label = connector.label.trimmingCharacters(in: .whitespacesAndNewlines)
            if !label.isEmpty, !selected.contains(.other(label: label)) { selected.append(.other(label: label)) }
        }
        return selected
    }

    func value() -> MobileChargingSupplyDto? {
        if !isModified, let originalSupply {
            if connectors.contains(.unknown) {
                return MobileChargingSupplyDto(voltageVolts: originalSupply.voltageVolts, connectors: [.unknown])
            }
            return originalSupply
        }
        let voltage: UInt16?
        if voltageText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            voltage = nil
        } else {
            guard let parsed = UInt16(voltageText), (1...2_000).contains(parsed) else { return nil }
            voltage = parsed
        }
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

    private func markModified() {
        isModified = true
        origin = .manual
        onChange?()
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
