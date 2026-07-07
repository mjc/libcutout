import CutoutMobile
import SwiftUI

private func groupVoltageText(_ snapshot: BmsSnapshot, index: Int?) -> String {
    guard let index else { return "--" }
    return groupVoltageText(snapshot.groups.first { $0.index == index })
}

func groupVoltageText(_ group: BmsGroupSnapshot?) -> String {
    guard let value = group?.voltage?.value else { return "--" }
    return RideUnits.voltageText(millivolts: value, fractionDigits: 3)
}

func groupVoltageText(_ voltage: Voltage?) -> String {
    guard let value = voltage?.value else { return "--" }
    return RideUnits.voltageText(millivolts: value, fractionDigits: 3)
}
