import CoreLocation
import CutoutMobile
import CutoutMobileFFI
import MapKit
import SwiftUI

struct ChargingMapContentView: View {
    private enum DisplayMode: Hashable {
        case map
        case list
    }

    private struct DetailItem: Identifiable {
        let id: String
        let fallback: MobileChargingSpotDto

        init(_ spot: MobileChargingSpotDto) {
            id = spot.id
            fallback = spot
        }
    }

    private enum SheetRoute: Identifiable {
        case details(DetailItem)
        case issues
        case editor(ChargingSpotEditorModel)

        var id: String {
            switch self {
            case .details(let item): "details-\(item.id)"
            case .issues: "issues"
            case .editor(let model): "editor-\(ObjectIdentifier(model))"
            }
        }
    }

    private struct CountryLookupID: Hashable {
        let editor: ObjectIdentifier
        let hasLocation: Bool
    }

    @Bindable private var model: ChargingMapModel
    @Binding private var editor: ChargingSpotEditorModel?
    @Binding private var mapPosition: MapCameraPosition
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.locale) private var locale
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    private let cameraDidChange: (MapCameraUpdateContext) -> Void
    private let defaultCameraSpan: MKCoordinateSpan
    private let phoneLocationSample: MobilePhoneLocationSampleDto?
    @State private var locationSamples = ChargingLocationSampleSelector()
    @State private var mapCameraSnapshot = ChargingMapCameraSnapshot()
    @State private var pendingMapSelection = false
    @State private var visibleCenter: CLLocationCoordinate2D?
    @State private var cameraSpan: MKCoordinateSpan?
    @State private var userMovedCamera = false
    @State private var displayMode = DisplayMode.map
    @State private var sheetRoute: SheetRoute?
    @State private var editorLaunchError: MobileChargingError?
    @State private var knownVoltages: Set<UInt16> = [120, 230, 240]
    @State private var cameraRevision = 0
    @State private var selectedPopupHeight: CGFloat = 154

    @ScaledMetric(relativeTo: .body) private var pinHeight: CGFloat = 46
    private let pinTailHeight: CGFloat = 7

    init(
        model: ChargingMapModel, editor: Binding<ChargingSpotEditorModel?>,
        mapPosition: Binding<MapCameraPosition>,
        cameraDidChange: @escaping (MapCameraUpdateContext) -> Void = { _ in },
        defaultCameraSpan: MKCoordinateSpan = RideMapPresentationState.sharedLocalSpan,
        phoneLocationSample: MobilePhoneLocationSampleDto? = nil
    ) {
        self.model = model
        self._editor = editor
        self._mapPosition = mapPosition
        self.cameraDidChange = cameraDidChange
        self.defaultCameraSpan = defaultCameraSpan
        self.phoneLocationSample = phoneLocationSample
    }

    var body: some View {
        locationAwareContent
            .sheet(item: presentedSheet) { route in
                switch route {
                case .details(let item):
                    let spot = currentSpot(for: item)
                    ChargingSpotDetailView(
                        spot: spot,
                        isDeleting: model.pendingWithdrawalIDs.contains(item.id),
                        errorKey: model.error.map(ChargingSpotPresentation.loadErrorKey),
                        onEdit: beginEditing,
                        onDelete: { spot in
                            Task {
                                if await model.withdraw(spot) { sheetRoute = nil }
                            }
                        }
                    )
                case .issues:
                    ChargingSyncIssuesView(model: model, onEdit: beginEditingConflict)
                case .editor(let editor):
                    ChargingSpotEditorView(model: editor, onCancel: cancelEditor) { receipt in
                        finishAddingSpot(receipt, coordinate: editor.coordinate)
                    }
                }
            }
    }

    private var mapLayout: some View {
        VStack(spacing: 0) {
            controls
            Group {
                if displayMode == .list {
                    VStack(spacing: 0) {
                        spotList
                        status
                    }
                } else {
                    GeometryReader { mapArea in
                        let placementIsAccessibilitySize =
                            dynamicTypeSize.isAccessibilitySize && editor?.stage == .moving
                        // Reserve space relative to the map region so the inset stays within its proposal.
                        let usableHeight = mapArea.size.height.isFinite ? max(0, mapArea.size.height) : 0
                        let detailMapInsetHeight = usableHeight * 0.45
                        VStack(spacing: 0) {
                            map
                                .frame(maxWidth: .infinity)
                                .frame(
                                    height: placementIsAccessibilitySize
                                        ? usableHeight * 0.4
                                        : nil
                                )
                                .safeAreaInset(edge: .bottom, spacing: 0) {
                                    if editor?.stage == .details || editor?.stage == .saving {
                                        Color.clear
                                            .frame(height: detailMapInsetHeight)
                                            .accessibilityHidden(true)
                                    }
                                }
                                .overlay(alignment: .bottom) {
                                    if !placementIsAccessibilitySize {
                                        VStack(spacing: 0) { if editor == nil { status } }
                                            .frame(maxWidth: .infinity)
                                            .background(.regularMaterial)
                                    }
                                }
                            if placementIsAccessibilitySize {
                                ScrollView {
                                    placementControls(isAccessibilitySize: true)
                                }
                                .frame(maxWidth: .infinity)
                                .frame(height: usableHeight * 0.6)
                                .accessibilityIdentifier("charging.placement.scroll")
                            }
                        }
                        .frame(
                            height: placementIsAccessibilitySize ? usableHeight : nil,
                            alignment: .top
                        )
                        .clipped()
                        .contentShape(Rectangle())
                    }
                }
            }
            .overlay(alignment: .top) {
                if model.isLoading {
                    HStack {
                        ProgressView()
                        Text(localizedAppText("charging.status.loading"))
                    }
                    .font(.footnote)
                    .padding(8)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
                    .allowsHitTesting(false)
                }
            }
        }
    }

    private var queryAwareContent: some View {
        mapLayout
            .task(id: model.queryID) { await model.load() }
            .onChange(of: model.discoveryTarget, initial: true) { _, _ in
                applyPendingDiscoveryTarget()
            }
            .onChange(of: displayMode) { _, mode in
                guard mode == .map else { return }
                if pendingMapSelection {
                    pendingMapSelection = false
                } else if applyPendingDiscoveryTarget() {
                    return
                } else if let camera = mapCameraSnapshot.camera {
                    mapPosition = .camera(camera)
                }
            }
            .onChange(of: model.spots, initial: true) { _, spots in
                let supplies = spots.compactMap(\.input).flatMap(\.supplies)
                knownVoltages.formUnion(ChargingSpotPresentation.voltageOptions(in: supplies))
            }
            .onChange(of: editor == nil) { _, isClosed in
                if isClosed { applyPendingDiscoveryTarget() }
            }
            .onChange(of: model.userLocation) { _, _ in
                applyPendingDiscoveryTarget()
            }
    }

    @discardableResult
    private func applyPendingDiscoveryTarget() -> Bool {
        guard displayMode == .map, editor == nil, !userMovedCamera,
            let origin = model.userLocation,
            let target = model.consumeDiscoveryTarget()
        else { return false }
        mapPosition = .region(ChargingMapCamera.regionIncluding(origin: origin, target: target))
        return true
    }

    private var locationAwareContent: some View {
        queryAwareContent
            .task { model.communitySyncRequested() }
            .task { await followUserLocation() }
            .task(id: countryLookupID) { await resolveEditorCountry() }
            .onChange(of: editor?.stage, editorStageChanged)
            .onChange(of: editor?.latitudeText) { _, _ in recenterEditedCoordinate() }
            .onChange(of: editor?.longitudeText) { _, _ in recenterEditedCoordinate() }
            .onChange(of: phoneLocationSample, initial: true, phoneLocationChanged)
    }

    private var countryLookupID: CountryLookupID? {
        editor.map {
            CountryLookupID(editor: ObjectIdentifier($0), hasLocation: locationSamples.freshLocation() != nil)
        }
    }

    private func resolveEditorCountry() async {
        guard let editor else { return }
        guard let location = locationSamples.freshLocation() else {
            editor.setCountryCode(nil)
            editor.setConnectorChoices(nil)
            return
        }
        let origin = Self.dto(location.coordinate)
        let countryCode = await ChargingCountryLookup.countryCode(at: origin)
        guard !Task.isCancelled else { return }
        editor.setCountryCode(countryCode)
        do {
            let database = try await RustPersistenceStore.open()
            let choices = try await database.chargingConnectorChoicesNear(
                origin: origin, locationCountryCode: countryCode)
            guard !Task.isCancelled else { return }
            editor.setConnectorChoices(choices)
        } catch {
            guard !Task.isCancelled else { return }
            editor.setConnectorChoices(nil)
        }
    }

    private func editorStageChanged(_: ChargingSpotEditorModel.Stage?, _ stage: ChargingSpotEditorModel.Stage?) {
        guard stage == ChargingSpotEditorModel.Stage.moving, let editor else { return }
        visibleCenter = Self.coordinate(editor.coordinate)
        centerMap(on: Self.coordinate(editor.coordinate), span: cameraSpan ?? defaultCameraSpan)
    }

    private func recenterEditedCoordinate() {
        guard let editor, editor.stage == .details, !editor.hasInvalidCoordinate else { return }
        centerMap(on: Self.coordinate(editor.coordinate), span: cameraSpan ?? defaultCameraSpan)
    }

    private func phoneLocationChanged(_: MobilePhoneLocationSampleDto?, _ sample: MobilePhoneLocationSampleDto?) {
        guard let sample, let location = locationSamples.consider(sample, now: Date()) else { return }
        acceptUserLocation(location)
    }

    private var controls: some View {
        Group {
            if dynamicTypeSize.isAccessibilitySize {
                VStack(spacing: 4) {
                    displayModePicker
                    HStack {
                        filterMenu
                        Spacer(minLength: 12)
                        addButton
                    }
                }
            } else {
                HStack {
                    displayModePicker
                    filterMenu
                    addButton
                }
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    private var displayModePicker: some View {
        HStack(spacing: 0) {
            displayModeButton(.map)
            displayModeButton(.list)
        }
        .padding(2)
        .background(PevColors.cardFill, in: Capsule())
        .overlay { Capsule().stroke(PevColors.cardStroke, lineWidth: 1) }
    }

    private var addButton: some View {
        Button {
            beginAddingAtCurrentLocation()
        } label: {
            Image(systemName: "plus")
                .frame(minWidth: 44, minHeight: 44)
        }
        .disabled(editor != nil || (visibleCenter == nil && locationSamples.freshLocation(at: Date()) == nil))
        .accessibilityLabel(localizedAppText("charging.add"))
        .accessibilityIdentifier("charging.add")
    }

    private func displayModeButton(_ mode: DisplayMode) -> some View {
        let selected = displayMode == mode
        let key = mode == .map ? "charging.view.map" : "charging.view.list"
        return Button {
            displayMode = mode
        } label: {
            Text(localizedAppText(key))
                .font(.callout)
                .frame(minWidth: 64, maxWidth: .infinity, minHeight: 44)
                .background(selected ? PevColors.pageBackground : .clear, in: Capsule())
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .disabled(editor != nil)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityRemoveTraits(selected ? [] : .isSelected)
        .accessibilityIdentifier(key)
    }

    private var filterMenu: some View {
        Menu {
            Menu(localizedAppText("charging.filter.cost")) {
                filterButton("charging.filter.all", selected: model.filter.cost == nil) { $0.cost = nil }
                filterButton("charging.cost.free", selected: model.filter.cost == .free) { $0.cost = .free }
                filterButton("charging.cost.paid", selected: model.filter.cost == .paid) { $0.cost = .paid }
            }
            Menu(localizedAppText("charging.filter.venue")) {
                filterButton("charging.filter.all", selected: model.filter.venue == nil) { $0.venue = nil }
                venueOption(.restaurant, key: "charging.venue.restaurant")
                venueOption(.cafe, key: "charging.venue.cafe")
                venueOption(.bar, key: "charging.venue.bar")
                venueOption(.evChargingStation, key: "charging.venue.ev")
                venueOption(.other, key: "charging.venue.other")
            }
            Menu(localizedAppText("charging.filter.connector")) {
                filterButton("charging.filter.all", selected: model.filter.connector == nil) { $0.connector = nil }
                ForEach(ChargingSpotPresentation.connectorOptions, id: \.key) { option in
                    filterButton(option.key, selected: model.filter.connector == option.value) {
                        $0.connector = option.value
                    }
                }
            }
            Menu(localizedAppText("charging.filter.voltage")) {
                filterButton("charging.filter.all", selected: model.filter.voltageVolts == nil) {
                    $0.voltageVolts = nil
                }
                ForEach(knownVoltages.sorted(), id: \.self) { voltage in
                    Button("\(voltage) V") { updateFilter { $0.voltageVolts = voltage } }
                }
            }
        } label: {
            Label(localizedAppText("charging.filter.title"), systemImage: "line.3.horizontal.decrease")
        }
        .frame(minWidth: 44, minHeight: 44)
        .accessibilityIdentifier("charging.filters")
    }

    private func venueOption(_ venue: MobileChargingVenueDto, key: String) -> some View {
        filterButton(key, selected: model.filter.venue == venue) { $0.venue = venue }
    }

    private func filterButton(_ key: String, selected: Bool, change: @escaping (inout MobileChargingFilterDto) -> Void)
        -> some View
    {
        Button {
            updateFilter(change)
        } label: {
            if selected { Label(localizedAppText(key), systemImage: "checkmark") } else { Text(localizedAppText(key)) }
        }
    }

    private func updateFilter(_ change: (inout MobileChargingFilterDto) -> Void) {
        var filter = model.filter
        change(&filter)
        model.setFilter(filter)
    }

    private var selectedSpotVisibility: ChargingSpotPresentation.SavedSelectionVisibility {
        let selected = model.selectedSpot
        let hasValidInput =
            selected?.input.map {
                CLLocationCoordinate2DIsValid(Self.coordinate($0.coordinate))
            } ?? false
        let hasVisibleStatus = selected.map { $0.status == .queued || $0.status == .published } ?? false
        let filter = model.filter
        return ChargingSpotPresentation.savedSelectionVisibility(
            selectedID: selected?.id,
            pageIDs: model.spots.map(\.id),
            hasValidInput: hasValidInput,
            hasVisibleStatus: hasVisibleStatus,
            filtersAreEmpty: filter.cost == nil && filter.connector == nil
                && filter.voltageVolts == nil && filter.venue == nil
        )
    }

    private var map: some View {
        let selected = model.selectedSpot
        let selectedVisibility = selectedSpotVisibility
        let selectionIsOnMap = selectedVisibility == .page || selectedVisibility == .separateAnnotation
        let spots = model.spots
        let _ = cameraRevision
        return MapReader { proxy in
            Map(
                position: $mapPosition, interactionModes: [.pan, .zoom],
                selection: Binding<String?>(
                    get: { selectionIsOnMap ? model.selectedSpot?.id : nil },
                    set: { id in
                        // Filtered-out and moderated selections remain available in details.
                        if id != nil || selectionIsOnMap { model.select(id: id) }
                    }
                )
            ) {
                UserAnnotation()
                ForEach(spots, id: \.id) { spot in
                    if let input = spot.input {
                        Annotation(
                            input.name ?? ChargingSpotPresentation.unknownName,
                            coordinate: CLLocationCoordinate2D(
                                latitude: input.coordinate.latitudeDegrees,
                                longitude: input.coordinate.longitudeDegrees
                            ), anchor: .bottom
                        ) {
                            pinButton(spot, selected: selected?.id == spot.id)
                        }
                        .tag(spot.id)
                        .annotationTitles(.hidden)
                    }
                }
                if selectedVisibility == .separateAnnotation, let selected, let input = selected.input {
                    Annotation(
                        input.name ?? ChargingSpotPresentation.unknownName,
                        coordinate: Self.coordinate(input.coordinate),
                        anchor: .bottom
                    ) {
                        pinButton(selected, selected: true)
                    }
                    .tag(selected.id)
                    .annotationTitles(.hidden)
                }
                if let editor, editor.stage == .details || editor.stage == .saving {
                    Annotation(
                        localizedAppText("charging.placement.title"),
                        coordinate: Self.coordinate(editor.coordinate),
                        anchor: .bottom
                    ) {
                        ChargingPlacementPin()
                            .allowsHitTesting(false)
                            .accessibilityHidden(true)
                    }
                    .annotationTitles(.hidden)
                }
            }
            .accessibilityLabel(localizedAppText("charging.map.label"))
            .mapStyle(.standard(elevation: .flat, pointsOfInterest: .excludingAll, showsTraffic: false))
            .onChange(of: model.selectedSpot?.id, initial: true) { _, _ in
                selectedPopupHeight = 154
            }
            .onMapCameraChange(frequency: .continuous) { context in
                mapCameraSnapshot.capture(context.camera)
                if selected != nil { cameraRevision &+= 1 }
                if editor?.stage == .moving {
                    visibleCenter = context.region.center
                    editor?.move(to: Self.dto(context.region.center))
                }
            }
            .onMapCameraChange(frequency: .onEnd, cameraChanged)
            .overlay {
                if editor?.stage == .moving {
                    ChargingPlacementPin()
                        .offset(y: -27)
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
            }
            .overlay(alignment: .bottom) {
                if editor?.stage == .moving && !dynamicTypeSize.isAccessibilitySize {
                    placementControls(isAccessibilitySize: false)
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if editor == nil {
                    Button {
                        guard let location = locationSamples.freshLocation(at: Date()) else { return }
                        model.cancelAutomaticDiscovery()
                        centerMap(on: location.coordinate, span: cameraSpan ?? defaultCameraSpan)
                    } label: {
                        Image(systemName: "location.fill").padding(14)
                    }
                    .background(.regularMaterial, in: Circle())
                    .disabled(locationSamples.freshLocation(at: Date()) == nil)
                    .accessibilityLabel(localizedAppText("charging.location.center"))
                    .padding(.trailing, 12)
                    .padding(.bottom, 48)
                }
            }
            .overlay {
                GeometryReader { geometry in
                    if case nil = sheetRoute, editor == nil, let selected,
                        let input = selected.input,
                        selectionIsOnMap,
                        let point = proxy.convert(
                            CLLocationCoordinate2D(
                                latitude: input.coordinate.latitudeDegrees,
                                longitude: input.coordinate.longitudeDegrees
                            ),
                            to: .local
                        ),
                        CGRect(origin: .zero, size: geometry.size).contains(point)
                    {
                        let belowTop = point.y + 8
                        let top: CGFloat =
                            belowTop + selectedPopupHeight <= geometry.size.height - 8
                            ? belowTop
                            : max(8, point.y - pinHeight - 8 - selectedPopupHeight)
                        let panelWidth =
                            dynamicTypeSize.isAccessibilitySize
                            ? max(0, geometry.size.width - 24)
                            : 180
                        votePanel(selected, panelWidth: panelWidth)
                            .onGeometryChange(for: CGFloat.self) { proxy in
                                proxy.size.height
                            } action: { height in
                                if selectedPopupHeight != height { selectedPopupHeight = height }
                            }
                            .position(
                                x: ChargingMapCamera.popupCenterX(
                                    pinX: point.x, mapWidth: geometry.size.width, panelWidth: panelWidth
                                ),
                                y: top + selectedPopupHeight / 2
                            )
                    }
                }
            }
        }
    }

    private var presentedSheet: Binding<SheetRoute?> {
        Binding(
            get: {
                if let editor, editor.stage != .moving { return .editor(editor) }
                return sheetRoute
            },
            set: { route in
                guard let route else {
                    cancelEditor()
                    return
                }
                if case .editor = route {
                    sheetRoute = nil
                } else {
                    sheetRoute = route
                }
            }
        )
    }

    private func placementControls(isAccessibilitySize: Bool) -> some View {
        VStack(spacing: 8) {
            Text(localizedAppText("charging.placement.title"))
                .font(.headline)
                .accessibilityIdentifier("charging.placement.title")
            Text(localizedAppText("charging.placement.instructions"))
                .font(.footnote)
                .fixedSize(horizontal: false, vertical: true)
                .multilineTextAlignment(.leading)
                .accessibilityIdentifier("charging.placement.instructions")
            if isAccessibilitySize {
                placementActions(fullWidth: true)
                useMyLocationButton
            } else {
                useMyLocationButton
                placementActions(fullWidth: false)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
        .padding(8)
    }

    private var useMyLocationButton: some View {
        Button {
            guard let location = locationSamples.freshLocation() else { return }
            editor?.move(to: Self.dto(location.coordinate))
            visibleCenter = location.coordinate
            centerMap(on: location.coordinate, span: cameraSpan ?? defaultCameraSpan)
        } label: {
            Label(localizedAppText("charging.placement.use_my_location"), systemImage: "location.fill")
                .frame(maxWidth: .infinity, minHeight: 44)
        }
        .disabled(locationSamples.freshLocation() == nil)
        .accessibilityIdentifier("charging.placement.use-my-location")
    }

    private func placementActions(fullWidth: Bool) -> some View {
        Group {
            if fullWidth {
                VStack(spacing: 8) {
                    cancelPlacementButton
                    confirmPlacementButton
                }
            } else {
                HStack {
                    cancelPlacementButton
                    Spacer()
                    confirmPlacementButton
                }
            }
        }
    }

    private var cancelPlacementButton: some View {
        Button {
            cancelEditor()
        } label: {
            Text(localizedAppText("charging.editor.cancel"))
                .frame(maxWidth: .infinity, minHeight: 44)
        }
        .accessibilityIdentifier("charging.placement.cancel")
    }

    private var confirmPlacementButton: some View {
        Button {
            if let visibleCenter { editor?.move(to: Self.dto(visibleCenter)) }
            editor?.confirmLocation()
        } label: {
            Text(localizedAppText("charging.placement.confirm"))
                .frame(maxWidth: .infinity, minHeight: 44)
        }
        .buttonStyle(.borderedProminent)
        .accessibilityIdentifier("charging.placement.confirm")
    }

    private func cameraChanged(_ context: MapCameraUpdateContext) {
        visibleCenter = context.region.center
        cameraSpan = context.region.span
        mapCameraSnapshot.capture(context.camera)
        if mapPosition.positionedByUser {
            userMovedCamera = true
            model.cancelAutomaticDiscovery()
            cameraDidChange(context)
        }
        if editor?.stage == .moving {
            editor?.move(to: Self.dto(context.region.center))
        } else if let bounds = RideMapCanvasView.geoBounds(for: context.region) {
            model.setViewport(bounds)
        }
    }

    private func beginPlacement(at coordinate: CLLocationCoordinate2D) {
        guard editor == nil, CLLocationCoordinate2DIsValid(coordinate) else { return }
        model.cancelAutomaticDiscovery()
        model.deselect()
        if displayMode == .list { pendingMapSelection = true }
        displayMode = .map
        sheetRoute = nil
        editor = ChargingSpotEditorModel(coordinate: Self.dto(coordinate), inputLocale: locale)
        centerMap(on: coordinate, span: cameraSpan ?? defaultCameraSpan)
        visibleCenter = coordinate
    }

    private func beginAddingAtCurrentLocation() {
        let coordinate = locationSamples.freshLocation(at: Date())?.coordinate ?? visibleCenter
        guard let coordinate, CLLocationCoordinate2DIsValid(coordinate) else { return }
        beginPlacement(at: coordinate)
    }

    private func finishAddingSpot(_ receipt: MobileChargingWriteReceiptDto, coordinate: MobileMapCoordinateDto) {
        let isNewSpot = editor?.isEditing == false
        sheetRoute = nil
        editor = nil
        centerMap(on: Self.coordinate(coordinate), span: cameraSpan ?? defaultCameraSpan)
        if isNewSpot {
            model.setFilter(MobileChargingFilterDto(cost: nil, connector: nil, voltageVolts: nil, venue: nil))
        }
        Task {
            await model.refreshSavedSpot(id: receipt.id)
            model.communitySyncRequested()
        }
    }

    private func beginEditing(_ spot: MobileChargingSpotDto) {
        editorLaunchError = nil
        Task {
            do {
                let database = try await RustPersistenceStore.open()
                guard
                    let editor = ChargingSpotEditorModel(
                        spot: spot,
                        inputLocale: locale,
                        reloadLatest: { try await database.chargingSpot(id: spot.id) },
                        save: { fields, revision in
                            guard let revision else { throw MobileChargingError.Conflict }
                            return try await database.editChargingSpot(
                                id: spot.id, expectedRevision: revision, fields: fields)
                        }
                    )
                else { return }
                model.cancelAutomaticDiscovery()
                if displayMode == .list {
                    pendingMapSelection = true
                    displayMode = .map
                }
                self.editor = editor
                sheetRoute = nil
            } catch {
                editorLaunchError = error as? MobileChargingError ?? .StorageFailure
            }
        }
    }

    private func beginEditingConflict(_ issue: MobileChargingSyncConflictDto) async throws {
        guard issue.action.editsFields, let draft = issue.input else { throw MobileChargingError.NotFound }
        let database = try await RustPersistenceStore.open()
        let editor = ChargingSpotEditorModel(
            draft: draft,
            inputLocale: locale,
            expectedRevision: issue.localRevision,
            reloadLatest: { try await database.chargingSpot(id: issue.id) },
            save: { fields, revision in
                guard let revision else { throw MobileChargingError.Conflict }
                return try await database.replaceBlockedChargingOperation(
                    operationId: issue.operationId,
                    expectedLocalRevision: revision,
                    fields: fields)
            })
        self.editor = editor
        sheetRoute = nil
    }

    private func cancelEditor() {
        sheetRoute = nil
        editor = nil
    }

    private func centerMap(on coordinate: CLLocationCoordinate2D, span: MKCoordinateSpan) {
        guard
            let region = RideMapPresentationState.localRegion(
                center: coordinate, longitudeDelta: span.longitudeDelta
            )
        else { return }
        mapPosition = .region(region)
    }

    private func followUserLocation() async {
        do {
            for try await update in CLLocationUpdate.liveUpdates() {
                guard !Task.isCancelled else { return }
                guard let location = update.location,
                    locationSamples.consider(location, now: Date()) != nil
                else { continue }
                acceptUserLocation(location)
            }
        } catch {
            // Location permission and availability are handled by the system map.
            // Browsing and placing a spot still work without a location fix.
        }
    }

    private func acceptUserLocation(_ location: CLLocation) {
        var shouldCenter = model.userLocation == nil && !userMovedCamera && editor == nil
        #if DEBUG
            // Keep fixture pins in view; Add spot still uses the real location sample.
            shouldCenter = shouldCenter && !CutoutUITestChargingFixture.isEnabled
        #endif
        if shouldCenter {
            centerMap(on: location.coordinate, span: cameraSpan ?? defaultCameraSpan)
        }
        model.setUserLocation(Self.dto(location.coordinate))
    }

    private static func coordinate(_ dto: MobileMapCoordinateDto) -> CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: dto.latitudeDegrees, longitude: dto.longitudeDegrees)
    }

    private static func dto(_ coordinate: CLLocationCoordinate2D) -> MobileMapCoordinateDto {
        MobileMapCoordinateDto(latitudeDegrees: coordinate.latitude, longitudeDegrees: coordinate.longitude)
    }

    private func pinButton(_ spot: MobileChargingSpotDto, selected: Bool) -> some View {
        let cost =
            spot.input.map { localizedAppText($0.cost == .free ? "charging.cost.free" : "charging.cost.paid") }
            ?? localizedAppText("charging.value.unknown")
        return Button {
            model.select(spot)
        } label: {
            ZStack(alignment: .top) {
                ChargingPinShape(tailHeight: pinTailHeight)
                    .fill(pinColor(for: spot))
                    .overlay {
                        ChargingPinShape(tailHeight: pinTailHeight).stroke(.white, lineWidth: 2.5)
                    }
                HStack(spacing: 6) {
                    Image(systemName: markerSymbol(for: spot))
                        .font(.body.weight(.semibold))
                    Text(cost)
                        .font(.caption.weight(.bold))
                        .fixedSize()
                }
                .foregroundStyle(.white)
                .padding(.horizontal, 11)
                .frame(minWidth: 64, minHeight: pinHeight - pinTailHeight)
            }
            .fixedSize(horizontal: true, vertical: false)
            .frame(height: pinHeight)
            .overlay {
                if selected {
                    ChargingPinShape(tailHeight: pinTailHeight)
                        .stroke(Color.yellow, lineWidth: 3)
                        .padding(-4)
                }
            }
            .contentShape(ChargingPinShape(tailHeight: pinTailHeight))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(spot.input?.name ?? ChargingSpotPresentation.unknownName), \(cost)")
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityRemoveTraits(selected ? [] : .isSelected)
        .accessibilityIdentifier("charging.pin.\(spot.id)")
    }

    private func votePanel(_ spot: MobileChargingSpotDto, panelWidth: CGFloat) -> some View {
        VStack(spacing: 6) {
            HStack(alignment: .top) {
                if let note = spot.input?.accessNote, !note.isEmpty {
                    Text(note)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                        .truncationMode(.tail)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("charging.selected.access-note")
                } else {
                    Spacer(minLength: 0)
                }
                Button(action: model.deselect) {
                    Image(systemName: "xmark.circle.fill")
                        .frame(minWidth: 44, minHeight: 44)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(localizedAppText("charging.selected.close"))
                .accessibilityIdentifier("charging.selected.close")
            }
            HStack(spacing: 8) {
                voteButton(spot, vote: .up, count: spot.votesUp, key: "charging.vote.up")
                voteButton(spot, vote: .down, count: spot.votesDown, key: "charging.vote.down")
            }
            if model.pendingVoteIDs.contains(spot.id) || spot.pendingVote {
                Text(localizedAppText("charging.vote.queued"))
                    .font(.caption)
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .accessibilityIdentifier("charging.pending-vote")
            }
            Button {
                sheetRoute = .details(DetailItem(spot))
            } label: {
                HStack {
                    Text(localizedAppText("charging.details.button"))
                    Spacer(minLength: 8)
                    Image(systemName: "chevron.right")
                }
                .font(.callout.weight(.semibold))
                .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(localizedAppText("charging.details.open"))
            .accessibilityIdentifier("charging.details.open")
        }
        .padding(8)
        .frame(width: panelWidth)
        .fixedSize(horizontal: false, vertical: true)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
    }

    private func voteButton(
        _ spot: MobileChargingSpotDto,
        vote: MobileChargingVoteDto,
        count: UInt64,
        key: String
    ) -> some View {
        let selected = spot.vote == vote
        return Button {
            Task { await model.vote(vote, forSpotID: spot.id) }
        } label: {
            ZStack(alignment: .topTrailing) {
                HStack(spacing: 7) {
                    Image(systemName: voteSymbol(vote))
                        .font(.system(size: 16, weight: .semibold))
                    if spot.remoteId != nil { Text("\(count)") }
                }
                .font(.callout)
                .frame(maxWidth: .infinity, minHeight: 44)
                if model.pendingVoteIDs.contains(spot.id) {
                    ProgressView().controlSize(.mini).offset(x: -2, y: 2)
                }
            }
            .background {
                Capsule().stroke(selected ? voteTint(vote) : PevColors.cardStroke, lineWidth: selected ? 2 : 1)
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .disabled(model.pendingVoteIDs.contains(spot.id))
        .accessibilityLabel(localizedAppText(key))
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityRemoveTraits(selected ? [] : .isSelected)
        .accessibilityValue(
            ChargingSpotPresentation.voteAccessibilityValue(
                count: spot.remoteId == nil ? nil : count,
                selected: selected,
                locale: locale
            )
        )
        .accessibilityIdentifier(key)
    }

    private func pinColor(for spot: MobileChargingSpotDto) -> Color {
        let free = spot.input?.cost == .free
        if colorScheme == .dark {
            return free
                ? Color(red: 32.0 / 255, green: 115.0 / 255, blue: 66.0 / 255)
                : Color(red: 128.0 / 255, green: 92.0 / 255, blue: 19.0 / 255)
        }
        return free
            ? Color(red: 0, green: 97.0 / 255, blue: 41.0 / 255)
            : Color(red: 122.0 / 255, green: 74.0 / 255, blue: 0)
    }

    private func voteTint(_ vote: MobileChargingVoteDto) -> Color {
        vote == .up ? PevColors.green : PevColors.red
    }

    private func markerSymbol(for spot: MobileChargingSpotDto) -> String {
        switch spot.input?.venue {
        case .restaurant: "fork.knife"
        case .cafe: "cup.and.saucer.fill"
        case .bar: "wineglass.fill"
        case .evChargingStation: "ev.charger.fill"
        case .other, nil: "powerplug.fill"
        }
    }

    private func voteSymbol(_ vote: MobileChargingVoteDto) -> String {
        switch vote {
        case .up: "checkmark"
        case .down: "xmark"
        }
    }

    private var spotList: some View {
        List(model.spots, id: \.id) { spot in
            HStack {
                Button {
                    model.select(spot)
                    if let coordinate = spot.input?.coordinate {
                        pendingMapSelection = true
                        centerMap(on: Self.coordinate(coordinate), span: cameraSpan ?? defaultCameraSpan)
                        displayMode = .map
                    }
                } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(spot.input?.name ?? ChargingSpotPresentation.unknownName).font(.headline)
                        Text(spot.input?.accessNote ?? "").font(.subheadline)
                        Text(
                            spot.input.map {
                                localizedAppText($0.cost == .free ? "charging.cost.free" : "charging.cost.paid")
                            } ?? localizedAppText("charging.value.unknown")
                        )
                        .font(.caption).foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Button {
                    sheetRoute = .details(DetailItem(spot))
                } label: {
                    Image(systemName: "info.circle").frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityLabel(localizedAppText("charging.details.open"))
                .accessibilityIdentifier("charging.details.open")
            }
        }
        .listStyle(.plain)
    }

    @ViewBuilder private var status: some View {
        if let error = model.error ?? editorLaunchError {
            statusRow(ChargingSpotPresentation.loadErrorKey(error), retry: model.error != nil)
        } else if model.spots.isEmpty && !model.isLoading {
            Button(localizedAppText("charging.status.empty"), action: beginAddingAtCurrentLocation)
                .frame(minHeight: 44)
                .font(.footnote)
                .accessibilityIdentifier("charging.empty.add")
        } else if model.limitReached {
            Text(localizedAppText("charging.status.limit")).font(.footnote).padding(8)
        }
        if selectedSpotVisibility == .feedback, let spot = model.selectedSpot, let input = spot.input {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(localizedAppText("charging.spot.selected.label"))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(ChargingSpotPresentation.conflictName(input.name))
                        .font(.subheadline)
                        .lineLimit(2)
                }
                Spacer(minLength: 8)
                Button(localizedAppText("charging.details.button")) {
                    sheetRoute = .details(DetailItem(spot))
                }
                .frame(minHeight: 44)
                .accessibilityIdentifier("charging.saved-selection.details")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 4)
        }
        if !model.spots.isEmpty && model.hasMore && !model.limitReached {
            Button(localizedAppText("charging.load_more")) { Task { await model.loadMore() } }
                .frame(minHeight: 44).padding(.bottom, 4)
        }
        ChargingCommunityStatusView(model: model) {
            sheetRoute = .issues
        }
        if let error = model.voteError {
            Text(localizedAppText(ChargingSpotPresentation.voteErrorKey(error)))
                .font(.footnote).foregroundStyle(.secondary).padding(8)
        }
    }

    private func currentSpot(for item: DetailItem) -> MobileChargingSpotDto {
        if let spot = model.spots.first(where: { $0.id == item.id }) { return spot }
        if let spot = model.selectedSpot, spot.id == item.id { return spot }
        return item.fallback
    }

    private func statusRow(_ key: String, retry: Bool) -> some View {
        HStack {
            Text(localizedAppText(key))
            if retry { Button(localizedAppText("charging.retry")) { Task { await model.load() } } }
        }
        .font(.footnote)
        .padding(8)
        .accessibilityElement(children: .combine)
    }
}

private struct ChargingPinShape: Shape {
    let tailHeight: CGFloat

    func path(in rect: CGRect) -> Path {
        let bottom = rect.maxY - tailHeight
        let radius = min(rect.width / 2, bottom / 2)
        let center = rect.midX
        var path = Path()
        path.move(to: CGPoint(x: rect.minX + radius, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX - radius, y: rect.minY))
        path.addQuadCurve(
            to: CGPoint(x: rect.maxX, y: rect.minY + radius),
            control: CGPoint(x: rect.maxX, y: rect.minY)
        )
        path.addLine(to: CGPoint(x: rect.maxX, y: bottom - radius))
        path.addQuadCurve(
            to: CGPoint(x: rect.maxX - radius, y: bottom),
            control: CGPoint(x: rect.maxX, y: bottom)
        )
        path.addLine(to: CGPoint(x: center + 6, y: bottom))
        path.addQuadCurve(
            to: CGPoint(x: center, y: rect.maxY),
            control: CGPoint(x: center + 4, y: bottom + tailHeight * 0.65)
        )
        path.addQuadCurve(
            to: CGPoint(x: center - 6, y: bottom),
            control: CGPoint(x: center - 4, y: bottom + tailHeight * 0.65)
        )
        path.addLine(to: CGPoint(x: rect.minX + radius, y: bottom))
        path.addQuadCurve(
            to: CGPoint(x: rect.minX, y: bottom - radius),
            control: CGPoint(x: rect.minX, y: bottom)
        )
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radius))
        path.addQuadCurve(
            to: CGPoint(x: rect.minX + radius, y: rect.minY),
            control: CGPoint(x: rect.minX, y: rect.minY)
        )
        path.closeSubpath()
        return path
    }
}

/// The point stays at the selected coordinate while the map moves beneath it.
private struct ChargingPlacementPin: View {
    var body: some View {
        ZStack(alignment: .top) {
            ChargingPlacementPinShape()
                .fill(.yellow)
                .overlay { ChargingPlacementPinShape().stroke(.white, lineWidth: 3) }
            Image(systemName: "bolt.fill")
                .font(.system(size: 20, weight: .bold))
                .foregroundStyle(.black)
                .padding(.top, 11)
        }
        .frame(width: 44, height: 54)
        .shadow(color: .black.opacity(0.3), radius: 3, y: 2)
    }
}

private struct ChargingPlacementPinShape: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.midX, y: rect.maxY))
        path.addCurve(
            to: CGPoint(x: rect.minX, y: rect.minY + rect.width / 2),
            control1: CGPoint(x: rect.minX + rect.width * 0.2, y: rect.maxY * 0.7),
            control2: CGPoint(x: rect.minX, y: rect.maxY * 0.6))
        path.addArc(
            center: CGPoint(x: rect.midX, y: rect.minY + rect.width / 2),
            radius: rect.width / 2, startAngle: .degrees(180), endAngle: .degrees(0), clockwise: false)
        path.addCurve(
            to: CGPoint(x: rect.midX, y: rect.maxY),
            control1: CGPoint(x: rect.maxX, y: rect.maxY * 0.6),
            control2: CGPoint(x: rect.maxX - rect.width * 0.2, y: rect.maxY * 0.7))
        path.closeSubpath()
        return path
    }
}
