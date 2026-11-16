import CutoutMobile
import Foundation

enum RideMapMetricFormatting {
    @MainActor
    static func vehicleLabel(
        identity: String?,
        resolve: (String) -> String?,
        noIdentityFallback: String
    ) -> String {
        guard let identity else {
            return noIdentityFallback
        }
        return resolve(identity)
            .flatMap { $0.isEmpty ? nil : $0 }
            ?? localizedAppText("ride_map.vehicle_name_unavailable")
    }

    static func distanceText(for summary: MobileRideMapSummaryDto) -> String {
        let millimetres = (summary.distanceMeters * 1_000).rounded()
        guard millimetres.isFinite, millimetres <= Double(Int64.max), millimetres >= Double(Int64.min) else {
            return "0.0 \(RideUnits.distanceUnitImperial)"
        }
        let text = RideUnits.distanceText(
            millimetres: Int64(millimetres),
            unit: RideUnits.distanceUnitImperial
        )
        return "\(text) \(RideUnits.distanceUnit(forSpeedUnit: RideUnits.speedUnit))"
    }

    static func movingTimeText(for summary: MobileRideMapSummaryDto) -> String {
        switch summary.movingTime {
        case .unavailable:
            "--"
        case .complete(let movingMilliseconds, _):
            durationText(milliseconds: movingMilliseconds)
        case .partial(let movingMilliseconds, _, _):
            "≥ \(durationText(milliseconds: movingMilliseconds))"
        }
    }

    static func movingTimeSpokenValue(for summary: MobileRideMapSummaryDto) -> String {
        switch summary.movingTime {
        case .unavailable:
            localizedAppText("ride_map.moving_time_unavailable")
        case .complete(let movingMilliseconds, _):
            durationText(milliseconds: movingMilliseconds)
        case .partial(let movingMilliseconds, _, _):
            localizedAppText("ride_map.moving_time_partial", durationText(milliseconds: movingMilliseconds))
        }
    }

    static func elapsedTimeText(for summary: MobileRideMapSummaryDto) -> String {
        switch summary.elapsedTime {
        case .unavailable:
            "--"
        case .complete(let durationMilliseconds):
            durationText(milliseconds: durationMilliseconds)
        case .partial(let durationMilliseconds):
            "≥ \(durationText(milliseconds: durationMilliseconds))"
        }
    }

    static func elapsedTimeSpokenValue(for summary: MobileRideMapSummaryDto) -> String {
        switch summary.elapsedTime {
        case .unavailable:
            localizedAppText("ride_map.elapsed_time_unavailable")
        case .complete(let durationMilliseconds):
            durationText(milliseconds: durationMilliseconds)
        case .partial(let durationMilliseconds):
            localizedAppText("ride_map.elapsed_time_partial", durationText(milliseconds: durationMilliseconds))
        }
    }

    private static func durationText(milliseconds: UInt64) -> String {
        RideUnits.durationText(milliseconds: milliseconds)
    }

    static func recordedAtText(for milliseconds: UInt64) -> String {
        guard milliseconds > 0 else {
            return localizedAppText("ride_map.untitled_ride")
        }
        return Date(timeIntervalSince1970: Double(milliseconds) / 1_000)
            .formatted(.dateTime.month(.abbreviated).day().year().hour().minute())
    }
}
