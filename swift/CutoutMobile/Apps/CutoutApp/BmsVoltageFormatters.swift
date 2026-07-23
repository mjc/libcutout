import CutoutMobile
import SwiftUI

func bmsGroupVoltageMetricValue(_ group: BmsGroupSnapshot?) -> PevDashboardMetricValue {
    bmsGroupVoltageMetricValue(group?.voltage)
}

func bmsGroupVoltageMetricValue(_ voltage: Voltage?) -> PevDashboardMetricValue {
    guard let value = voltage?.value else { return .unavailable }
    let text = RideUnits.voltageText(millivolts: value, fractionDigits: 3)
    return .available(display: text, accessibility: text)
}
