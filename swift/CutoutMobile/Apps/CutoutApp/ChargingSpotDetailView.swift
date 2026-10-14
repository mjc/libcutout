import CutoutMobile
import CutoutMobileFFI
import Foundation
import SwiftUI

struct ChargingSpotDetailView: View {
    let spot: MobileChargingSpotDto
    var isDeleting = false
    var errorKey: String? = nil
    var onEdit: ((MobileChargingSpotDto) -> Void)? = nil
    var onDelete: ((MobileChargingSpotDto) -> Void)? = nil
    @State private var confirmsDeletion = false
    @Environment(\.dismiss) private var dismiss

    private var input: MobileChargingInputDto? { spot.input }
    private var canEdit: Bool { spot.owned && (spot.status == .queued || spot.status == .published) }

    var body: some View {
        NavigationStack {
            List {
                Section(localizedAppText("charging.details.section")) {
                    detail("charging.details.name", input?.name ?? ChargingSpotPresentation.unknownName)
                    detail("charging.details.access", input?.accessNote)
                    detail(
                        "charging.details.cost",
                        input.map { localizedAppText($0.cost == .free ? "charging.cost.free" : "charging.cost.paid") })
                    detail("charging.details.venue", input?.venue.map(ChargingSpotPresentation.venueName))
                    detail("charging.details.hours", input?.hours)
                    detail("charging.details.terms", input?.terms.map(ChargingSpotPresentation.termsName))
                    detail("charging.details.price", input?.price.map(ChargingSpotPresentation.priceText))
                }
                Section(localizedAppText("charging.details.supplies")) {
                    if let supplies = input?.supplies, !supplies.isEmpty {
                        Text(supplies.map(ChargingSpotPresentation.supplyText).joined(separator: "\n"))
                    } else {
                        Text(localizedAppText("charging.spot.power.unknown"))
                    }
                }
                if canEdit, onEdit != nil || onDelete != nil {
                    Section {
                        if let onEdit {
                            Button(localizedAppText("charging.editor.edit")) { onEdit(spot) }
                                .accessibilityIdentifier("charging.details.edit")
                        }
                        if onDelete != nil {
                            Button(localizedAppText("charging.editor.delete"), role: .destructive) {
                                confirmsDeletion = true
                            }
                            .accessibilityIdentifier("charging.details.delete")
                        }
                        if let errorKey {
                            Text(localizedAppText(errorKey)).foregroundStyle(.red)
                        }
                        if isDeleting { ProgressView() }
                    }
                    .disabled(isDeleting)
                }
                Section(localizedAppText("charging.details.community")) {
                    detail(
                        "charging.details.status",
                        localizedAppText(ChargingSpotPresentation.statusKey(spot.status)))
                    detail(
                        "charging.details.votes",
                        spot.remoteId == nil
                            ? localizedAppText("charging.spot.votes.community")
                            : "\(spot.votesUp) ✓ · \(spot.votesDown) ×")
                    if spot.pendingOperations > 0 {
                        Text(localizedAppText("charging.spot.status.pending"))
                    }
                    detail(
                        "charging.details.verified",
                        spot.verifiedAtMs.map {
                            Date(timeIntervalSince1970: Double($0) / 1_000).formatted(
                                date: .abbreviated, time: .shortened)
                        } ?? localizedAppText("charging.details.not_verified"))
                }
            }
            .navigationTitle(input?.name ?? ChargingSpotPresentation.unknownName)
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(localizedAppText("ride.readings.done")) { dismiss() }
                        .accessibilityIdentifier("charging.details.done")
                }
            }
        }
        .confirmationDialog(localizedAppText("charging.editor.delete.confirm"), isPresented: $confirmsDeletion) {
            Button(localizedAppText("charging.editor.delete"), role: .destructive) { onDelete?(spot) }
        }
        .interactiveDismissDisabled(isDeleting)
        .accessibilityIdentifier("charging.spot.details-screen")
    }

    @ViewBuilder private func detail(_ key: String, _ value: String?) -> some View {
        if let value, !value.isEmpty {
            if key == "charging.details.access" {
                VStack(alignment: .leading, spacing: 4) {
                    Text(localizedAppText(key)).font(.caption).foregroundStyle(.secondary)
                    Text(value).fixedSize(horizontal: false, vertical: true)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier(key)
            } else {
                LabeledContent(localizedAppText(key), value: value)
            }
        }
    }
}

