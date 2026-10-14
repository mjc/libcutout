import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation

typealias ChargingSpotSave =
    @MainActor @Sendable (MobileChargingInputDto) async throws -> MobileChargingWriteReceiptDto?

@MainActor
@Observable
final class ChargingSpotEditorModel {
    enum Stage: Equatable { case moving, details, saving }
    private enum Destination: Sendable {
        case create
        case edit(id: String, revision: UInt64)
    }

    private(set) var stage: Stage
    private(set) var coordinate: MobileMapCoordinateDto
    var latitudeText: String {
        didSet {
            errorKey = nil
            updateCoordinateFromText()
        }
    }
    var longitudeText: String {
        didSet {
            errorKey = nil
            updateCoordinateFromText()
        }
    }
    var accessNote = "" { didSet { errorKey = nil } }
    var name = "" { didSet { errorKey = nil } }
    var venue: MobileChargingVenueDto? {
        didSet {
            errorKey = nil
            venueChanged(from: oldValue)
        }
    }
    var hours = "" { didSet { errorKey = nil } }
    var cost: MobileChargingCostDto? {
        didSet {
            errorKey = nil
            if cost == .free {
                terms = nil
                priceAmount = ""
                priceCurrency = ""
                priceBasis = nil
            }
        }
    }
    var terms: MobileChargingTermsDto? { didSet { errorKey = nil } }
    var priceAmount = "" {
        didSet {
            originalPrice = nil
            errorKey = nil
        }
    }
    var priceCurrency = "" {
        didSet {
            originalPrice = nil
            errorKey = nil
        }
    }
    var priceBasis: MobileChargingPriceBasisDto? {
        didSet {
            originalPrice = nil
            errorKey = nil
        }
    }
    var supplies = [ChargingSupplyDraft]()
    var areOptionalDetailsExpanded = false
    private(set) var errorKey: String?
    private(set) var countryCode: String?
    private(set) var isCountryCodeManuallySelected = false
    @ObservationIgnored private var originalPrice: MobileChargingPriceDto?
    var countrySelection: String? {
        get { countryCode }
        set { chooseCountryCode(newValue) }
    }
    private(set) var outletChoices: [MobileChargingConnectorDto]
    private(set) var powerSuggestion: MobileChargingSupplyDto
    private(set) var isEditing: Bool
    let inputLocale: Locale

    private let saveSpot: ChargingSpotSave

    private var parsedCoordinate: MobileMapCoordinateDto? {
        guard let latitude = Double(latitudeText), let longitude = Double(longitudeText),
            latitude.isFinite, longitude.isFinite,
            (-90...90).contains(latitude), (-180...180).contains(longitude)
        else { return nil }
        return MobileMapCoordinateDto(latitudeDegrees: latitude, longitudeDegrees: longitude)
    }

    var hasInvalidCoordinate: Bool { parsedCoordinate == nil }

    private var parsedPrice: MobileChargingPriceDto? {
        if let originalPrice { return originalPrice }
        return ChargingPriceDraft.makePrice(
            amount: priceAmount, currency: priceCurrency, basis: priceBasis, locale: inputLocale
        )
    }

    var hasIncompletePrice: Bool {
        priceAmount.isEmpty && priceCurrency.isEmpty && priceBasis == nil
            ? false
            : parsedPrice == nil
    }

    var hasInvalidOptionalDetails: Bool {
        (cost == .paid && hasIncompletePrice) || supplies.contains { $0.value() == nil && !$0.isEmpty }
    }

    var canSave: Bool {
        guard stage == .details, parsedCoordinate != nil,
            !accessNote.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            cost != nil
        else { return false }
        return !hasInvalidOptionalDetails
    }

