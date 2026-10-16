import CutoutMobile
import CutoutMobileFFI
import Foundation
import SwiftUI

struct ChargingSpotDetailView: View {
    let spot: MobileChargingSpotDto
    var isDeleting = false
    var errorKey: String? = nil
    let onEdit: (MobileChargingSpotDto) -> Void
    let onDelete: (MobileChargingSpotDto) -> Void
    @State private var confirmsDeletion = false
    @State private var showsContentReport = false
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private var input: MobileChargingInputDto? { spot.input }
    private var canEdit: Bool { spot.owned && (spot.status == .queued || spot.status == .published) }
    private var canReport: Bool { spot.remoteId != nil && spot.status == .published }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    if let input {
                        Text(input.accessNote)
                            .font(.body)
                            .lineSpacing(4)
                            .fixedSize(horizontal: false, vertical: true)
                            .accessibilityLabel(localizedAppText("charging.editor.access_note"))
                            .accessibilityValue(input.accessNote)
                            .accessibilityIdentifier("charging.details.access")

                        powerSummary(input)

                        VStack(alignment: .leading, spacing: 16) {
                            if let venue = input.venue, venue != .other {
                                Label(
                                    ChargingSpotPresentation.venueName(venue),
                                    systemImage: "mappin.and.ellipse"
                                )
                                .font(.body.weight(.medium))
                                .foregroundStyle(PevColors.primaryText)
                                .fixedSize(horizontal: false, vertical: true)
                            }
                            detail("charging.details.hours", input.hours, symbol: "clock")
                            detail("charging.details.terms", input.terms.map(ChargingSpotPresentation.termsName))
                            detail("charging.details.price", input.price.map(ChargingSpotPresentation.priceText))
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }

                    actionArea
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 20)
                .padding(.top, 12)
                .padding(.bottom, 24)
            }
            .scrollIndicators(.hidden)
            .background(PevColors.pageBackground)
            .navigationTitle(input?.name ?? ChargingSpotPresentation.unknownName)
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
                .presentationDetents(dynamicTypeSize.isAccessibilitySize ? [.large] : [.medium, .large])
                .presentationDragIndicator(.visible)
            #endif
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(localizedAppText("ride.readings.done")) { dismiss() }
                        .disabled(isDeleting)
                        .accessibilityIdentifier("charging.details.done")
                }
            }
        }
        .confirmationDialog(localizedAppText("charging.editor.delete.confirm"), isPresented: $confirmsDeletion) {
            Button(localizedAppText("charging.editor.delete"), role: .destructive) { onDelete(spot) }
        }
        .interactiveDismissDisabled(isDeleting)
        .sheet(isPresented: $showsContentReport) {
            if let remoteID = spot.remoteId { ChargingContentReportView(remoteID: remoteID) }
        }
        .accessibilityIdentifier("charging.spot.details-screen")
    }

    private func powerSummary(_ input: MobileChargingInputDto) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(localizedAppText(input.cost == .free ? "charging.cost.free" : "charging.cost.paid"))
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(PevColors.primaryText)
                .padding(.horizontal, 11)
                .padding(.vertical, 6)
                .background(
                    (input.cost == .free ? PevColors.green : PevColors.yellow).opacity(0.16),
                    in: Capsule()
                )

            ForEach(input.supplies.indices, id: \.self) { index in
                let supply = input.supplies[index]
                HStack(alignment: .center, spacing: 12) {
                    if let connector = firstRealConnector(in: supply) {
                        ChargingConnectorIcon(connector: connector, size: 24)
                            .foregroundStyle(PevColors.brand)
                            .frame(width: 44, height: 44)
                            .background(PevColors.yellow.opacity(0.14), in: RoundedRectangle(cornerRadius: 12))
                    }
                    Text(ChargingSpotPresentation.supplyText(supply))
                        .font(.headline)
                        .foregroundStyle(PevColors.primaryText)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(PevColors.cardFill, in: RoundedRectangle(cornerRadius: 18))
        .overlay {
            RoundedRectangle(cornerRadius: 18)
                .strokeBorder(PevColors.cardStroke.opacity(0.7), lineWidth: 1)
        }
    }

    private func firstRealConnector(in supply: MobileChargingSupplyDto) -> MobileChargingConnectorDto? {
        supply.connectors.first { connector in
            if case .unknown = connector { return false }
            return true
        }
    }

    @ViewBuilder private var actionArea: some View {
        if canEdit || canReport {
            VStack(alignment: .leading, spacing: 12) {
                if canEdit {
                    ViewThatFits(in: .horizontal) {
                        HStack(spacing: 12) {
                            editButton
                            deleteButton
                        }
                        VStack(alignment: .trailing, spacing: 10) {
                            editButton
                            deleteButton
                        }
                    }
                }
                if canReport {
                    Button {
                        showsContentReport = true
                    } label: {
                        Label(localizedAppText("charging.report.title"), systemImage: "flag")
                            .frame(maxWidth: .infinity, minHeight: 48)
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("charging.details.report")
                }
                if let errorKey {
                    Text(localizedAppText(errorKey))
                        .foregroundStyle(PevColors.red)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if isDeleting { ProgressView() }
            }
            .disabled(isDeleting)
        }
    }

    private var editButton: some View {
        Button {
            onEdit(spot)
        } label: {
            Label(localizedAppText("charging.editor.edit"), systemImage: "pencil")
                .font(.headline)
                .frame(maxWidth: .infinity, minHeight: 48)
        }
        .buttonStyle(.borderedProminent)
        .buttonBorderShape(.capsule)
        .tint(PevColors.yellow)
        .foregroundStyle(.black)
        .accessibilityIdentifier("charging.details.edit")
    }

    private var deleteButton: some View {
        Button(role: .destructive) {
            confirmsDeletion = true
        } label: {
            Image(systemName: "trash")
                .frame(minWidth: 48, minHeight: 48)
        }
        .buttonStyle(.bordered)
        .tint(PevColors.red)
        .accessibilityLabel(localizedAppText("charging.editor.delete"))
        .accessibilityIdentifier("charging.details.delete")
    }

    @ViewBuilder private func detail(_ key: String, _ value: String?, symbol: String? = nil) -> some View {
        if let value, !value.isEmpty {
            VStack(alignment: .leading, spacing: 5) {
                if let symbol {
                    Label(localizedAppText(key), systemImage: symbol)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(PevColors.muted)
                } else {
                    Text(localizedAppText(key))
                        .font(.caption.weight(.medium))
                        .foregroundStyle(PevColors.muted)
                }
                Text(value)
                    .font(.body)
                    .foregroundStyle(PevColors.primaryText)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

extension ChargingSpotPresentation {
    static let connectorOptions: [(key: String, value: MobileChargingConnectorDto)] = [
        ("charging.connector.nema_5_15", .nema515), ("charging.connector.type_a", .typeA),
        ("charging.connector.type_c", .typeC), ("charging.connector.type_d", .typeD),
        ("charging.connector.type_e", .typeE), ("charging.connector.type_f", .typeF),
        ("charging.connector.type_g", .typeG), ("charging.connector.type_h", .typeH),
        ("charging.connector.type_i", .typeI), ("charging.connector.type_j", .typeJ),
        ("charging.connector.type_k", .typeK), ("charging.connector.type_l", .typeL),
        ("charging.connector.type_m", .typeM), ("charging.connector.type_n", .typeN),
        ("charging.connector.type_o", .typeO),
        ("charging.connector.nema_5_20", .nema520), ("charging.connector.nema_14_50", .nema1450),
        ("charging.connector.j1772", .j1772), ("charging.connector.type2", .type2),
        ("charging.connector.ccs1", .ccs1), ("charging.connector.ccs2", .ccs2),
        ("charging.connector.chademo", .chademo), ("charging.connector.nacs", .nacs),
        ("charging.connector.nema_14_30", .nema1430), ("charging.connector.nema_10_30", .nema1030),
        ("charging.connector.nema_6_50", .nema650), ("charging.connector.nema_10_50", .nema1050),
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

    static func connectorIdentifier(_ connector: MobileChargingConnectorDto) -> String {
        switch connector {
        case .typeA: "type-a"
        case .typeC: "type-c"
        case .typeD: "type-d"
        case .typeE: "type-e"
        case .typeF: "type-f"
        case .typeG: "type-g"
        case .typeH: "type-h"
        case .typeI: "type-i"
        case .typeJ: "type-j"
        case .typeK: "type-k"
        case .typeL: "type-l"
        case .typeM: "type-m"
        case .typeN: "type-n"
        case .typeO: "type-o"
        case .nema515: "nema-5-15"
        case .nema520: "nema-5-20"
        case .nema1430: "nema-14-30"
        case .nema1450: "nema-14-50"
        case .nema1030: "nema-10-30"
        case .nema650: "nema-6-50"
        case .nema1050: "nema-10-50"
        case .j1772: "j1772"
        case .type2: "type-2"
        case .ccs1: "ccs-1"
        case .ccs2: "ccs-2"
        case .chademo: "chademo"
        case .nacs: "nacs"
        case .unknown: "unknown"
        case .other: "other"
        }
    }

    static func connectorAssetIdentifier(_ connector: MobileChargingConnectorDto) -> String {
        if case .other = connector { return "unknown" }
        return connectorIdentifier(connector)
    }

    static func supplyText(_ supply: MobileChargingSupplyDto) -> String {
        let voltage = supply.voltageVolts.map { "\($0) V" } ?? localizedAppText("charging.spot.power.unknown")
        let connectors = supply.connectors.map(connectorName).joined(separator: ", ")
        let power = connectors.isEmpty ? voltage : "\(voltage) · \(connectors)"
        guard let count = supply.outletCount else { return power }
        let countText = localizedAppText("charging.spot.outlets", count)
        return "\(countText) · \(power)"
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
