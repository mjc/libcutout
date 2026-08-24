import Accessibility
import CutoutMobile
import Foundation
import SwiftUI
#if os(iOS)
import UIKit
#endif

struct ContentView: View {
    let model: CutoutAppModel
    @Binding private var navigationPath: [CutoutAppRoute]
    @AccessibilityFocusState private var focusedRoute: CutoutAppRoute?
    @State private var connectionAnnouncements = ConnectionAccessibilityAnnouncements()

    init(model: CutoutAppModel, navigationPath: Binding<[CutoutAppRoute]>) {
        self.model = model
        _navigationPath = navigationPath
    }

    private var route: CutoutAppRoute {
        navigationPath.last ?? .devicePicker
    }

    var body: some View {
        NavigationStack(path: $navigationPath) {
            ZStack {
                PevColors.pageBackground
                    .ignoresSafeArea()

                DevicePickerRouteView(model: model, pair: pair, navigate: navigate)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .accessibilityLabel(localizedAppText("picker.title"))
                .accessibilityFocused($focusedRoute, equals: .devicePicker)
            }
            .navigationDestination(for: CutoutAppRoute.self) { destination in
                destinationContent(for: destination)
                    .navigationBarBackButtonHidden(true)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(PevColors.pageBackground.ignoresSafeArea())
        .onChange(of: route, initial: true) { _, route in
            focusedRoute = route
        }
        .onChange(of: model.captureStatus) { _, status in
            if let announcement = status?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
        .onChange(of: model.phase) { _, phase in
            if let announcement = connectionAnnouncements.next(for: phase) {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
        .onChange(of: model.connectionState) { _, state in
            if let announcement = connectionAnnouncements.next(for: state) {
                AccessibilityNotification.Announcement(announcement).post()
            }
            switch state.navigationIntent(isRecordOnlyCapture: model.isRecordOnlyCapture) {
            case .returnToPicker:
                navigate(to: .devicePicker)
            case let .openRide(connectionRoute) where route == .devicePicker:
                navigate(to: CutoutAppRoute.route(for: connectionRoute))
            case .stay, .openRide:
                break
            }
        }
        .onChange(of: model.devicePickerScanState?.status) { _, _ in
            guard let scanState = model.devicePickerScanState,
                  let announcement = connectionAnnouncements.next(for: scanState) else {
                return
            }
            AccessibilityNotification.Announcement(announcement).post()
        }
        .onChange(of: model.bmsSnapshot?.accessibilityAlertLevel) { _, level in
            if let announcement = level?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
        .onChange(of: model.liveActivityError) { _, error in
            if let error {
                AccessibilityNotification.Announcement(error.accessibilityAnnouncement).post()
            }
        }
    }

    private func pair(_ row: DevicePickerRow) {
        guard row.isSupported else { return }

        connectionAnnouncements.beginUserInitiatedAttempt()
        guard model.pair(platformIdentifier: row.id) else { return }
    }

    private func selectTarget(_ target: PevNavigationTarget) {
        navigate(to: CutoutAppRoute.route(forNavigationTarget: target))
    }

    private func navigate(to route: CutoutAppRoute) {
        navigationPath = CutoutAppRoute.navigationPath(for: route)
    }

    private func disconnectAndReturnToPicker() {
        model.disconnectTransport()
        navigate(to: .devicePicker)
    }

    private func finishCaptureAndReturnToPicker() {
        Task { @MainActor in
            guard await model.finishCapture() else { return }
            navigate(to: .devicePicker)
        }
    }

    @ViewBuilder
    private func destinationContent(for destination: CutoutAppRoute) -> some View {
        if destination == .capture || isRideMapDetail(destination) || (destination == .rideMap && model.selectedConnectionRoute == nil) {
            ZStack {
                PevColors.pageBackground
                    .ignoresSafeArea()
            routedContent(for: destination)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .accessibilityFocused($focusedRoute, equals: destination)
        } else {
            let tabs = TabView(selection: tabSelection) {
                ForEach(availableTabs) { tab in
                    if let tabRoute = destination.destination(for: tab) {
                        Tab(value: tab.id) {
                            connectedDestination(for: tabRoute)
                        } label: {
                            Label(tab.title, systemImage: tab.id.systemImage)
                        }
                        .accessibilityIdentifier(tab.accessibilityIdentifier)
                    }
                }
            }
            .tint(tabAccent)
#if os(iOS)
            tabs
                .toolbarBackground(PevColors.pageBackground, for: .tabBar)
                .toolbarBackground(.visible, for: .tabBar)
#else
            tabs
#endif
        }
    }

    private func isRideMapDetail(_ destination: CutoutAppRoute) -> Bool {
        if case .rideMapDetail = destination { return true }
        return false
    }

    private func isRideMapDestination(_ destination: CutoutAppRoute) -> Bool {
        destination == .rideMap || isRideMapDetail(destination)
    }

    private func connectedDestination(for destination: CutoutAppRoute) -> some View {
        ZStack {
            PevColors.pageBackground
                .ignoresSafeArea()

            Group {
                if isRideMapDestination(destination) {
                    // The map is a product-category-neutral destination. It owns its
                    // CutOut/Map chrome instead of inheriting the vehicle dashboard shell.
                    routedContent(for: destination)
                } else {
                    PevAppShell(
                        sectionTitle: appSectionTitle(for: destination),
                        disconnect: disconnectAndReturnToPicker
                    ) {
                        routedContent(for: destination)
                    }
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel(appSectionTitle(for: destination))
            .accessibilityFocused($focusedRoute, equals: destination)
        }
    }

    @ViewBuilder
    private func routedContent(for destination: CutoutAppRoute) -> some View {
        switch destination {
        case .eucRide:
            EucRideRouteView(model: model)
        case .eucPack(let packScreen):
            EucPackRouteView(
                model: model,
                packScreen: packScreen,
                selectedGroupIndex: destination.selectedBmsGroupIndex,
                navigate: navigate
            )
        case .vescRide:
            VescRideRouteView(model: model)
        case .vescDebug:
            VescDebugRouteView(model: model)
        case .capture:
            CaptureRouteView(model: model, finishCapture: finishCaptureAndReturnToPicker)
        case .rideMap:
            RideMapRouteView(model: model, { rideID in
                model.rideMapMode = .history
                navigate(to: .rideMapDetail(rideID: rideID))
            }, showBackButton: model.selectedConnectionRoute == nil, back: { navigate(to: .devicePicker) })
        case let .rideMapDetail(rideID):
            RideMapRouteView(
                model: model,
                initialHistoryID: rideID,
                detailOnly: true,
                closeDetail: { navigate(to: .rideMap) }
            )
        case .devicePicker:
            EmptyView()
        }
    }

    private func appSectionTitle(for destination: CutoutAppRoute) -> String {
        switch destination {
        case .eucRide, .vescRide:
            localizedAppText("navigation.section.ride")
        case .eucPack:
            localizedAppText("navigation.section.pack")
        case .vescDebug:
            localizedAppText("navigation.section.debug")
        case .capture:
            localizedAppText("navigation.section.capture")
        case .rideMap, .rideMapDetail:
            localizedAppText("navigation.section.map")
        case .devicePicker:
            localizedAppText("navigation.section.ride")
        }
    }

    private var availableTabs: [PevScreenTab] {
        route.availableNavigationTabs(for: model.selectedConnectionRoute)
    }

    private var tabSelection: Binding<PevScreenTabID> {
        Binding(
            get: {
                availableTabs.first(where: \.isSelected)?.id ?? .ride
            },
            set: { selectedID in
                guard selectedID != availableTabs.first(where: \.isSelected)?.id else { return }
                guard let target = availableTabs.first(where: { $0.id == selectedID })?.destinationTarget else {
                    return
                }
                selectTarget(target)
            }
        )
    }

    private var tabAccent: Color {
        #if os(iOS)
        switch model.selectedConnectionRoute {
        case .vescOnewheel:
            .purple
        default:
            .yellow
        }
        #else
        .primary
        #endif
    }

}

private extension PevScreenTabID {
    var systemImage: String {
        switch self {
        case .ride:
            "speedometer"
        case .pack:
            "battery.100percent"
        case .debug:
            "wrench.and.screwdriver"
        case .map:
            "map"
        case .tune:
            "slider.horizontal.3"
        case .logs:
            "doc.text"
        }
    }
}
