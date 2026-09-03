import CoreLocation
import CutoutMobile
import CutoutMobileFFI
import MapKit
import SwiftUI

struct RideMapCanvasView: View {
    private struct SegmentPath: Identifiable {
        let id: UInt64
        let startReason: MobileRideMapSegmentStartReason
        let singletonAccessibilityLabel: String?
        var coordinates: [CLLocationCoordinate2D]

        var isGap: Bool {
            startReason.isBackgroundGap
        }
    }

    private struct ContextSegmentPath: Identifiable {
        let id: String
        let startReason: MobileRideMapSegmentStartReason
        var coordinates: [CLLocationCoordinate2D]

        var isGap: Bool {
            startReason.isBackgroundGap
        }
    }

    struct PathKey: Equatable {
        struct SegmentMetadata: Equatable {
            let id: UInt64
            let startReason: MobileRideMapSegmentStartReason
            let visiblePointCount: UInt64
            let canonicalPointCount: UInt64?
        }

        struct ContextRouteMetadata: Equatable {
            let routeID: String
            let pointCount: Int
            let firstSequence: UInt64?
            let lastSequence: UInt64?
        }

        let routeID: String
        let projectionVersion: UInt64
        let firstSequence: UInt64?
        let lastSequence: UInt64?
        let pointCount: Int
        let segmentMetadata: [SegmentMetadata]
        let contextRouteMetadata: [ContextRouteMetadata]
    }

    let points: [MobileRideMapRouteDisplayPoint]
    let routeID: String
    let projectionVersion: UInt64
    let showsStartMarker: Bool
    let showsEndMarker: Bool
    let showsCurrentMarker: Bool
    let endpointMetadata: MobileRideMapRouteEndpointMetadata
    let cameraRegion: MobileRideMapCameraRegion?
    let segments: [MobileRideMapSegmentDisplayMetadata]
    /// Rust-bounded surrounding routes. These are rendered as subdued context and never fit the
    /// camera or participate in the selected route's endpoint annotations.
    let contextRoutes: [MobileRideMapHistoryContextRoute]
    let fitsRouteOnChange: Bool
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    let cameraDidChange: (MKCoordinateRegion) -> Void
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var segmentPaths = [SegmentPath]()
    @State private var contextSegmentPaths = [ContextSegmentPath]()
    @State private var renderedKey: PathKey?
    @State private var fittedRouteID: String?

    static func markerOffsets(
        for coordinates: [CLLocationCoordinate2D],
        dynamicTypeSize: DynamicTypeSize = .large
    ) -> (start: CGSize, end: CGSize) {
        let scale: CGFloat = dynamicTypeSize.isAccessibilitySize ? 1.5 : 1
        guard let first = coordinates.first, let last = coordinates.last, coordinates.count > 1 else {
            guard coordinates.isEmpty == false else { return (.zero, .zero) }
            return (
                CGSize(width: -36 * scale, height: -18 * scale),
                CGSize(width: 36 * scale, height: 18 * scale)
            )
        }
        let distance = CLLocation(latitude: first.latitude, longitude: first.longitude)
            .distance(from: CLLocation(latitude: last.latitude, longitude: last.longitude))
        guard distance < 80 else { return (.zero, .zero) }
        return (
            CGSize(width: -56 * scale, height: -24 * scale),
            CGSize(width: 56 * scale, height: 24 * scale)
        )
    }

    static func markerTitleLineLimit(for dynamicTypeSize: DynamicTypeSize) -> Int {
        dynamicTypeSize.isAccessibilitySize ? 2 : 1
    }

    static func canonicalEndpointPoint(
        in points: [MobileRideMapRouteDisplayPoint],
        sequence: UInt64?,
        isVisible: Bool
    ) -> MobileRideMapRouteDisplayPoint? {
        guard isVisible, let sequence else { return nil }
        return points.first { $0.sequence == sequence }
    }

    static func retainedSingletonSegmentIDs(
        in points: [MobileRideMapRouteDisplayPoint],
        segments: [MobileRideMapSegmentDisplayMetadata]
    ) -> Set<UInt64> {
        let displayedSegmentIDs = Set(points.map(\.segmentId))
        return Set(
            segments.lazy
                .filter { $0.isRetainedSingleton && displayedSegmentIDs.contains($0.segmentId) }
                .map(\.segmentId)
        )
    }

