import CutoutMobileFFI
import MapKit

/// Builds a camera region that keeps both the starting location and chosen pin visible.
enum ChargingMapCamera {
    private static let minimumSpanMeters = 1_000.0
    private static let padding = 1.25

    static func regionIncluding(
        origin: MobileMapCoordinateDto,
        target: MobileMapCoordinateDto
    ) -> MKCoordinateRegion {
        let originPoint = MKMapPoint(
            CLLocationCoordinate2D(
                latitude: origin.latitudeDegrees,
                longitude: origin.longitudeDegrees
            ))
        let targetPoint = MKMapPoint(
            CLLocationCoordinate2D(
                latitude: target.latitudeDegrees,
                longitude: target.longitudeDegrees
            ))
        let worldWidth = MKMapRect.world.size.width
        var deltaX = targetPoint.x - originPoint.x
        if deltaX > worldWidth / 2 {
            deltaX -= worldWidth
        } else if deltaX < -worldWidth / 2 {
            deltaX += worldWidth
        }

        let centerY = (originPoint.y + targetPoint.y) / 2
        let centerLatitude = MKMapPoint(x: originPoint.x, y: centerY).coordinate.latitude
        let minimumSpanPoints = minimumSpanMeters / MKMetersPerMapPointAtLatitude(centerLatitude)
        let width = max(abs(deltaX) * padding, minimumSpanPoints)
        let height = max(abs(targetPoint.y - originPoint.y) * padding, minimumSpanPoints)
        let unwrappedTargetX = originPoint.x + deltaX
        let rect = MKMapRect(
            origin: MKMapPoint(
                x: min(originPoint.x, unwrappedTargetX) - (width - abs(deltaX)) / 2,
                y: centerY - height / 2
            ),
            size: MKMapSize(width: width, height: height)
        )

        var centerX = (originPoint.x + unwrappedTargetX) / 2
        centerX.formTruncatingRemainder(dividingBy: worldWidth)
        if centerX < 0 { centerX += worldWidth }
        let center = MKMapPoint(x: centerX, y: centerY).coordinate
        let north = MKMapPoint(x: centerX, y: rect.minY).coordinate.latitude
        let south = MKMapPoint(x: centerX, y: rect.maxY).coordinate.latitude
        let span = MKCoordinateSpan(
            latitudeDelta: max(abs(north - south), 0.000_001),
            longitudeDelta: min(width / worldWidth * 360, 360)
        )
        return MKCoordinateRegion(center: center, span: span)
    }
}
