import CutoutMobile
import SwiftUI

func groupVoltageText(_ group: BmsGroupSnapshot?) -> String {
    groupVoltageText(group?.voltage)
}

func groupVoltageText(_ voltage: Voltage?) -> String {
    guard let value = voltage?.value else { return "--" }
    return RideUnits.voltageText(millivolts: value, fractionDigits: 3)
}
