import CoreLocation
import CutoutMobileFFI
import MapKit

@MainActor
enum ChargingCountryLookup {
    static func countryCode(at coordinate: MobileMapCoordinateDto) async -> String? {
        let location = CLLocation(latitude: coordinate.latitudeDegrees, longitude: coordinate.longitudeDegrees)
        guard let request = MKReverseGeocodingRequest(location: location) else { return nil }
        do {
            let places = try await request.mapItems
            guard !Task.isCancelled else { return nil }
            return places.first?.addressRepresentations?.region?.identifier
        } catch {
            return nil
        }
    }
}
