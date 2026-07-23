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

func bmsTemperatureMetricValue(_ temperature: Temperature?) -> PevDashboardMetricValue {
    guard let temperature else { return .unavailable }
    let text = temperatureText(temperature)
    return .available(display: text, accessibility: text)
}
