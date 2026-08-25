import CoreLocation
import CutoutMobile
import CutoutMobileFFI
import MapKit
import SwiftUI

struct RideMapCanvasView: View {
    private struct SegmentPath: Identifiable {
        let id: UInt64
        var coordinates: [CLLocationCoordinate2D]
    }

    private struct PathKey: Equatable {
        let routeID: String
        let firstSequence: UInt64?
        let lastSequence: UInt64?
        let pointCount: Int
    }

    let points: [MobileRideMapRouteDisplayPoint]
    let routeID: String
    let showsStartMarker: Bool
    let showsEndMarker: Bool
    let fitsRouteOnChange: Bool
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    let cameraDidChange: () -> Void
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @State private var segmentPaths = [SegmentPath]()
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

    static func region(for points: [MobileRideMapRouteDisplayPoint]) -> MKCoordinateRegion? {
        guard let first = points.first else { return nil }
        var minimumLatitude = first.latitudeDegrees
        var maximumLatitude = first.latitudeDegrees
        var longitudes = [first.longitudeDegrees]
        for point in points.dropFirst() {
            minimumLatitude = min(minimumLatitude, point.latitudeDegrees)
            maximumLatitude = max(maximumLatitude, point.latitudeDegrees)
            longitudes.append(point.longitudeDegrees)
        }

        let longitude = shortestLongitudeInterval(longitudes)
        return MKCoordinateRegion(
            center: CLLocationCoordinate2D(
                latitude: (minimumLatitude + maximumLatitude) / 2,
                longitude: longitude.center
            ),
            span: MKCoordinateSpan(
                latitudeDelta: max((maximumLatitude - minimumLatitude) * 1.35, 0.002),
                longitudeDelta: max(longitude.span * 1.35, 0.002)
            )
        )
    }

    var body: some View {
        let endpointOffsets = Self.markerOffsets(for: points.map {
            CLLocationCoordinate2D(latitude: $0.latitudeDegrees, longitude: $0.longitudeDegrees)
        }, dynamicTypeSize: dynamicTypeSize)

        Map(position: $mapPosition, interactionModes: [.pan, .zoom]) {
            ForEach(segmentPaths) { segment in
                MapPolyline(coordinates: segment.coordinates)
                    .stroke(
                        segment.id == 0 ? PevColors.yellow : PevColors.muted,
                        style: StrokeStyle(lineWidth: 4, dash: segment.id == 0 ? [] : [7, 5])
                    )
            }
            if showsStartMarker, let first = points.first {
                Annotation(
                    "",
                    coordinate: coordinate(for: first)
                ) {
                    RideMapRouteMarker(
                        title: localizedAppText("ride_map.start_marker"),
                        color: PevColors.yellow
                    )
                    .offset(x: endpointOffsets.start.width, y: endpointOffsets.start.height)
                }
            }
            if let last = points.last {
                Annotation(
                    "",
                    coordinate: coordinate(for: last)
                ) {
                    RideMapRouteMarker(
                        title: showsEndMarker
                            ? localizedAppText("ride_map.end_marker")
                            : localizedAppText("ride_map.current_marker"),
                        color: showsEndMarker ? PevColors.red : PevColors.green
                    )
                    .offset(x: endpointOffsets.end.width, y: endpointOffsets.end.height)
                }
            }
        }
        .mapStyle(.standard(elevation: .realistic, pointsOfInterest: .excludingAll, showsTraffic: false))
        // MapKit renders its own place labels and does not scale them with the
        // app's content hierarchy. Keep the cartographic layer at its legible
        // baseline while the surrounding controls continue to honor Dynamic
        // Type; otherwise accessibility sizes produce overlapping city names.
        .dynamicTypeSize(.large)
        .onMapCameraChange(frequency: .onEnd) { _ in
            if isApplyingCamera {
                // Consume the callback generated by our own region update. Waiting for
                // MapKit's callback, instead of dispatching a delayed reset, keeps a
                // programmatic recenter from being mistaken for a user pan.
                isApplyingCamera = false
            } else {
                cameraDidChange()
            }
        }
        .task(id: pathKey) {
            let key = pathKey
            updatePaths(for: key)
            if fitsRouteOnChange, fittedRouteID != key.routeID, points.isEmpty == false {
                fitMap(to: points)
                fittedRouteID = key.routeID
            }
        }
        .accessibilityLabel(localizedAppText("ride_map.map_alternative"))
        .accessibilityIdentifier("ride-map.map")
    }

    private var pathKey: PathKey {
        PathKey(
            routeID: routeID,
            firstSequence: points.first?.sequence,
            lastSequence: points.last?.sequence,
            pointCount: points.count
        )
    }

    private func updatePaths(for key: PathKey) {
        guard let currentKey = renderedKey, currentKey.routeID == key.routeID else {
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
                rebuilt.append(SegmentPath(id: point.segmentId, coordinates: [coordinate]))
            }
        }
        segmentPaths = rebuilt
        renderedKey = key
    }

    private func append(_ point: MobileRideMapRouteDisplayPoint) {
        let coordinate = coordinate(for: point)
        if segmentPaths.last?.id == point.segmentId {
            segmentPaths[segmentPaths.index(before: segmentPaths.endIndex)].coordinates.append(coordinate)
        } else {
            segmentPaths.append(SegmentPath(id: point.segmentId, coordinates: [coordinate]))
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

    private func fitMap(to points: [MobileRideMapRouteDisplayPoint]) {
        guard let region = Self.region(for: points) else { return }
        isApplyingCamera = true
        mapPosition = .region(region)
    }

    private static func shortestLongitudeInterval(_ longitudes: [Double]) -> (center: Double, span: Double) {
        guard longitudes.count > 1 else { return (longitudes[0], 0) }
        let normalized = longitudes.map { longitude in
            let shifted = (longitude + 180).truncatingRemainder(dividingBy: 360)
            return shifted >= 0 ? shifted : shifted + 360
        }.sorted()

        var largestGap = -1.0
        var largestGapIndex = 0
        for index in normalized.indices {
            let next = index == normalized.index(before: normalized.endIndex)
                ? normalized[0] + 360
                : normalized[index + 1]
            let gap = next - normalized[index]
            if gap > largestGap {
                largestGap = gap
                largestGapIndex = index
            }
        }

        let startIndex = normalized.index(after: largestGapIndex) == normalized.endIndex
            ? normalized.startIndex
            : normalized.index(after: largestGapIndex)
        let start = normalized[startIndex]
        let end = normalized[largestGapIndex] + (normalized[largestGapIndex] < start ? 360 : 0)
        let span = max(end - start, 0)
        let rawCenter = start + span / 2 - 180
        let center = rawCenter > 180 ? rawCenter - 360 : rawCenter
        return (center, span)
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
