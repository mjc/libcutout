import Accessibility
import CutoutMobile
import CutoutMobileFFI
import SwiftUI

struct RidePhoneSettingsGearButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "gearshape")
                .font(.system(size: 20, weight: .semibold))
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(localizedAppText("phone_control.settings"))
        .accessibilityIdentifier("ride.phone-control.settings")
    }
}

struct VescRideScreenView: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    @ScaledMetric(relativeTo: .headline) private var headroomLineHeight: CGFloat = 21
    @ScaledMetric(relativeTo: .caption) private var footerHeight: CGFloat = 20
    let liveSnapshot: VescRideSnapshot?
    let phase: SessionConnectionPhase
    let now: MonotonicMilliseconds
    let captureStatusText: String?
    let connectionStatusText: String?
    var disconnect: (() -> Void)? = nil
    var makeDrivetrainEditor: (() -> VescDrivetrainEditor?)? = nil
    var phoneDrive: PhoneDriveModel? = nil
    @State private var showsStandardReadings = false
    @State private var showsPhoneDriveSettings = false
    @State private var drivetrainEditor: VescDrivetrainEditor?

    private var presentation: VescRideScreenPresentation {
        VescRideScreenPresentation(
            snapshot: liveSnapshot,
            phase: phase,
            now: now,
            connectionStatusText: connectionStatusText
        )
    }

    var dashboardTiles: [PevDashboardTile] {
        guard liveSnapshot != nil else {
            return VescRideScreenPresentation(
                snapshot: VescRideSnapshot(
                    title: presentation.title, vehicleKind: .unknown,
                    application: .absent, controllerState: .unknown),
                phase: phase, now: now, connectionStatusText: connectionStatusText
            ).dashboardTiles
        }
        return presentation.dashboardTiles
    }

    var body: some View {
        Group {
            if presentation.usesStandardInstruments {
                if let phoneDrive, liveSnapshot?.vehicleKind == .eskate {
                    PhoneDriveRideSurface(model: phoneDrive) { standardInstruments }
                } else {
                    standardInstruments
                }
            } else {
                refloatInstruments
            }
        }
        .accessibilityElement(children: .contain)
        .sheet(
            isPresented: $showsPhoneDriveSettings,
            onDismiss: {
                guard let phoneDrive, let request = phoneDrive.dismissSettings() else { return }
                guard !phoneDrive.activateAfterSettingsDismissal(request) else { return }
                guard phoneDrive.presentSettings(for: request.token, activationFailed: true) else { return }
                showsPhoneDriveSettings = true
            }
        ) {
            if let phoneDrive {
                PhoneDriveSettingsSheet(model: phoneDrive) { request in
                    guard phoneDrive.claimActivationDismissal(request) else { return }
                    showsPhoneDriveSettings = false
                }
            }
        }
        .sheet(item: $drivetrainEditor) { editor in
            VescDrivetrainEditorView(editor: editor)
        }
        .onChange(of: phoneDrive?.navigationSettingsRequest) { _, request in
            if let request, request > 0 { showsPhoneDriveSettings = true }
        }
        .onChange(of: showsStandardReadings) { _, _ in phoneDrive?.cancel(.navigation) }
        .onChange(of: drivetrainEditor != nil) { _, _ in phoneDrive?.cancel(.navigation) }
        .onChange(of: liveSnapshot?.warning) { _, warning in
            if let announcement = warning?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
        .onChange(of: liveSnapshot?.stopReason) { _, reason in
            guard liveSnapshot?.warning == .some(.none) else { return }
            if let announcement = reason?.accessibilityAnnouncement {
                AccessibilityNotification.Announcement(announcement).post()
            }
        }
    }

    private var refloatInstruments: some View {
        PevRideDashboardShell(
            heroStyle: .vesc,
            title: presentation.title,
            subtitle: presentation.subtitle,
            statusTone: presentation.statusTone,
            captureStatusText: captureStatusText,
            speedReadout: presentation.speedReadout,
            speedCaption: localizedAppText("vesc.speed.caption")
        ) {
            if dynamicTypeSize.isAccessibilitySize {
                PevRideAccessibleReadings(
                    headroomLabel: localizedAppText("vesc.duty_headroom.label"),
                    headroom: liveSnapshot?.dutyHeadroomMetricValue ?? .unavailable,
                    headroomProgress: liveSnapshot?.dutyHeadroomProgress,
                    tiles: dashboardTiles,
                    footpadText: liveSnapshot?.footpad?.stateDisplayText
                )
            } else {
                GeometryReader { geometry in
                    VStack(spacing: 8) {
                        PevDashboardProgressBar(
                            label: localizedAppText("vesc.duty_headroom.label"),
                            metricValue: liveSnapshot?.dutyHeadroomMetricValue ?? .unavailable,
                            progress: liveSnapshot?.dutyHeadroomProgress,
                            height: 10
                        )
                        .fixedSize(horizontal: false, vertical: true)
                        PevDashboardGrid(
                            columns: [GridItem(.flexible(), spacing: 8), GridItem(.flexible(), spacing: 8)],
                            spacing: 8
                        ) {
                            ForEach(dashboardTiles) { tile in
                                PevDashboardMetricTile(
                                    label: tile.kind == .motorCurrent
                                        ? localizedAppText("vesc.metric.motor_current.compact") : tile.label,
                                    metricValue: tile.metricValue, unit: tile.unit,
                                    prominence: .ride
                                )
                                .accessibilityLabel(tile.label)
                                .frame(
                                    height: min(
                                        108,
                                        max(0, (geometry.size.height - headroomLineHeight - 17 - footerHeight - 24) / 2)
                                    ))
                            }
                        }
                        Group {
                            if let footpad = liveSnapshot?.footpad {
                                Text(footpad.stateDisplayText)
                                    .font(.caption.weight(.semibold))
                                    .lineLimit(1)
                                    .minimumScaleFactor(0.6)
                            } else {
                                Color.clear
                                    .accessibilityHidden(true)
                            }
                        }
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(minHeight: 20)
                    }
                }
            }
        }
    }

    private var standardInstruments: some View {
        PevRideInstrumentLayout(
            spacing: usesCompactPhoneInstruments ? 8 : 16,
            bottomPadding: isEskate ? 8 : 28,
            ink: isEskate ? PevDashboardColors.rideInk : PevColors.primaryText,
            canvas: isEskate ? PevDashboardColors.rideCanvas : PevColors.pageBackground,
            landscapeOverride: usesCompactPhoneInstruments && horizontalSizeClass == .compact
                ? verticalSizeClass == .compact : nil
        ) {
            HStack(spacing: 8) {
                PevRideStatusHeader(
                    title: presentation.title, subtitle: presentation.subtitle,
                    statusTone: presentation.statusTone, captureStatusText: captureStatusText,
                    quietStatus: true
                )
                .lineLimit(isEskate ? 1 : nil)
                .minimumScaleFactor(isEskate ? 0.6 : 1)
                if let disconnect {
                    Button(action: disconnect) {
                        Image(systemName: "xmark")
                            .font(isEskate ? .system(size: 20, weight: .semibold) : .body.weight(.semibold))
                            .frame(width: 44, height: 44)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(localizedAppText("ride.action.disconnect"))
                    .accessibilityIdentifier("dashboard.disconnect")
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("dashboard.top.navigation")
        } primary: { landscape in
            if isEskate {
                eskatePrimaryInstruments(landscape: landscape)
            } else {
                PevRidePrimaryInstruments(
                    speed: presentation.speedReadout,
                    caption: localizedAppText("vesc.speed.caption"), landscape: landscape,
                    muted: PevColors.muted,
                    speedAccessory: { drivetrainRecoveryAccessory }
                ) { size in
                    standardEnergyReading(pointSize: min(76, size.width * 0.23, size.height * 0.45))
                }
            }
        } summary: {
            standardSummaryReadings
        } secondary: {
            // At accessibility sizes the scrollable Readings sheet retains every secondary
            // measurement while the fixed Ride surface keeps room for speed and energy.
            if !dynamicTypeSize.isAccessibilitySize {
                standardSecondaryReadings
            }
        }
        .sheet(isPresented: $showsStandardReadings) {
            PevRideReadingsDetail(tiles: dashboardTiles, footpadText: nil)
        }
    }

    private var usesCompactPhoneInstruments: Bool {
        isEskate && phoneDrive?.snapshot.mode == .buttons
    }

    private var isEskate: Bool { liveSnapshot?.vehicleKind == .eskate }

    private func eskatePrimaryInstruments(landscape: Bool) -> some View {
        GeometryReader { geometry in
            let batteryReserve: CGFloat = 72
            let speedBudget = max(48, geometry.size.height - batteryReserve)
            let pointSize: CGFloat =
                if dynamicTypeSize.isAccessibilitySize {
                    min(64, geometry.size.width * 0.26, speedBudget * 0.45)
                } else if landscape {
                    min(112, geometry.size.width * 0.34, speedBudget * 0.62)
                } else {
                    min(164, geometry.size.width * 0.40, speedBudget * 0.72)
                }
            VStack(spacing: 8) {
                VStack(spacing: 0) {
                    PevRideSpeedReading(
                        speed: presentation.speedReadout,
                        caption: localizedAppText("vesc.speed.caption"),
                        pointSize: pointSize,
                        muted: PevDashboardColors.rideMuted
                    )
                    drivetrainRecoveryAccessory
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                eskateBatteryReading
                Spacer(minLength: 0)
            }
            .frame(width: geometry.size.width, height: geometry.size.height, alignment: .top)
        }
    }

    private var eskateBatteryReading: some View {
        let tile = presentation.standardEnergyTile
        let level = liveSnapshot?.batteryLevelReported ?? liveSnapshot?.batteryLevelEstimated
        return Button {
            showsStandardReadings = true
        } label: {
            VStack(alignment: .leading, spacing: 6) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(tile.label)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(PevDashboardColors.rideMuted)
                        .layoutPriority(1)
                    Spacer(minLength: 8)
                    HStack(alignment: .firstTextBaseline, spacing: 2) {
                        Text(tile.metricValue.displayText)
                            .font(.title3.weight(.semibold))
                            .monospacedDigit()
                        if tile.metricValue != .unavailable {
                            Text(tile.unit)
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(PevDashboardColors.rideMuted)
                        }
                    }
                    .fixedSize(horizontal: true, vertical: false)
                }
                Capsule()
                    .fill(PevDashboardColors.rideLine)
                    .frame(height: 4)
                    .overlay(alignment: .leading) {
                        if let level {
                            GeometryReader { proxy in
                                Capsule()
                                    .fill(
                                        colorScheme == .dark
                                            ? PevDashboardColors.rideAccent : PevDashboardColors.rideInk
                                    )
                                    .frame(
                                        width: proxy.size.width * max(0, min(1, Double(level.value) / 100))
                                    )
                            }
                        }
                    }
                    .accessibilityHidden(true)
            }
            .frame(maxWidth: .infinity, minHeight: 44, alignment: .center)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(tile.label)
        .accessibilityValue(tile.metricValue.accessibilityValue(unit: tile.unit, detail: tile.detail))
        .accessibilityHint(localizedAppText("ride.readings.open"))
        .accessibilityIdentifier("ride.readings.open")
    }

    @ViewBuilder
    private var drivetrainRecoveryAccessory: some View {
        if phase == .live, !presentation.speedReadout.isAvailable, let makeDrivetrainEditor {
            Button(localizedAppText("vesc.drivetrain.set")) {
                drivetrainEditor = makeDrivetrainEditor()
            }
            .font(usesCompactPhoneInstruments ? .caption.weight(.semibold) : .subheadline.weight(.semibold))
            .lineLimit(isEskate ? 1 : nil)
            .minimumScaleFactor(isEskate ? 0.5 : 1)
            .padding(.horizontal, 14)
            .frame(minHeight: 44)
            .background(isEskate ? PevDashboardColors.rideLine.opacity(0.4) : PevColors.cardFill, in: Capsule())
            .buttonStyle(.plain)
            .accessibilityIdentifier("vesc.drivetrain.open")
        }
    }

    private func standardEnergyReading(pointSize: CGFloat) -> some View {
        let tile = presentation.standardEnergyTile
        return Button {
            showsStandardReadings = true
        } label: {
            PevInstrumentReading(
                label: tile.label, value: tile.metricValue, unit: tile.unit,
                pointSize: dynamicTypeSize.isAccessibilitySize ? min(52, pointSize) : pointSize,
                identifier: "vesc.ride.energy", alignment: .center,
                detail: tile.detail, ink: PevColors.primaryText, muted: PevColors.muted
            )
            .visualContent
            .frame(maxWidth: .infinity, minHeight: 44)
            .padding(.vertical, 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(tile.label)
        .accessibilityValue(tile.metricValue.accessibilityValue(unit: tile.unit, detail: tile.detail))
        .accessibilityHint(localizedAppText("ride.readings.open"))
        .accessibilityIdentifier("ride.readings.open")
    }

    private var standardSummaryReadings: some View {
        HStack(alignment: .top, spacing: 24) {
            ForEach(Array(presentation.standardSummaryTiles.enumerated()), id: \.element.kind) { index, tile in
                PevInstrumentReading(
                    label: tile.label, value: tile.metricValue, unit: tile.unit,
                    pointSize: usesCompactPhoneInstruments ? 34 : 52, identifier: "vesc.ride.\(tile.kind)",
                    alignment: index == 0 ? .leading : .trailing, detail: tile.detail,
                    ink: isEskate ? PevDashboardColors.rideInk : PevColors.primaryText,
                    muted: isEskate ? PevDashboardColors.rideMuted : PevColors.muted
                )
                .frame(maxWidth: .infinity, alignment: index == 0 ? .leading : .trailing)
            }
        }
        .padding(.vertical, usesCompactPhoneInstruments ? 8 : 18)
        .frame(minHeight: dynamicTypeSize.isAccessibilitySize ? nil : usesCompactPhoneInstruments ? 72 : 138)
        .overlay(alignment: .top) { Rectangle().fill(instrumentLine).frame(height: 1) }
        .overlay(alignment: .bottom) { Rectangle().fill(instrumentLine).frame(height: 1) }
    }

    private var standardSecondaryReadings: some View {
        HStack(alignment: .top, spacing: 12) {
            ForEach(Array(presentation.standardSecondaryTiles.enumerated()), id: \.element.kind) { index, tile in
                PevInstrumentReading(
                    label: tile.label, value: tile.metricValue, unit: tile.unit,
                    pointSize: usesCompactPhoneInstruments ? 18 : 23, identifier: "vesc.ride.\(tile.kind)",
                    alignment: index == 0 ? .leading : index == 1 ? .center : .trailing,
                    detail: tile.detail,
                    ink: isEskate ? PevDashboardColors.rideInk : PevColors.primaryText,
                    muted: isEskate ? PevDashboardColors.rideMuted : PevColors.muted
                )
                .frame(maxWidth: .infinity, alignment: index == 0 ? .leading : index == 1 ? .center : .trailing)
            }
        }
        .frame(minHeight: usesCompactPhoneInstruments ? 42 : 64, alignment: .top)
    }

    private var instrumentLine: Color { isEskate ? PevDashboardColors.rideLine : PevColors.cardStroke }
}
