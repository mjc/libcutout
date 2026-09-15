import Foundation
import XCTest
@testable import CutoutApp

final class AppLocalizationTests: XCTestCase {
    func testSemanticDeviceControlKeysHaveRiderFacingEnglishCopy() {
        let keys = [
            "actions.gyro_calibration.help",
            "actions.gyro_calibration.label",
            "actions.horn.help",
            "actions.horn.label",
            "actions.trip_meter_reset.help",
            "actions.trip_meter_reset.label",
            "settings.acceleration_assist.label",
            "settings.beeper_volume_level.label",
            "settings.beeper_volume_percent.label",
            "settings.brake_overpressure_alarm.label",
            "settings.auto_shutdown_remaining.label",
            "settings.charge_mode.label",
            "settings.charge_limit_diagnostic.label",
            "settings.choice.both_alarm_stages",
            "settings.choice.charging",
            "settings.choice.first_alarm_stage",
            "settings.choice.hard",
            "settings.choice.high",
            "settings.choice.imperial",
            "settings.choice.low",
            "settings.choice.medium",
            "settings.choice.metric",
            "settings.choice.not_charging",
            "settings.choice.off",
            "settings.choice.pattern_0",
            "settings.choice.pattern_1",
            "settings.choice.pattern_2",
            "settings.choice.pattern_3",
            "settings.choice.pattern_4",
            "settings.choice.pattern_5",
            "settings.choice.pattern_6",
            "settings.choice.pattern_7",
            "settings.choice.pattern_8",
            "settings.choice.pattern_9",
            "settings.choice.pwm_tiltback",
            "settings.choice.soft",
            "settings.display_brightness.label",
            "settings.display_units.label",
            "settings.dynamic_assist.label",
            "settings.headlight.label",
            "settings.high_beam.label",
            "settings.high_speed_mode.label",
            "settings.lateral_tilt_limit.label",
            "settings.lighting_pattern.label",
            "settings.low_battery_mode.label",
            "settings.maximum_speed.label",
            "settings.pedal_angle.label",
            "settings.pedal_dip_compensation.label",
            "settings.pedal_hardness.label",
            "settings.pedal_mode.label",
            "settings.pwm_tiltback.label",
            "settings.power_off_delay.label",
            "settings.riding_preset.label",
            "settings.roll_angle_mode.label",
            "settings.speed_alarm_mode.label",
            "settings.speed_alarm_threshold.label",
            "settings.taillight.label",
            "settings.tiltback_speed.label",
            "settings.transport_mode.label",
            "settings.voltage_correction.label",
        ]

        for key in keys {
            XCTAssertNotEqual(localizedAppText(key), key, key)
        }
        XCTAssertEqual(localizedAppText("settings.pwm_tiltback.label"), "PWM duty tilt-back")
    }

    func testCatalogFixtureFormatsHighlightedBmsAccessibilityValue() throws {
        let fixture = try AppLocalizationBundleFixture(strings: """
        "bms.group.accessibility.highlighted" = "Highlighted: %1$@";
        """)
        defer { fixture.remove() }

        XCTAssertEqual(
            localizedAppText(
                "bms.group.accessibility.highlighted",
                arguments: ["Voltage: 4.036"],
                bundle: fixture.bundle,
                locale: Locale(identifier: "en_US")
            ),
            "Highlighted: Voltage: 4.036"
        )
    }
}

private struct AppLocalizationBundleFixture {
    let root: URL
    let bundle: Bundle

    init(strings: String) throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: UUID().uuidString, directoryHint: .isDirectory)
            .appendingPathExtension("bundle")
        let localization = root.appending(path: "en.lproj", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: localization, withIntermediateDirectories: true)
        try strings.write(to: localization.appending(path: "Localizable.strings"), atomically: true, encoding: .utf8)
        self.root = root
        bundle = try XCTUnwrap(Bundle(url: root))
    }

    func remove() {
        try? FileManager.default.removeItem(at: root)
    }
}
