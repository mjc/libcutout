import CutoutMobileFFI
import SwiftUI

struct ChargingSupplyForm: View {
    @Bindable var draft: ChargingSupplyDraft
    let outletChoices: [MobileChargingConnectorDto]
    @FocusState.Binding var focusedField: ChargingSpotEditorField?
    let onRemove: () -> Void

    private var choices: [MobileChargingConnectorDto] {
        var values = outletChoices + ChargingSpotPresentation.connectorOptions.map(\.value) + draft.connectors
        if !values.contains(.unknown) { values.append(.unknown) }
        return values.filter { if case .other = $0 { false } else { true } }.uniqued()
    }

    var body: some View {
        Group {
            Text(localizedAppText("charging.editor.supply.section"))
                .font(.headline)
            LabeledContent(localizedAppText("charging.editor.voltage")) {
                TextField("", text: $draft.voltageText)
                    #if os(iOS)
                        .keyboardType(.numberPad)
                    #endif
                    .focused($focusedField, equals: .supplyVoltage(draft.id))
                    .multilineTextAlignment(.trailing)
                    .accessibilityLabel(localizedAppText("charging.editor.voltage"))
                    .accessibilityIdentifier("charging.editor.supply.voltage")
            }
            if draft.hasInvalidVoltage {
                Text(localizedAppText("charging.editor.error.voltage"))
                    .font(.footnote)
                    .foregroundStyle(.red)
                    .accessibilityIdentifier("charging.editor.supply.voltage-error")
            }
            Text(localizedAppText("charging.editor.supply.unknown_hint"))
                .font(.footnote)
                .foregroundStyle(.secondary)
            Menu {
                ForEach(choices, id: \.self) { connector in
                    Toggle(
                        isOn: Binding(
                            get: { draft.connectors.contains(connector) },
                            set: { draft.set(connector, selected: $0) }
                        )
                    ) {
                        Text(
                            connector == .unknown
                                ? localizedAppText("charging.connector.unknown")
                                : ChargingSpotPresentation.connectorName(connector))
                    }
                    .accessibilityIdentifier("charging.editor.supply.connector.\(connectorKey(connector))")
                }
            } label: {
                LabeledContent(localizedAppText("charging.editor.plugs"), value: selectedPlugs)
            }
            .accessibilityIdentifier("charging.editor.supply.plugs")
            ForEach(draft.otherConnectors) { connector in
                @Bindable var connector = connector
                LabeledContent(localizedAppText("charging.editor.connector.other")) {
                    HStack {
                        TextField("", text: $connector.label)
                            .focused($focusedField, equals: .supplyConnector(connector.id))
                            .accessibilityLabel(localizedAppText("charging.editor.connector.other"))
                            .disabled(draft.connectors.contains(.unknown))
                            .accessibilityIdentifier("charging.editor.supply.custom-connector")
                        Button(role: .destructive) {
                            draft.removeOtherConnector(id: connector.id)
                        } label: {
                            Image(systemName: "minus.circle")
                        }
                        .accessibilityLabel(localizedAppText("charging.editor.connector.remove"))
                    }
                }
            }
            Button(localizedAppText("charging.editor.connector.add_other")) { draft.addOtherConnector() }
                .accessibilityIdentifier("charging.editor.supply.add-other")
            Button(localizedAppText("charging.editor.supply.remove"), role: .destructive, action: onRemove)
            Divider()
        }
    }

    private var selectedPlugs: String {
        let labels = draft.connectorSummary.map { connector in
            connector == .unknown
                ? localizedAppText("charging.connector.unknown")
                : ChargingSpotPresentation.connectorName(connector)
        }
        return labels.isEmpty ? localizedAppText("charging.editor.plugs.none") : labels.joined(separator: ", ")
    }

    private func connectorKey(_ value: MobileChargingConnectorDto) -> String {
        ChargingSpotPresentation.connectorName(value).lowercased().replacingOccurrences(of: " ", with: "-")
    }
}

extension Array where Element: Hashable {
    fileprivate func uniqued() -> [Element] {
        var seen = Set<Element>()
        return filter { seen.insert($0).inserted }
    }
}
