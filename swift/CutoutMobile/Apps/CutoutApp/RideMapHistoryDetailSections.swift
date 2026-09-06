import CutoutMobile
import CutoutMobileFFI
import MapKit
import SwiftUI

extension MobileRideMapError {
    /// Keeps storage details out of the spoken music-history failure state.
    var musicHistoryAccessibilityText: String {
        localizedAppText("music.history.unavailable")
    }
}

private extension MobileMusicHistoryStateDto {
    var detailTitle: String {
        switch self {
        case .missing:
            localizedAppText("music.history.state.missing")
        case .disabled:
            localizedAppText("music.history.state.disabled")
        case .redacted:
            localizedAppText("music.history.state.redacted")
        case .humanReadable:
            localizedAppText("music.history.state.human_readable")
        case .deleted:
            localizedAppText("music.history.state.deleted")
        }
    }

    var detailSymbol: String {
        switch self {
        case .missing: "minus.circle"
        case .disabled: "nosign"
        case .redacted: "eye.slash"
        case .humanReadable: "music.note"
        case .deleted: "trash"
        }
    }

    func showsDetailStatus(timelineIsEmpty: Bool) -> Bool {
        self != .humanReadable || timelineIsEmpty
    }
}

struct RideMapHistoryDetailHeader: View {
    let close: () -> Void

    var body: some View {
        Text(localizedAppText("ride_map.detail_title"))
            .font(.headline.weight(.semibold))
            .lineLimit(2)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity, minHeight: 44)
            .padding(.horizontal, 72)
            .overlay(alignment: .leading) {
                Button(action: close) {
                    Label(localizedAppText("ride_map.detail_back"), systemImage: "chevron.left")
                        .labelStyle(.titleAndIcon)
                }
                .buttonStyle(.plain)
                .foregroundStyle(PevColors.yellow)
                .frame(minWidth: 44, minHeight: 44)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .background(PevColors.pageBackground)
            .accessibilityIdentifier("ride-map.detail-header")
    }
}

struct RideMapHistoryDetailMap: View {
    let points: [MobileRideMapRouteDisplayPoint]
    let routeID: String
    let projectionVersion: UInt64
    let endpointMetadata: MobileRideMapRouteEndpointMetadata
    let cameraRegion: MobileRideMapCameraRegion?
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let state: RideMapHistoryRouteState
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    let cameraDidChange: (MKCoordinateRegion) -> Void

    var body: some View {
        ZStack {
            RideMapCanvasView(
                points: points,
                routeID: routeID,
                projectionVersion: projectionVersion,
                showsStartMarker: true,
                showsEndMarker: true,
                showsCurrentMarker: false,
                endpointMetadata: endpointMetadata,
                cameraRegion: cameraRegion,
                segments: segments,
                contextRoutes: [],
                fitsRouteOnChange: true,
                mapPosition: $mapPosition,
                isApplyingCamera: $isApplyingCamera,
                cameraDidChange: cameraDidChange
            )

            if state == .loading {
                HStack(spacing: 8) {
                    ProgressView()
                        .tint(PevColors.yellow)
                    Text(localizedAppText("ride_map.history_loading"))
                        .font(.subheadline.weight(.semibold))
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 10)
                .modifier(RideMapLoadingSurface())
                .accessibilityIdentifier("ride-map.detail-loading")
            } else if state == .error {
                ContentUnavailableView(
                    localizedAppText("ride_map.detail_error_title"),
                    systemImage: "exclamationmark.triangle"
                )
                .accessibilityIdentifier("ride-map.detail-map-error")
            } else if state == .empty {
                ContentUnavailableView(
                    localizedAppText("ride_map.no_points"),
                    systemImage: "location.slash"
                )
                .accessibilityIdentifier("ride-map.detail-no-points")
            }
        }
    }
}

struct RideMapHistoryDetailUnavailableState: View {
    let hasError: Bool
    let retry: () -> Void