    static func pathKey(
        routeID: String,
        projectionVersion: UInt64,
        points: [MobileRideMapRouteDisplayPoint],
        segments: [MobileRideMapSegmentDisplayMetadata] = [],
        contextRoutes: [MobileRideMapHistoryContextRoute] = []
    ) -> PathKey {
        return PathKey(
            routeID: routeID,
            projectionVersion: projectionVersion,
            firstSequence: points.first?.sequence,
            lastSequence: points.last?.sequence,
            pointCount: points.count,
            segmentMetadata: segments.map {
                PathKey.SegmentMetadata(
                    id: $0.segmentId,
                    startReason: $0.startReason,
                    visiblePointCount: $0.visiblePointCount,
                    canonicalPointCount: $0.canonicalPointCount
                )
            },
            contextRouteMetadata: contextRoutes.map {
                PathKey.ContextRouteMetadata(
                    routeID: $0.rideID,
                    pointCount: $0.projection.points.count,
                    firstSequence: $0.projection.points.first?.sequence,
                    lastSequence: $0.projection.points.last?.sequence
                )
            }
        )
    }

    static func mapRegion(
        for cameraRegion: MobileRideMapCameraRegion,
        centeredOn latest: MobileRideMapRouteDisplayPoint? = nil
    ) -> MKCoordinateRegion {
        let center = latest.map {
            CLLocationCoordinate2D(latitude: $0.latitudeDegrees, longitude: $0.longitudeDegrees)
        } ?? CLLocationCoordinate2D(
            latitude: cameraRegion.centerLatitudeDegrees,
            longitude: cameraRegion.centerLongitudeDegrees
        )
        return MKCoordinateRegion(
            center: center,
            span: MKCoordinateSpan(
                latitudeDelta: cameraRegion.latitudeSpanDegrees,
                longitudeDelta: cameraRegion.longitudeSpanDegrees
            )
        )
    }

    static func geoBounds(for region: MKCoordinateRegion) -> MobileGeoBoundsDto? {
        let center = region.center
        let span = region.span
        guard center.latitude.isFinite,
              center.longitude.isFinite,
              span.latitudeDelta.isFinite,
              span.longitudeDelta.isFinite,
              span.latitudeDelta >= 0,
              span.longitudeDelta >= 0
        else {
            return nil
        }

        let halfLatitude = min(span.latitudeDelta / 2, 90)
        let minimumLatitude = max(-90, center.latitude - halfLatitude)
        let maximumLatitude = min(90, center.latitude + halfLatitude)
        let longitudeBounds: (minimum: Double, maximum: Double)
        if span.longitudeDelta >= 360 {
            longitudeBounds = (-180, 180)
        } else {
            longitudeBounds = (
                normalizeLongitude(center.longitude - span.longitudeDelta / 2),
                normalizeLongitude(center.longitude + span.longitudeDelta / 2)
            )
        }
        return MobileGeoBoundsDto(
            minimumLatitudeDegrees: minimumLatitude,
            maximumLatitudeDegrees: maximumLatitude,
            minimumLongitudeDegrees: longitudeBounds.minimum,
            maximumLongitudeDegrees: longitudeBounds.maximum
        )
    }

