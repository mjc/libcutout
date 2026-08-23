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

    let points: [MobileRideMapPointDto]
    let routeID: String
    @Binding var mapPosition: MapCameraPosition
    @Binding var isApplyingCamera: Bool
    let cameraDidChange: () -> Void
    @State private var segmentPaths = [SegmentPath]()
    @State private var renderedKey: PathKey?

    var body: some View {
        Map(position: $mapPosition, interactionModes: [.pan, .zoom]) {
            ForEach(segmentPaths) { segment in
                MapPolyline(coordinates: segment.coordinates)
                    .stroke(
                        segment.id == 0 ? .blue : .orange,
                        style: StrokeStyle(lineWidth: 4, dash: segment.id == 0 ? [] : [6, 4])
                    )
            }
            if let first = points.first {
                Marker(
                    localizedAppText("ride_map.start_marker"),
                    coordinate: coordinate(for: first)
                )
                .tint(.green)
            }
            if let last = points.last, points.count > 1 {
                Marker(
                    localizedAppText("ride_map.current_marker"),
                    coordinate: coordinate(for: last)
                )
                .tint(.blue)
            }
        }
        .mapStyle(.standard)
        .onMapCameraChange(frequency: .onEnd) { _ in
            if isApplyingCamera == false {
                cameraDidChange()
            }
        }
        .task(id: pathKey) {
            updatePaths(for: pathKey)
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

    private func append(_ point: MobileRideMapPointDto) {
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

    private func coordinate(for point: MobileRideMapPointDto) -> CLLocationCoordinate2D {
        CLLocationCoordinate2D(latitude: point.latitudeDegrees, longitude: point.longitudeDegrees)
    }
}