    var body: some View {
        VStack(spacing: 12) {
            ContentUnavailableView(
                hasError
                    ? localizedAppText("ride_map.detail_error_title")
                    : localizedAppText("ride_map.history_empty"),
                systemImage: hasError ? "exclamationmark.triangle" : "map"
            )
            if hasError {
                Button(localizedAppText("ride_map.history_retry"), action: retry)
                    .buttonStyle(.borderedProminent)
                    .tint(PevColors.yellow)
                    .accessibilityIdentifier("ride-map.detail-retry")
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct RideMapHistoryDetailSummary: View {
    let distance: String
    let duration: String
    let averageSpeed: String
    let recordedAt: String
    let vehicle: String
    let telemetryState: MobileRideMapTelemetryStateDto
    let displayPointCount: Int
    let recordedPointCount: UInt64
    let pointsTruncated: Bool
    let segmentCount: UInt64
    let segments: [MobileRideMapSegmentDisplayMetadata]
    let segmentsOmittedByBudget: Bool
    let canonicalBackgroundGapCount: UInt64
    let musicTimeline: [MobileMusicRideEventDto]
    let musicHistoryState: MobileMusicHistoryStateDto?
    let musicError: MobileRideMapError?
    let forgetMusicHistory: () -> Bool
    let state: RideMapHistoryRouteState
    let loadRoutePreview: () -> Void
    let shareText: String
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    @State private var isMusicHistoryForgetConfirmationPresented = false
    @State private var isMusicHistoryForgetErrorPresented = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if state == .loading {
                HStack(spacing: 8) {
                    ProgressView()
                        .tint(PevColors.yellow)
                    Text(localizedAppText("ride_map.history_loading"))
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(PevColors.muted)
                }
                .accessibilityIdentifier("ride-map.detail-summary-loading")
            } else {
                if state == .error {
                    Label(
                        localizedAppText("ride_map.command_failed"),
                        systemImage: "exclamationmark.triangle"
                    )
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .accessibilityIdentifier("ride-map.detail-error")
                }
                VStack(alignment: .leading, spacing: 3) {
                    Text(recordedAt)
                        .font(.subheadline.weight(.semibold))
                    Text(vehicle)
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                }
                .accessibilityElement(children: .combine)
                HStack(alignment: .firstTextBaseline, spacing: 0) {
                    RideMapDetailMetric(
                        value: distance,
                        label: localizedAppText("ride_map.metric_distance")
                    )
                    RideMapDetailMetric(
                        value: duration,
                        label: localizedAppText("ride_map.metric_elapsed")
                    )
                    RideMapDetailMetric(
                        value: averageSpeed,
                        label: localizedAppText("ride_map.metric_average_speed")
                    )
                }
                RideMapRouteTruthView(
                    displayedPointCount: displayPointCount,
                    recordedPointCount: recordedPointCount,
                    rustSegmentCount: segmentCount,
                    decision: nil,
                    showsRecordedBounds: !pointsTruncated,
                    segmentsOmittedByBudget: segmentsOmittedByBudget,
                    segments: segments,
                    canonicalBackgroundGapCount: canonicalBackgroundGapCount,
                    hasRoute: RideMapRouteTruthView.routeExists(
                        recordedPointCount: recordedPointCount,
                        displayedPointCount: displayPointCount
                    ),
                    telemetryState: telemetryState
                )
                if let musicError {
                    Label(
                        localizedAppText("music.history.unavailable"),
                        systemImage: "exclamationmark.triangle"
                    )
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .accessibilityValue(Text(musicError.musicHistoryAccessibilityText))
                    .accessibilityIdentifier("ride-map.detail-music-history-unavailable")
                } else {
                    if let musicHistoryState,
                       musicHistoryState.showsDetailStatus(timelineIsEmpty: musicTimeline.isEmpty)
                    {
                        Label(
                            musicHistoryState.detailTitle,
                            systemImage: musicHistoryState.detailSymbol
                        )
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("ride-map.detail-music-history-state")
                    }
                    if musicTimeline.isEmpty == false {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack(alignment: .firstTextBaseline) {
                                Text(localizedAppText("music.timeline.title"))
                                    .font(.headline.weight(.semibold))
                                Spacer()
                                Button(localizedAppText("music.history.forget"), role: .destructive) {
                                    isMusicHistoryForgetConfirmationPresented = true
                                }
                                .font(.caption.weight(.semibold))
                                .accessibilityIdentifier("ride-map.detail-forget-music-history")
                            }
                            MusicTimelineRows(events: musicTimeline)
                        }
                        .accessibilityIdentifier("ride-map.detail-music-timeline")
                        .confirmationDialog(
                            localizedAppText("music.history.forget.title"),
                            isPresented: $isMusicHistoryForgetConfirmationPresented,
                            titleVisibility: .visible
                        ) {
                            Button(localizedAppText("music.history.forget"), role: .destructive) {
                                if forgetMusicHistory() == false {
                                    isMusicHistoryForgetErrorPresented = true
                                }
                            }
                            Button(localizedAppText("common.cancel"), role: .cancel) {}
                        } message: {
                            Text(localizedAppText("music.history.forget.message"))
                        }
                        .alert(
                            localizedAppText("music.history.forget.error"),
                            isPresented: $isMusicHistoryForgetErrorPresented
                        ) {
                            Button(localizedAppText("common.cancel"), role: .cancel) {}
                        }
                    }
                }
                if pointsTruncated {
                    Text(localizedAppText("ride_map.history_truncated_count", displayPointCount))
                        .font(.caption)
                        .foregroundStyle(PevColors.muted)
                        .accessibilityIdentifier("ride-map.detail-truncated")
                }

                HStack(spacing: 10) {
                    if pointsTruncated || state == .error {
                        Button(
                            state == .error
                                ? localizedAppText("ride_map.history_retry")
                                : localizedAppText("ride_map.show_route_preview"),
                            action: showRoutePreview
                        )
                        .buttonStyle(.bordered)
                        .tint(PevColors.primaryText)
                    }

                    ShareLink(item: shareText) {
                        Label(
                            localizedAppText("ride_map.share"), systemImage: "square.and.arrow.up")
                    }
                    .buttonStyle(.bordered)
                    .tint(PevColors.primaryText)
                }
            }
        }
        .font(.subheadline)
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            PevColors.cardFill,
            in: UnevenRoundedRectangle(
                topLeadingRadius: 28,
                bottomLeadingRadius: 0,
                bottomTrailingRadius: 0,
                topTrailingRadius: 28
            )
        )
        .padding(.bottom, 8)
    }

    private func showRoutePreview() {
        isApplyingCamera = true
        loadRoutePreview()
        mapPosition = .automatic
    }
}

struct RideMapDetailMetric: View {
    let value: String
    let label: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(value)
                .font(.title2.weight(.bold).monospacedDigit())
                .lineLimit(1)
                .minimumScaleFactor(0.72)
            Text(label)
                .font(.caption)
                .foregroundStyle(PevColors.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