extension ChargingSpotPresentation {
    static let connectorOptions: [(key: String, value: MobileChargingConnectorDto)] = [
        ("charging.connector.type_a", .typeA), ("charging.connector.type_b", .typeB),
        ("charging.connector.type_c", .typeC), ("charging.connector.type_d", .typeD),
        ("charging.connector.type_e", .typeE), ("charging.connector.type_f", .typeF),
        ("charging.connector.type_g", .typeG), ("charging.connector.type_h", .typeH),
        ("charging.connector.type_i", .typeI), ("charging.connector.type_j", .typeJ),
        ("charging.connector.type_k", .typeK), ("charging.connector.type_l", .typeL),
        ("charging.connector.type_m", .typeM), ("charging.connector.type_n", .typeN),
        ("charging.connector.type_o", .typeO), ("charging.connector.nema_5_15", .nema515),
        ("charging.connector.nema_5_20", .nema520), ("charging.connector.nema_14_50", .nema1450),
        ("charging.connector.j1772", .j1772), ("charging.connector.type2", .type2),
        ("charging.connector.ccs1", .ccs1), ("charging.connector.ccs2", .ccs2),
        ("charging.connector.chademo", .chademo), ("charging.connector.nacs", .nacs),
        ("charging.connector.unknown", .unknown),
    ]

    static func loadErrorKey(_ error: MobileChargingError) -> String {
        switch error {
        case .Unavailable: "charging.status.unavailable"
        case .StorageFailure: "charging.status.storage_failed"
        case .Unauthorized: "charging.sync.unauthorized"
        case .Network: "charging.sync.offline"
        case .Rejected: "charging.sync.rejected"
        case .Throttled: "charging.sync.throttled"
        case .Conflict, .NotOwned, .NotFound: "charging.status.refused"
        case .OutboxFull: "charging.status.outbox_full"
        default: "charging.status.failed"
        }
    }

    static func voteErrorKey(_ error: MobileChargingError) -> String {
        switch error {
        case .Unavailable: "charging.vote.unavailable"
        case .StorageFailure: "charging.vote.storage_failed"
        case .Conflict, .NotOwned, .NotFound: "charging.vote.refused"
        case .OutboxFull: "charging.vote.outbox_full"
        default: "charging.vote.failed"
        }
    }

    static func connectorName(_ connector: MobileChargingConnectorDto) -> String {
        if case .other(let label) = connector { return label }
        return localizedAppText(
            connectorOptions.first(where: { $0.value == connector })?.key ?? "charging.connector.unknown")
    }

    static func supplyText(_ supply: MobileChargingSupplyDto) -> String {
        let voltage = supply.voltageVolts.map { "\($0) V" } ?? localizedAppText("charging.spot.power.unknown")
        let connectors = supply.connectors.map(connectorName).joined(separator: ", ")
        return connectors.isEmpty ? voltage : "\(voltage) · \(connectors)"
    }

    static func venueName(_ venue: MobileChargingVenueDto) -> String {
        let key: String
        switch venue {
        case .restaurant: key = "charging.venue.restaurant"
        case .cafe: key = "charging.venue.cafe"
        case .bar: key = "charging.venue.bar"
        case .evChargingStation: key = "charging.venue.ev"
        case .other: key = "charging.venue.other"
        }
        return localizedAppText(key)
    }

    static func termsName(_ terms: MobileChargingTermsDto) -> String {
        switch terms {
        case .directFee: localizedAppText("charging.terms.direct_fee")
        case .purchaseRequired: localizedAppText("charging.terms.purchase")
        case .both: localizedAppText("charging.terms.both")
        }
    }

    static func priceText(_ price: MobileChargingPriceDto) -> String {
        let exponent = Int16(price.minorUnitExponent)
        let amount = NSDecimalNumber(mantissa: price.amountMinor, exponent: -exponent, isNegative: false)
        let formatter = NumberFormatter()
        formatter.numberStyle = .currency
        formatter.currencyCode = price.currency
        formatter.minimumFractionDigits = Int(price.minorUnitExponent)
        formatter.maximumFractionDigits = Int(price.minorUnitExponent)
        let amountText = formatter.string(from: amount) ?? "\(amount) \(price.currency)"
        let basis: String
        switch price.basis {
        case .perCharge: basis = localizedAppText("charging.price.per_charge")
        case .perHour: basis = localizedAppText("charging.price.per_hour")
        case .perKwh: basis = localizedAppText("charging.price.per_kwh")
        case .purchase: basis = localizedAppText("charging.price.purchase")
        }
        return "\(amountText) · \(basis)"
    }
}
