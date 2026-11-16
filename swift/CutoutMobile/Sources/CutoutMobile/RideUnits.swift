import CutoutMobileFFI
import Foundation

public enum RideDisplayPolicy {
    public static func telemetryIsFresh(
        observedAt: MonotonicMilliseconds?,
        now: MonotonicMilliseconds
    ) -> Bool {
        telemetrySampleIsFresh(observedAtMs: observedAt?.rawValue, nowMs: now.rawValue)
    }
}

public enum ScreenshotEvidencePolicy {
    public static var maxEdgePixels: Int {
        Int(screenshotEvidenceMaxEdgePixels())
    }

    public static var jpegQuality: Double {
        Double(screenshotEvidenceJpegQuality()) / 100
    }
}

public enum RideUnits {
    public static var speedUnit: String {
        localizedUnit(rideSpeedDisplay(millimetresPerSecond: 0).unit, speed: "mph", distance: "mi")
    }
    public static let distanceUnitImperial = "mi"
    public static let distanceUnitMetric = "km"
    public static let voltageUnit = "V"
    public static let currentUnit = "A"
    public static let powerUnit = "kW"
    public static let angleUnit = "°"
    public static let temperatureUnit = "°C"
    public static let percentUnit = "%"

    public static func speedValue(millimetersPerSecond: Int32) -> Double {
        scaledValue(rideSpeedDisplay(millimetresPerSecond: Int64(millimetersPerSecond)))
    }

    public static func speedText(millimetersPerSecond: Int32, fractionDigits: Int = 1) -> String {
        speedText(millimetresPerSecond: Int64(millimetersPerSecond), fractionDigits: fractionDigits)
    }

    public static func speedText(millimetresPerSecond: Int64, fractionDigits: Int = 1) -> String {
        _ = fractionDigits
        return rideSpeedDisplay(millimetresPerSecond: millimetresPerSecond).text
    }

    public static func voltageText<T: BinaryInteger>(millivolts: T, fractionDigits: Int = 1) -> String {
        decimalString(Double(Int64(millivolts)) / 1_000.0, fractionDigits: fractionDigits)
    }

    public static func currentText<T: BinaryInteger>(milliamps: T, fractionDigits: Int = 1) -> String {
        decimalString(Double(Int64(milliamps)) / 1_000.0, fractionDigits: fractionDigits)
    }

    public static func angleText<T: BinaryInteger>(millidegrees: T, fractionDigits: Int = 1) -> String {
        decimalString(Double(Int64(millidegrees)) / 1_000.0, fractionDigits: fractionDigits)
    }

    public static func temperatureText<T: BinaryInteger>(millicelsius: T, fractionDigits: Int = 0) -> String {
        decimalString(Double(Int64(millicelsius)) / 1_000.0, fractionDigits: fractionDigits)
    }

    public static func powerText<T: BinaryInteger>(milliwatts: T, fractionDigits: Int) -> String {
        _ = fractionDigits
        return ridePowerDisplay(milliwatts: Int64(clamping: milliwatts)).text
    }

    public static func percentText<T: BinaryInteger>(_ percent: T) -> String {
        "\(percent)"
    }

    public static func permillePercentText<T: BinaryInteger>(_ permille: T) -> String {
        ridePermillePercentLabel(permille: Int64(clamping: permille)).text
    }

    public static func durationText(milliseconds: UInt64) -> String {
        rideDurationText(milliseconds: milliseconds)
    }

    public static func distanceUnit(forSpeedUnit speedUnit: String) -> String {
        _ = speedUnit
        return localizedUnit(
            rideDistanceDisplay(millimetres: 0).unit,
            speed: "mph",
            distance: distanceUnitImperial
        )
    }

    public static func distanceText<T: BinaryInteger>(
        millimeters: T,
        unit: String,
        fractionDigits: Int = 1
    ) -> String {
        _ = unit
        _ = fractionDigits
        return rideDistanceDisplay(millimetres: Int64(clamping: millimeters)).text
    }

    public static func distanceText<T: BinaryInteger>(
        millimetres: T,
        unit: String,
        fractionDigits: Int = 1
    ) -> String {
        distanceText(millimeters: millimetres, unit: unit, fractionDigits: fractionDigits)
    }

    private static func scaledValue(_ display: MobileRideQuantityDisplayDto) -> Double {
        var scale = 1.0
        for _ in 0..<display.fractionDigits {
            scale *= 10
        }
        guard scale > 0 else { return 0 }
        return Double(display.scaled) / scale
    }

    private static func localizedUnit(
        _ unit: MobileRideDisplayUnitDto,
        speed: String,
        distance: String
    ) -> String {
        switch unit {
        case .milesPerHour:
            speed
        case .miles:
            distance
        case .kilowatts:
            powerUnit
        case .percent:
            percentUnit
        }
    }

    public static func decimalString(
        _ value: Double,
        fractionDigits: Int,
        locale: Locale = .current
    ) -> String {
        value.formatted(
            .number
                .precision(.fractionLength(fractionDigits))
                .locale(locale)
        )
    }
}
