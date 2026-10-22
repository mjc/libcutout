import Accessibility
import CutoutMobile
import CutoutMobileFFI
import SwiftUI

struct VescRideScreenView: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
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
        .sheet(item: $drivetrainEditor) { editor in
            VescDrivetrainEditorView(editor: editor)
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
                if let disconnect {
                    Button(action: disconnect) {
                        Image(systemName: "xmark")
                            .font(.body.weight(.semibold))
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
            if usesCompactPhoneInstruments {
                GeometryReader { geometry in
                    HStack(spacing: 8) {
                        VStack(spacing: 0) {
                            PevRideSpeedReading(
                                speed: presentation.speedReadout,
                                caption: localizedAppText("vesc.speed.caption"),
                                pointSize: min(104, geometry.size.width * 0.30, geometry.size.height * 0.75),
                                muted: PevDashboardColors.rideMuted
                            )
                            drivetrainRecoveryAccessory
                        }
                        .frame(width: max(0, geometry.size.width * 0.64 - 4))
                        standardEnergyReading(
                            pointSize: min(36, geometry.size.width * 0.105, geometry.size.height * 0.50)
                        )
                        .frame(maxWidth: .infinity)
                    }
                    .frame(maxHeight: .infinity)
                }
            } else {
                PevRidePrimaryInstruments(
                    speed: presentation.speedReadout,
                    caption: localizedAppText("vesc.speed.caption"), landscape: landscape,
                    speedFraction: isEskate ? 0.48 : 0.42,
                    muted: isEskate ? PevDashboardColors.rideMuted : PevColors.muted,
                    speedAccessory: {
                        drivetrainRecoveryAccessory
                    }
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

    @ViewBuilder
    private var drivetrainRecoveryAccessory: some View {
        if phase == .live, !presentation.speedReadout.isAvailable, let makeDrivetrainEditor {
            Button(localizedAppText("vesc.drivetrain.set")) {
                drivetrainEditor = makeDrivetrainEditor()
            }
            .font(usesCompactPhoneInstruments ? .caption.weight(.semibold) : .subheadline.weight(.semibold))
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
            Group {
                if isEskate, tile.kind == .batteryLevel,
                    let level = liveSnapshot?.batteryLevelReported ?? liveSnapshot?.batteryLevelEstimated
                {
                    PevRideReserveGauge(
                        label: tile.label, valueText: tile.metricValue.displayText, unit: tile.unit,
                        progress: Double(level.value) / 100,
                        accent: PevDashboardColors.rideInk,
                        track: PevDashboardColors.rideLine,
                        ink: PevDashboardColors.rideInk,
                        muted: PevDashboardColors.rideMuted,
                        showsScale: false, compact: usesCompactPhoneInstruments,
                        valueScale: usesCompactPhoneInstruments ? 0.35 : 0.30
                    )
                    .frame(maxWidth: usesCompactPhoneInstruments ? 140 : 292)
                    .frame(height: usesCompactPhoneInstruments ? 112 : 220)
                } else {
                    PevInstrumentReading(
                        label: tile.label, value: tile.metricValue, unit: tile.unit,
                        pointSize: pointSize, identifier: "vesc.ride.energy", alignment: .center,
                        detail: tile.detail,
                        ink: isEskate ? PevDashboardColors.rideInk : PevColors.primaryText,
                        muted: isEskate ? PevDashboardColors.rideMuted : PevColors.muted
                    )
                    .visualContent
                }
            }
            .frame(maxWidth: .infinity, minHeight: 44)
            .padding(.vertical, usesCompactPhoneInstruments ? 4 : 14)
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