    init(
        coordinate: MobileMapCoordinateDto, countryCode: String? = nil, inputLocale: Locale = .current,
        save: ChargingSpotSave? = nil
    ) {
        let countryCode = Self.normalizedCountryCode(countryCode)
        let suggestion = mobileChargingPowerSuggestion(countryCode: countryCode, evStation: false)
        self.stage = .moving
        self.coordinate = coordinate
        self.latitudeText = Self.coordinateText(coordinate.latitudeDegrees)
        self.longitudeText = Self.coordinateText(coordinate.longitudeDegrees)
        self.countryCode = countryCode
        self.outletChoices = mobileChargingOutletChoices(countryCode: countryCode)
        self.powerSuggestion = suggestion
        self.isEditing = false
        self.inputLocale = inputLocale
        self.saveSpot = save ?? { fields in try await Self.persist(.create, fields: fields) }
    }

    convenience init?(
        spot: MobileChargingSpotDto, countryCode: String? = nil, inputLocale: Locale = .current,
        save: ChargingSpotSave? = nil
    ) {
        guard let input = spot.input else { return nil }
        self.init(
            draft: input, countryCode: countryCode, inputLocale: inputLocale,
            save: save ?? { fields in
                try await Self.persist(.edit(id: spot.id, revision: spot.localRevision), fields: fields)
            })
    }

    init(
        draft input: MobileChargingInputDto, countryCode: String? = nil, inputLocale: Locale = .current,
        save: @escaping ChargingSpotSave
    ) {
        let countryCode = Self.normalizedCountryCode(countryCode)
        let suggestion = mobileChargingPowerSuggestion(
            countryCode: countryCode,
            evStation: input.venue == .evChargingStation
        )
        self.stage = .details
        self.areOptionalDetailsExpanded = true
        self.coordinate = input.coordinate
        self.latitudeText = Self.coordinateText(input.coordinate.latitudeDegrees)
        self.longitudeText = Self.coordinateText(input.coordinate.longitudeDegrees)
        self.accessNote = input.accessNote
        self.name = input.name ?? ""
        self.venue = input.venue
        self.hours = input.hours ?? ""
        self.cost = input.cost
        self.terms = input.terms
        self.priceAmount = Self.amountText(input.price, locale: inputLocale)
        self.priceCurrency = input.price?.currency ?? ""
        self.priceBasis = input.price?.basis
        self.originalPrice = input.price
        self.supplies = input.supplies.map { ChargingSupplyDraft(supply: $0) }
        self.errorKey = nil
        self.countryCode = countryCode
        self.outletChoices = mobileChargingOutletChoices(countryCode: countryCode)
        self.powerSuggestion = suggestion
        self.isEditing = true
        self.inputLocale = inputLocale
        self.saveSpot = save
        for draft in supplies { observe(draft) }
    }

    func move(to coordinate: MobileMapCoordinateDto) {
        guard stage == .moving else { return }
        self.coordinate = coordinate
        latitudeText = Self.coordinateText(coordinate.latitudeDegrees)
        longitudeText = Self.coordinateText(coordinate.longitudeDegrees)
    }

    func confirmLocation() {
        guard stage == .moving else { return }
        stage = .details
    }

    func movePin() {
        guard stage == .details else { return }
        if let coordinate = parsedCoordinate { self.coordinate = coordinate }
        stage = .moving
        errorKey = nil
    }

    /// Applies automatic country context unless the rider has corrected it in the form.
    func setCountryCode(_ countryCode: String?) {
        guard !isCountryCodeManuallySelected else { return }
        applyCountryCode(countryCode)
    }

    func chooseCountryCode(_ countryCode: String?) {
        isCountryCodeManuallySelected = true
        applyCountryCode(countryCode)
    }

    private func applyCountryCode(_ value: String?) {
        let countryCode = Self.normalizedCountryCode(value)
        guard self.countryCode != countryCode else { return }
        self.countryCode = countryCode
        outletChoices = mobileChargingOutletChoices(countryCode: countryCode)
        powerSuggestion = mobileChargingPowerSuggestion(
            countryCode: countryCode, evStation: venue == .evChargingStation)
        if venue != .evChargingStation {
            for draft in supplies where draft.origin == .countrySuggestion {
                draft.updateFromCountrySuggestion(powerSuggestion)
            }
        }
    }

    private func updateCoordinateFromText() {
        if let parsedCoordinate { coordinate = parsedCoordinate }
    }

    func setVenue(_ venue: MobileChargingVenueDto?) { self.venue = venue }

