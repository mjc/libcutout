import CutoutMobile
import SwiftUI

func groupVoltageText(_ group: BmsGroupSnapshot?) -> String {
    guard let value = group?.voltage?.value else { return "--" }
    return RideUnits.voltageText(millivolts: value, fractionDigits: 3)
}

func groupVoltageText(_ voltage: Voltage?) -> String {
    guard let value = voltage?.value else { return "--" }
    return RideUnits.voltageText(millivolts: value, fractionDigits: 3)
}