    var body: some View {
        let endpointOffsets = Self.markerOffsets(for: points.map {
            CLLocationCoordinate2D(latitude: $0.latitudeDegrees, longitude: $0.longitudeDegrees)
        }, dynamicTypeSize: dynamicTypeSize)
        let startPoint = Self.canonicalEndpointPoint(
            in: points,
            sequence: endpointMetadata.canonicalStartSequence,
            isVisible: endpointMetadata.canonicalStartVisible
        )
        let endPoint = Self.canonicalEndpointPoint(
            in: points,
            sequence: endpointMetadata.canonicalEndSequence,
            isVisible: endpointMetadata.canonicalEndVisible
        )
        let retainedSingletonSegmentIDs = Self.retainedSingletonSegmentIDs(in: points, segments: segments)
        let retainedSingletonPaths = segmentPaths.filter {
            retainedSingletonSegmentIDs.contains($0.id) && $0.coordinates.count == 1
        }

        Map(position: $mapPosition, interactionModes: [.pan, .zoom]) {
            ForEach(contextSegmentPaths) { segment in
                MapPolyline(coordinates: segment.coordinates)
                    .stroke(
                        PevColors.muted.opacity(0.72),
                        style: StrokeStyle(
                            lineWidth: 3,
                            lineCap: .round,
                            dash: segment.isGap ? [7, 6] : []
                        )
                    )
            }
            ForEach(segmentPaths) { segment in
                MapPolyline(coordinates: segment.coordinates)
                    .stroke(
                        PevColors.yellow,
                        style: StrokeStyle(
                            lineWidth: 4,
                            lineCap: .round,
                            dash: segment.isGap ? [8, 6] : []
                        )
                    )
            }
            ForEach(retainedSingletonPaths) { segment in
                Annotation(
                    segment.singletonAccessibilityLabel
                        ?? segment.startReason.retainedSingletonAccessibilityLabel,
                    coordinate: segment.coordinates[0]
                ) {
                    RideMapRetainedSingletonSegmentMarker(
                        startReason: segment.startReason,
                        accessibilityLabel: segment.singletonAccessibilityLabel
                            ?? segment.startReason.retainedSingletonAccessibilityLabel
                    )
                }
            }
            if showsStartMarker, let startPoint {
                Annotation(
                    "",
                    coordinate: coordinate(for: startPoint)
                ) {
                    RideMapRouteMarker(
                        title: localizedAppText("ride_map.start_marker"),
                        color: PevColors.yellow
                    )
                    .offset(x: endpointOffsets.start.width, y: endpointOffsets.start.height)
                }
            }
            if showsEndMarker, let endPoint {
                Annotation(
                    "",
                    coordinate: coordinate(for: endPoint)
                ) {
                    RideMapRouteMarker(
                        title: localizedAppText("ride_map.end_marker"),
                        color: PevColors.red
                    )
                    .offset(x: endpointOffsets.end.width, y: endpointOffsets.end.height)
                }
            }
            if showsCurrentMarker, !showsEndMarker, let last = points.last {
                Annotation(
                    "",
                    coordinate: coordinate(for: last)
                ) {
                    RideMapRouteMarker(
                        title: localizedAppText("ride_map.current_marker"),
                        color: PevColors.green
                    )
                    .offset(x: endpointOffsets.end.width, y: endpointOffsets.end.height)
                }
            }
        }
        .mapStyle(.standard(elevation: .realistic, pointsOfInterest: .excludingAll, showsTraffic: false))
        .onMapCameraChange(frequency: .onEnd) { context in
            if isApplyingCamera {
                // Consume the callback generated by our own region update. Waiting for
                // MapKit's callback, instead of dispatching a delayed reset, keeps a
                // programmatic recenter from being mistaken for a user pan.
                isApplyingCamera = false
            } else {
                cameraDidChange(context.region)
            }
        }
        .task(id: pathKey) {
            let key = pathKey
            updatePaths(for: key)
            rebuildContextPaths()
            if fitsRouteOnChange, fittedRouteID != key.routeID, points.isEmpty == false {
                fitMap(to: cameraRegion)
                fittedRouteID = key.routeID
            }
        }
        .accessibilityLabel(localizedAppText("ride_map.map_alternative"))
        .accessibilityIdentifier("ride-map.map")
    }

    private var pathKey: PathKey {
        Self.pathKey(
            routeID: routeID,
            projectionVersion: projectionVersion,
            points: points,
            segments: segments,
            contextRoutes: contextRoutes
        )
    }

    private var startReasonsBySegmentID: [UInt64: MobileRideMapSegmentStartReason] {
        Dictionary(segments.map { ($0.segmentId, $0.startReason) }, uniquingKeysWith: { first, _ in first })
    }

    private var segmentMetadataByID: [UInt64: MobileRideMapSegmentDisplayMetadata] {
        Dictionary(segments.map { ($0.segmentId, $0) }, uniquingKeysWith: { first, _ in first })
    }

    private func updatePaths(for key: PathKey) {
        guard let currentKey = renderedKey,
              currentKey.routeID == key.routeID,
              currentKey.projectionVersion == key.projectionVersion
        else {
            rebuildPaths(for: key)
            return
        }

        guard currentKey.segmentMetadata == key.segmentMetadata else {
            rebuildPaths(for: key)
            return
        }

        guard let firstSequence = key.firstSequence,
              let lastSequence = key.lastSequence,
              let renderedFirst = currentKey.firstSequence,
              let renderedLast = currentKey.lastSequence
        else {
            rebuildPaths(for: key)
            return
        }

        if key.pointCount == currentKey.pointCount + 1,
           firstSequence == renderedFirst,
           lastSequence == renderedLast + 1,
           let point = points.last
        {
            append(point)
            renderedKey = key
            return
        }

        if key.pointCount == currentKey.pointCount,
           firstSequence == renderedFirst + 1,
           lastSequence == renderedLast + 1,
           let point = points.last
        {
            dropFirstPoint()
            append(point)
            renderedKey = key
            return
        }

        rebuildPaths(for: key)
    }

