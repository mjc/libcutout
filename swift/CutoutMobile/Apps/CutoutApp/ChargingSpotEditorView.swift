import CutoutMobileFFI
import SwiftUI

enum ChargingSpotEditorField: Hashable {
    case latitude, longitude, accessNote, name, hours, amount, currency
    case supplyVoltage(UUID)
    case supplyConnector(UUID)
}

struct ChargingSpotEditorView: View {
    private static let countryOptions: [(code: String, name: String)] = Locale.Region.isoRegions
        .map(\.identifier)
        .filter { $0.count == 2 }
        .map { (code: $0, name: Locale.current.localizedString(forRegionCode: $0) ?? $0) }
        .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }

    @Bindable var model: ChargingSpotEditorModel
    let onCancel: () -> Void
    let onSaved: (MobileChargingWriteReceiptDto) -> Void

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @FocusState private var focusedField: ChargingSpotEditorField?
    @State private var selectedDetent: PresentationDetent = .medium

    init(
        model: ChargingSpotEditorModel,
        onCancel: @escaping () -> Void,
        onSaved: @escaping (MobileChargingWriteReceiptDto) -> Void
    ) {
        self.model = model
        self.onCancel = onCancel
        self.onSaved = onSaved
        _selectedDetent = State(initialValue: model.isEditing || model.areOptionalDetailsExpanded ? .large : .medium)
    }

    private var isSaving: Bool { model.stage == .saving }
    private var isAccessNoteError: Bool {
        [
            "charging.editor.error.access_note_blank",
            "charging.editor.error.access_note_too_long",
            "charging.editor.error.access_note_control_character",
        ].contains(model.errorKey ?? "")
    }

    var body: some View {
        NavigationStack {
            Form {
                requiredDetails
                if model.hasInvalidOptionalDetails && !model.areOptionalDetailsExpanded {
                    Button {
                        model.areOptionalDetailsExpanded = true
                    } label: {
                        Label(
                            localizedAppText("charging.editor.error.optional_details"),
                            systemImage: "exclamationmark.circle.fill"
                        )
                        .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                        .foregroundStyle(.red)
                    }
                    .accessibilityIdentifier("charging.editor.optional-details-error")
                }
                Section {
                    Button(localizedAppText("charging.editor.move_pin")) {
                        focusedField = nil
                        model.movePin()
                    }
                    .accessibilityIdentifier("charging.editor.move-pin")
                }
                DisclosureGroup(isExpanded: $model.areOptionalDetailsExpanded) {
                    optionalDetails
                } label: {
                    Text(localizedAppText("charging.editor.add_details"))
                        .accessibilityIdentifier("charging.editor.add-details")
                }
            }
            #if os(iOS)
                .scrollDismissesKeyboard(.interactively)
            #endif
            .navigationTitle(localizedAppText(model.isEditing ? "charging.editor.edit_title" : "charging.editor.title"))
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(localizedAppText("charging.editor.cancel"), action: onCancel)
                        .disabled(isSaving)
                        .accessibilityIdentifier("charging.editor.cancel")
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(localizedAppText("charging.editor.save")) {
                        focusedField = nil
                        Task {
                            if let receipt = await model.save() { onSaved(receipt) }
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(model.canSave ? .accentColor : .gray)
                    .disabled(!model.canSave || isSaving)
                    .accessibilityIdentifier("charging.editor.save")
                }
            }
        }
        .accessibilityIdentifier("charging.editor.screen")
        #if os(iOS)
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if focusedField != nil {
                    HStack {
                        Spacer(minLength: 0)
                        Button {
                            focusedField = nil
                        } label: {
                            Text(localizedAppText("charging.editor.keyboard.done"))
                            .frame(minWidth: 44, minHeight: 44)
                            .contentShape(Rectangle())
                        }
                        .accessibilityIdentifier("charging.editor.keyboard.done")
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(.bar)
                }
            }
        #endif
        .interactiveDismissDisabled(isSaving)
        .disabled(isSaving)
        #if os(iOS)
            .presentationDetents(
                dynamicTypeSize.isAccessibilitySize ? [.large] : [.medium, .large],
                selection: Binding(
                    get: { dynamicTypeSize.isAccessibilitySize ? .large : selectedDetent },
                    set: { selectedDetent = $0 }
                )
            )
            .presentationDragIndicator(.visible)
            .onChange(of: model.areOptionalDetailsExpanded) { _, expanded in
                if dynamicTypeSize.isAccessibilitySize {
                    selectedDetent = .large
                } else {
                    selectedDetent = expanded ? .large : .medium
                }
            }
            .onChange(of: dynamicTypeSize) { _, size in
                if size.isAccessibilitySize { selectedDetent = .large }
            }
        #endif
    }

    private var requiredDetails: some View {
        Section {
            VStack(alignment: .leading, spacing: 8) {
                Text(localizedAppText("charging.editor.access_note"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                TextField("", text: $model.accessNote, axis: .vertical)
                    .lineLimit(3...6)
                    .frame(minHeight: 72, alignment: .topLeading)
                    .focused($focusedField, equals: .accessNote)
                    .accessibilityLabel(localizedAppText("charging.editor.access_note"))
                    .accessibilityIdentifier("charging.editor.access-note")
            }
            if dynamicTypeSize.isAccessibilitySize {
                VStack(spacing: 8) {
                    costChoice(.free)
                    costChoice(.paid)
                }
            } else {
                HStack {
                    costChoice(.free)
                    costChoice(.paid)
                }
            }
            if let errorKey = model.errorKey {
                Text(localizedAppText(errorKey))
                    .foregroundStyle(.red)
                    .accessibilityIdentifier(
                        isAccessNoteError ? "charging.editor.access-note-error" : "charging.editor.error")
            }
            if model.hasRevisionConflict {
                revisionConflictDetails
            }
            if model.hasInvalidCoordinate {
                Text(localizedAppText("charging.editor.error.coordinate"))
                    .font(.footnote)
                    .foregroundStyle(.red)
                    .accessibilityIdentifier("charging.editor.coordinate.error")
            }
        }
    }

    @ViewBuilder private var revisionConflictDetails: some View {
        if let latest = model.latestRevisionCandidate, let input = latest.input {
            VStack(alignment: .leading, spacing: 8) {
                ChargingInputSummary(input: input, title: "charging.editor.latest.title")
                Text(localizedAppText("charging.editor.keep_draft_warning"))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Button(localizedAppText("charging.editor.keep_draft")) {
                    model.keepDraftUsingLatestRevision()
                }
                .disabled(!model.canKeepDraftUsingLatestRevision)
                .accessibilityIdentifier("charging.editor.keep-draft-latest")
            }
        } else {
            if let errorKey = model.latestReloadErrorKey {
                Text(localizedAppText(errorKey)).foregroundStyle(.red)
            }
            HStack {
                Button(localizedAppText("charging.editor.review_latest")) {
                    Task { await model.loadLatestForReconciliation() }
                }
                .disabled(model.isLoadingLatest)
                if model.isLoadingLatest { ProgressView() }
            }
            .accessibilityIdentifier("charging.editor.review-latest")
        }
    }

    private func costChoice(_ cost: MobileChargingCostDto) -> some View {
        let selected = model.cost == cost
        let key = cost == .free ? "charging.cost.free" : "charging.cost.paid"
        return Button {
            model.cost = cost
        } label: {
            Label(localizedAppText(key), systemImage: selected ? "checkmark.circle.fill" : "circle")
                .frame(maxWidth: .infinity, minHeight: 44)
        }
        .buttonStyle(.bordered)
        .tint(selected ? .accentColor : .secondary)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityRemoveTraits(selected ? [] : .isSelected)
        .accessibilityIdentifier(cost == .free ? "charging.editor.cost.free" : "charging.editor.cost.paid")
    }

    private var coordinateFields: some View {
        Group {
            LabeledContent(localizedAppText("charging.editor.latitude")) {
                TextField("", text: $model.latitudeText)
                    #if os(iOS)
                        .keyboardType(.numbersAndPunctuation)
                    #endif
                    .focused($focusedField, equals: .latitude)
                    .multilineTextAlignment(.trailing)
                    .accessibilityLabel(localizedAppText("charging.editor.latitude"))
                    .accessibilityIdentifier("charging.editor.latitude")
            }
            LabeledContent(localizedAppText("charging.editor.longitude")) {
                TextField("", text: $model.longitudeText)
                    #if os(iOS)
                        .keyboardType(.numbersAndPunctuation)
                    #endif
                    .focused($focusedField, equals: .longitude)
                    .multilineTextAlignment(.trailing)
                    .accessibilityLabel(localizedAppText("charging.editor.longitude"))
                    .accessibilityIdentifier("charging.editor.longitude")
            }
        }
    }

    @ViewBuilder private var optionalDetails: some View {
        LabeledContent(localizedAppText("charging.editor.name")) {
            TextField("", text: $model.name)
                .focused($focusedField, equals: .name)
                .accessibilityLabel(localizedAppText("charging.editor.name"))
                .accessibilityIdentifier("charging.editor.name")
        }
        Picker(localizedAppText("charging.editor.venue"), selection: $model.venue) {
            Text(localizedAppText("charging.editor.venue.choose")).tag(nil as MobileChargingVenueDto?)
            Text(localizedAppText("charging.venue.restaurant")).tag(
                MobileChargingVenueDto.restaurant as MobileChargingVenueDto?)
            Text(localizedAppText("charging.venue.cafe")).tag(MobileChargingVenueDto.cafe as MobileChargingVenueDto?)
            Text(localizedAppText("charging.venue.bar")).tag(MobileChargingVenueDto.bar as MobileChargingVenueDto?)
            Text(localizedAppText("charging.venue.ev")).tag(
                MobileChargingVenueDto.evChargingStation as MobileChargingVenueDto?)
            Text(localizedAppText("charging.venue.other")).tag(MobileChargingVenueDto.other as MobileChargingVenueDto?)
        }
        .accessibilityIdentifier("charging.editor.venue")
        VStack(alignment: .leading, spacing: 8) {
            Text(localizedAppText("charging.editor.hours"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            TextField("", text: $model.hours, axis: .vertical)
                .lineLimit(1...4)
                .focused($focusedField, equals: .hours)
                .accessibilityLabel(localizedAppText("charging.editor.hours"))
                .accessibilityIdentifier("charging.editor.hours")
        }
        if model.cost == .paid {
            Picker(localizedAppText("charging.editor.terms"), selection: $model.terms) {
                Text(localizedAppText("charging.editor.terms.choose")).tag(nil as MobileChargingTermsDto?)
                Text(localizedAppText("charging.terms.direct_fee")).tag(
                    MobileChargingTermsDto.directFee as MobileChargingTermsDto?)
                Text(localizedAppText("charging.terms.purchase")).tag(
                    MobileChargingTermsDto.purchaseRequired as MobileChargingTermsDto?)
                Text(localizedAppText("charging.terms.both")).tag(
                    MobileChargingTermsDto.both as MobileChargingTermsDto?)
            }
            Picker(localizedAppText("charging.editor.price.basis"), selection: $model.priceBasis) {
                Text(localizedAppText("charging.editor.price.basis.choose")).tag(nil as MobileChargingPriceBasisDto?)
                Text(localizedAppText("charging.price.per_charge")).tag(
                    MobileChargingPriceBasisDto.perCharge as MobileChargingPriceBasisDto?)
                Text(localizedAppText("charging.price.per_hour")).tag(
                    MobileChargingPriceBasisDto.perHour as MobileChargingPriceBasisDto?)
                Text(localizedAppText("charging.price.per_kwh")).tag(
                    MobileChargingPriceBasisDto.perKwh as MobileChargingPriceBasisDto?)
                Text(localizedAppText("charging.price.purchase")).tag(
                    MobileChargingPriceBasisDto.purchase as MobileChargingPriceBasisDto?)
            }
            LabeledContent(localizedAppText("charging.editor.price.amount")) {
                TextField("", text: $model.priceAmount)
                    #if os(iOS)
                        .keyboardType(.decimalPad)
                    #endif
                    .focused($focusedField, equals: .amount)
                    .multilineTextAlignment(.trailing)
                    .accessibilityLabel(localizedAppText("charging.editor.price.amount"))
                    .accessibilityIdentifier("charging.editor.price.amount")
            }
            LabeledContent(localizedAppText("charging.editor.price.currency")) {
                TextField("", text: $model.priceCurrency)
                    #if os(iOS)
                        .textInputAutocapitalization(.characters)
                        .autocorrectionDisabled()
                    #endif
                    .focused($focusedField, equals: .currency)
                    .multilineTextAlignment(.trailing)
                    .accessibilityLabel(localizedAppText("charging.editor.price.currency"))
                    .accessibilityIdentifier("charging.editor.price.currency")
            }
            if model.hasIncompletePrice {
                Text(localizedAppText("charging.editor.error.price"))
                    .font(.footnote)
                    .foregroundStyle(.red)
                    .accessibilityIdentifier("charging.editor.price.error")
            }
        }
        Picker(localizedAppText("charging.editor.country"), selection: $model.countrySelection) {
            Text(localizedAppText("charging.editor.country.unknown")).tag(nil as String?)
            ForEach(Self.countryOptions, id: \.code) { country in
                Text(country.name).tag(country.code as String?)
            }
        }
        .pickerStyle(.menu)
        .accessibilityIdentifier("charging.editor.country")
        Button(localizedAppText("charging.editor.supply.add")) {
            if model.venue == .evChargingStation { model.addSupply() } else { model.addSuggestedSupply() }
        }
        .accessibilityIdentifier("charging.editor.supply.add")
        coordinateFields
        ForEach(model.supplies) { draft in
            ChargingSupplyForm(
                draft: draft,
                outletChoices: model.outletChoices,
                focusedField: $focusedField
            ) {
                model.removeSupply(id: draft.id)
            }
        }
    }
}
