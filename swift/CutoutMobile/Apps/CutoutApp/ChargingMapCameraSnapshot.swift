import MapKit
import SwiftUI

/// Keeps high-frequency camera updates without invalidating the whole map view.
@MainActor
final class ChargingMapCameraSnapshot {
    private(set) var camera: MapCamera?

    func capture(_ camera: MapCamera) {
        self.camera = camera
    }
}
