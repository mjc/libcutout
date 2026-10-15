import CutoutMobileFFI
import SwiftUI

struct ChargingConnectorIcon: View {
    let connector: MobileChargingConnectorDto
    var size: CGFloat = 32

    var body: some View {
        let asset = "ChargingConnector_\(ChargingSpotPresentation.connectorAssetIdentifier(connector))"
        Image(asset)
            .resizable()
            .renderingMode(.template)
            .scaledToFit()
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

struct ChargingConnectorPicker: View {
    let choices: [MobileChargingConnectorDto]
    let selection: MobileChargingConnectorDto?
    let onSelect: @MainActor (MobileChargingConnectorDto?) -> Void

    var body: some View {
        Menu {
            Button {
                onSelect(nil)
            } label: {
                Label {
                    Text(localizedAppText("charging.editor.plug.choose"))
                } icon: {
                    Image(systemName: "minus.circle")
                }
            }
            .accessibilityIdentifier("charging.editor.plug-choice.none")
            ForEach(choices, id: \.self) { connector in
                Button {
                    onSelect(connector)
                } label: {
                    Label {
                        Text(ChargingSpotPresentation.connectorName(connector))
                    } icon: {
                        ChargingConnectorIcon(connector: connector, size: 36)
                    }
                }
                .accessibilityIdentifier(
                    "charging.editor.plug-choice.\(ChargingSpotPresentation.connectorIdentifier(connector))")
            }
        } label: {
            HStack(spacing: 12) {
                Text(localizedAppText("charging.editor.plug_type"))
                Spacer(minLength: 8)
                if let selection {
                    let visibleSelection = ChargingSpotPresentation.canonicalConnector(selection)
                    ChargingConnectorIcon(connector: visibleSelection, size: 36)
                    Text(ChargingSpotPresentation.connectorName(visibleSelection))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                } else {
                    Text(localizedAppText("charging.editor.plug.choose"))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Image(systemName: "chevron.up.chevron.down")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .menuOrder(.fixed)
        .accessibilityIdentifier("charging.editor.plug-type")
    }

}

struct ChargingSupplyForm: View {
    @Bindable var draft: ChargingSupplyDraft
    let connectorChoices: [MobileChargingConnectorDto]
    @FocusState.Binding var focusedField: ChargingSpotEditorField?
    var validationIssue: MobileChargingFieldIssueDto? = nil
    let onRemove: () -> Void

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
            if let validationIssue, !draft.hasInvalidVoltage {
                Text(localizedAppText(ChargingEditorValidationPresentation.messageKey(for: validationIssue)))
                    .font(.footnote)
                    .foregroundStyle(.red)
                    .accessibilityIdentifier("charging.editor.supply.error")
            } else if draft.requiresConfirmation {
                Text(localizedAppText("charging.editor.error.supply_confirmation"))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            if draft.requiresConfirmation {
                Button(localizedAppText("charging.editor.supply.confirm"), action: draft.confirm)
                    .accessibilityIdentifier("charging.editor.supply.confirm")
            }
            Menu {
                ForEach(connectorChoices, id: \.self) { connector in
                    Toggle(
                        isOn: Binding(
                            get: {
                                draft.connectors.contains {
                                    ChargingSpotPresentation.connectorsMatch($0, connector)
                                }
                            },
                            set: { draft.set(connector, selected: $0) }
                        )
                    ) {
                        Label {
                            Text(
                                connector == .unknown
                                    ? localizedAppText("charging.connector.unknown")
                                    : ChargingSpotPresentation.connectorName(connector))
                        } icon: {
                            ChargingConnectorIcon(connector: connector, size: 32)
                        }
                    }
                    .accessibilityIdentifier(
                        "charging.editor.supply.connector.\(ChargingSpotPresentation.connectorIdentifier(connector))")
                }
            } label: {
                HStack(spacing: 8) {
                    Text(localizedAppText("charging.editor.plugs"))
                    Spacer(minLength: 8)
                    ForEach(draft.connectorSummary, id: \.self) {
                        ChargingConnectorIcon(connector: $0, size: 32)
                    }
                    Text(selectedPlugs)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                .contentShape(Rectangle())
            }
            .menuOrder(.fixed)
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

}

extension Array where Element: Hashable {
    fileprivate func uniqued() -> [Element] {
        var seen = Set<Element>()
        return filter { seen.insert($0).inserted }
    }
}
