import Foundation

public enum RideUnits {
    public static let speedUnit = "mph"
    public static let distanceUnitImperial = "mi"
    public static let distanceUnitMetric = "km"
    public static let temperatureUnit = "°C"

    private static let milesPerHourPerMillimeterPerSecond = 0.002_236_936_292_054_4

    public static func speedText(millimetersPerSecond: Int32, fractionDigits: Int = 1) -> String {
        decimalString(Double(millimetersPerSecond) * milesPerHourPerMillimeterPerSecond, fractionDigits: fractionDigits)
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
        decimalString(Double(Int64(milliwatts)) / 1_000_000.0, fractionDigits: fractionDigits)
    }

    public static func percentText<T: BinaryInteger>(_ percent: T) -> String {
        "\(percent)"
    }

    public static func permillePercentText<T: BinaryInteger>(_ permille: T) -> String {
        "\(permille / 10)"
    }

    public static func distanceUnit(forSpeedUnit speedUnit: String) -> String {
        switch speedUnit.lowercased().replacingOccurrences(of: " ", with: "") {
        case "km/h", "kmh", "kph":
            distanceUnitMetric
        default:
            distanceUnitImperial
        }
    }

    public static func distanceText<T: BinaryInteger>(
        millimeters: T,
        unit: String,
        fractionDigits: Int = 1
    ) -> String {
        switch unit {
        case distanceUnitMetric:
            decimalString(Double(Int64(millimeters)) / 1_000_000.0, fractionDigits: fractionDigits)
        default:
            decimalString(Double(Int64(millimeters)) / 1_609_344.0, fractionDigits: fractionDigits)
        }
    }

    public static func distanceText<T: BinaryInteger>(
        millimetres: T,
        unit: String,
        fractionDigits: Int = 1
    ) -> String {
        distanceText(millimeters: millimetres, unit: unit, fractionDigits: fractionDigits)
    }

    public static func decimalString(_ value: Double, fractionDigits: Int) -> String {
        String(format: "%.\(fractionDigits)f", value)
    }
}