    private func rebuildPaths(for key: PathKey) {
        var rebuilt = [SegmentPath]()
        for point in points {
            let coordinate = coordinate(for: point)
            if rebuilt.last?.id == point.segmentId {
                rebuilt[rebuilt.index(before: rebuilt.endIndex)].coordinates.append(coordinate)
            } else {
                rebuilt.append(
                    SegmentPath(
                        id: point.segmentId,
                        startReason: startReasonsBySegmentID[point.segmentId] ?? .unknown,
                        singletonAccessibilityLabel: segmentMetadataByID[point.segmentId]?
                            .singletonAccessibilityLabel,
                        coordinates: [coordinate]
                    )
                )
            }
        }
        segmentPaths = rebuilt
        renderedKey = key
    }

    private func rebuildContextPaths() {
        if contextRoutes.isEmpty {
            if contextSegmentPaths.isEmpty {
                return
            }
            contextSegmentPaths = []
            return
        }

        var rebuilt = [ContextSegmentPath]()
        for route in contextRoutes {
            let reasonsBySegmentID = Dictionary(
                route.projection.segments.map { ($0.segmentId, $0.startReason) },
                uniquingKeysWith: { first, _ in first }
            )
            for point in route.projection.points {
                let pathID = "\(route.rideID):\(point.segmentId)"
                if rebuilt.last?.id == pathID {
                    rebuilt[rebuilt.index(before: rebuilt.endIndex)].coordinates.append(coordinate(for: point))
                } else {
                    rebuilt.append(
                        ContextSegmentPath(
                            id: pathID,
                            startReason: reasonsBySegmentID[point.segmentId] ?? .unknown,
                            coordinates: [coordinate(for: point)]
                        )
                    )
                }
            }
        }
        contextSegmentPaths = rebuilt
    }

    private func append(_ point: MobileRideMapRouteDisplayPoint) {
        let coordinate = coordinate(for: point)
        if segmentPaths.last?.id == point.segmentId {
            segmentPaths[segmentPaths.index(before: segmentPaths.endIndex)].coordinates.append(coordinate)
        } else {
            segmentPaths.append(
                SegmentPath(
                    id: point.segmentId,
                    startReason: startReasonsBySegmentID[point.segmentId] ?? .unknown,
                    singletonAccessibilityLabel: segmentMetadataByID[point.segmentId]?
                        .singletonAccessibilityLabel,
                    coordinates: [coordinate]
                )
            )
        }
    }

    private func dropFirstPoint() {
        guard let firstPath = segmentPaths.first else { return }
        if firstPath.coordinates.count == 1 {
            segmentPaths.removeFirst()
        } else {
            segmentPaths[segmentPaths.startIndex].coordinates.removeFirst()
        }
    }

    private func coordinate(for point: MobileRideMapRouteDisplayPoint) -> CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: point.latitudeDegrees, longitude: point.longitudeDegrees)
    }

    private func fitMap(to cameraRegion: MobileRideMapCameraRegion?) {
        guard let cameraRegion else { return }
        isApplyingCamera = true
        mapPosition = .region(Self.mapRegion(for: cameraRegion))
    }

    private static func normalizeLongitude(_ longitude: Double) -> Double {
        let normalized = longitude.truncatingRemainder(dividingBy: 360)
        if normalized > 180 { return normalized - 360 }
        if normalized < -180 { return normalized + 360 }
        return normalized
    }
}

private struct RideMapRetainedSingletonSegmentMarker: View {
    let startReason: MobileRideMapSegmentStartReason
    let accessibilityLabel: String

    var body: some View {
        Circle()
            .fill(startReason.isBackgroundGap ? PevColors.orange : PevColors.cyan)
            .frame(width: 14, height: 14)
            .overlay {
                Circle()
                    .stroke(.black, lineWidth: 2)
            }
            .accessibilityElement()
            .accessibilityLabel(accessibilityLabel)
    }
}

private struct RideMapRouteMarker: View {
    let title: String
    let color: Color
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        VStack(spacing: 3) {
            if title.isEmpty == false {
                Text(title)
                    .font((dynamicTypeSize.isAccessibilitySize ? Font.caption : Font.caption2).weight(.black))
                    .textCase(.uppercase)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .background(.black.opacity(0.78), in: .rect(cornerRadius: 5))
                    .lineLimit(RideMapCanvasView.markerTitleLineLimit(for: dynamicTypeSize))
                    .minimumScaleFactor(0.75)
                    .fixedSize(horizontal: false, vertical: true)
            }

            ZStack {
                Circle()
                    .fill(.black.opacity(0.85))
                    .frame(width: 26, height: 26)
                Circle()
                    .stroke(color, lineWidth: 3)
                    .frame(width: 20, height: 20)
                Circle()
                    .fill(color)
                    .frame(width: 8, height: 8)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title.isEmpty ? localizedAppText("ride_map.current_marker") : title)
    }
}
