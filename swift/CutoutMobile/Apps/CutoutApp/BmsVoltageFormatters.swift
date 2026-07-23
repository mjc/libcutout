import CutoutMobile
import SwiftUI

func bmsGroupVoltageMetricValue(_ group: BmsGroupSnapshot?) -> PevDashboardMetricValue {
    bmsVoltageMetricValue(group?.voltage)
}

func bmsVoltageMetricValue(_ voltage: Voltage?) -> PevDashboardMetricValue {
    guard let value = voltage?.value else { return .unavailable }
    let text = RideUnits.voltageText(millivolts: value, fractionDigits: 3)
    return .available(display: text, accessibility: text)
}

func bmsPackVoltageMetricValue(_ voltage: Voltage?) -> PevDashboardMetricValue {
    guard let value = voltage?.value else { return .unavailable }
    let text = RideUnits.voltageText(millivolts: value)
    return .available(display: text, accessibility: text)
}

func bmsVoltageSagMetricValue(_ voltageSag: VoltageDelta?) -> PevDashboardMetricValue {
    guard let value = voltageSag?.value else { return .unavailable }
    let text = RideUnits.decimalString(abs(Double(value)) / 1_000.0, fractionDigits: 1)
    return .available(display: text, accessibility: text)
}

func bmsBatteryCurrentMetricValue(_ current: BatteryCurrent?) -> PevDashboardMetricValue {
    guard let value = current?.value else { return .unavailable }
    let text = RideUnits.decimalString(Double(value) / 1_000.0, fractionDigits: 0)
    return .available(display: text, accessibility: text)
}

func bmsTemperatureMetricValue(_ temperature: Temperature?) -> PevDashboardMetricValue {
    guard let temperature else { return .unavailable }
    let text = temperatureText(temperature)
    return .available(display: text, accessibility: text)
}