    func addSuggestedSupply() {
        guard venue != .evChargingStation else { return }
        let draft = ChargingSupplyDraft(supply: powerSuggestion, origin: .countrySuggestion)
        observe(draft)
        supplies.append(draft)
    }

    func addSupply() {
        let draft = ChargingSupplyDraft()
        observe(draft)
        supplies.append(draft)
    }
    func removeSupply(id: UUID) { supplies.removeAll { $0.id == id } }

    func save() async -> MobileChargingWriteReceiptDto? {
        guard canSave, let coordinate = parsedCoordinate, let cost else { return nil }
        stage = .saving
        errorKey = nil
        let fields = MobileChargingInputDto(
            coordinate: coordinate,
            accessNote: accessNote.trimmingCharacters(in: .whitespacesAndNewlines),
            cost: cost,
            name: optionalText(name),
            venue: venue,
            hours: optionalText(hours),
            terms: cost == .paid ? terms : nil,
            price: cost == .paid ? parsedPrice : nil,
            supplies: supplies.compactMap { $0.value() }
        )
        do {
            guard let receipt = try await saveSpot(fields) else {
                stage = .details
                errorKey = "charging.editor.error.failed"
                return nil
            }
            self.coordinate = coordinate
            return receipt
        } catch let error as MobileChargingError {
            stage = .details
            errorKey =
                switch error {
                case .AccessNoteBlank: "charging.editor.error.access_note_blank"
                case .AccessNoteTooLong: "charging.editor.error.access_note_too_long"
                case .AccessNoteControlCharacter: "charging.editor.error.access_note_control_character"
                case .InvalidPin: "charging.editor.error.coordinate"
                case .InvalidMetadata: "charging.editor.error.metadata"
                case .StorageFailure: "charging.editor.error.failed"
                default: ChargingSpotPresentation.loadErrorKey(error)
                }
            return nil
        } catch {
            stage = .details
            errorKey = "charging.editor.error.failed"
            return nil
        }
    }

    private func venueChanged(from oldValue: MobileChargingVenueDto?) {
        guard oldValue != venue else { return }
        powerSuggestion = mobileChargingPowerSuggestion(
            countryCode: countryCode,
            evStation: venue == .evChargingStation
        )
        if venue == .evChargingStation { supplies.removeAll { $0.origin == .countrySuggestion } }
    }

    private func observe(_ draft: ChargingSupplyDraft) {
        draft.onChange = { [weak self] in self?.errorKey = nil }
    }

    private static func normalizedCountryCode(_ value: String?) -> String? {
        guard let value, value.count == 2,
            value.unicodeScalars.allSatisfy({
                (65...90).contains(Int($0.value)) || (97...122).contains(Int($0.value))
            })
        else { return nil }
        return value.uppercased()
    }

    private static func coordinateText(_ value: Double) -> String { String(value) }

    private static func amountText(_ price: MobileChargingPriceDto?, locale: Locale) -> String {
        guard let price else { return "" }
        let digits = String(price.amountMinor).leftPadded(to: Int(price.minorUnitExponent) + 1)
        let exponent = Int(price.minorUnitExponent)
        guard exponent > 0 else { return digits }
        let split = digits.index(digits.endIndex, offsetBy: -exponent)
        return "\(digits[..<split])\(locale.decimalSeparator ?? ".")\(digits[split...])"
    }

    private func optionalText(_ value: String) -> String? {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private static func persist(_ destination: Destination, fields: MobileChargingInputDto) async throws
        -> MobileChargingWriteReceiptDto
    {
        do {
            let database = try await RustPersistenceStore.open()
            switch destination {
            case .create: return try await database.saveChargingSpot(fields: fields)
            case .edit(let id, let revision):
                return try await database.editChargingSpot(id: id, expectedRevision: revision, fields: fields)
            }
        } catch let error as MobileChargingError {
            throw error
        } catch {
            throw MobileChargingError.Unavailable
        }
    }
}

extension String {
    fileprivate func leftPadded(to length: Int) -> String {
        String(repeating: "0", count: max(0, length - count)) + self
    }
}
