package lol.cutout.ffitest

import android.app.Activity
import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.Switch
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidPhoneAlarmAuthorization
import lol.cutout.AndroidPhoneAlarmClient
import lol.cutout.MainActivity
import lol.cutout.PhoneAlarmPanel
import lol.cutout.PhoneAlarmPresentation
import lol.cutout.R
import lol.cutout.notificationText
import lol.cutout.phoneAlarmDeliveryCapability
import org.junit.Test
import uniffi.cutout_mobile_ffi.MobilePhoneAlarmDeliveryCapabilityDto
import uniffi.cutout_mobile_ffi.MobilePhoneAlarmEventDto
import uniffi.cutout_mobile_ffi.MobilePhoneAlarmPreferencesDto
import uniffi.cutout_mobile_ffi.MobileVescRideStopReasonDto
import uniffi.cutout_mobile_ffi.MobileVescRideWarningDto

/** Android effects/presentation only; domain alarm decisions remain covered by Rust tests. */
open class PhoneAlarmPanelTest {
    @Test
    fun androidAuthorizationMapsOnlySupportedDeliveryCapabilities() {
        check(phoneAlarmDeliveryCapability(AndroidPhoneAlarmAuthorization.UNAVAILABLE) == MobilePhoneAlarmDeliveryCapabilityDto(false, false))
        check(phoneAlarmDeliveryCapability(AndroidPhoneAlarmAuthorization.NOT_REQUESTED) == MobilePhoneAlarmDeliveryCapabilityDto(false, false))
        check(phoneAlarmDeliveryCapability(AndroidPhoneAlarmAuthorization.DISABLED) == MobilePhoneAlarmDeliveryCapabilityDto(false, false))
        check(phoneAlarmDeliveryCapability(AndroidPhoneAlarmAuthorization.ALERTS_NO_SOUND) == MobilePhoneAlarmDeliveryCapabilityDto(true, false))
        check(phoneAlarmDeliveryCapability(AndroidPhoneAlarmAuthorization.ALERTS_AND_SOUND) == MobilePhoneAlarmDeliveryCapabilityDto(true, true))
    }

    @Test
    fun nativeNotificationCopyMatchesRustEventAndUnknownFallbacks() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val pwm = MobilePhoneAlarmEventDto.PwmDuty(83u.toUByte(), 17u.toUByte()).notificationText(context)
        check(pwm.first == context.getString(R.string.phone_alarm_notification_pwm_title))
        check(pwm.second == context.getString(R.string.phone_alarm_notification_pwm_body, 83, 17))
        val warning = MobilePhoneAlarmEventDto.ControllerWarning(MobileVescRideWarningDto.UNKNOWN).notificationText(context)
        check(warning.second == context.getString(R.string.phone_alarm_notification_warning_unknown))
        val stop = MobilePhoneAlarmEventDto.ControllerStop(MobileVescRideStopReasonDto.PITCH).notificationText(context)
        check(stop.second == context.getString(R.string.phone_alarm_stop_pitch))
    }

    @Test
    fun panelKeepsPermissionAndThresholdControlsTruthfulAcrossNativeStates() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(
            Intent(instrumentation.targetContext, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        ) as Activity
        val client = AndroidPhoneAlarmClient(activity.applicationContext)
        try {
            instrumentation.runOnMainSync {
                val panel = PhoneAlarmPanel(activity, client) {}
                activity.setContentView(panel)
                panel.render(PhoneAlarmPresentation(
                    authorization = AndroidPhoneAlarmAuthorization.NOT_REQUESTED,
                    permissionRequestAvailable = true,
                ))
                check(!find<Button>(panel, "phone-alarm.permission.request").isShown)
                check(!find<View>(panel, "phone-alarm.configured").isShown)
                check(find<TextView>(panel) { it.text == activity.getString(R.string.phone_alarm_no_device) }.isShown)

                panel.render(PhoneAlarmPresentation(
                    preferences = MobilePhoneAlarmPreferencesDto("opaque-platform-id", true, 1u.toUByte(), 99u.toUByte()),
                    authorization = AndroidPhoneAlarmAuthorization.NOT_REQUESTED,
                    permissionRequestAvailable = true,
                ))
                check(find<Button>(panel, "phone-alarm.permission.request").isShown)
                check(find<View>(panel, "phone-alarm.configured").isShown)
                panel.render(PhoneAlarmPresentation(
                    preferences = MobilePhoneAlarmPreferencesDto("opaque-platform-id", true, 1u.toUByte(), 99u.toUByte()),
                    deviceLabel = activity.getString(R.string.phone_alarm_connected_device),
                    authorization = AndroidPhoneAlarmAuthorization.DISABLED,
                ))
                val toggle = find<Switch>(panel, "phone-alarm.enabled")
                check(toggle.isChecked && toggle.isEnabled)
                check(find<Button>(panel, "phone-alarm.permission.settings").isShown)
                check(find<Button>(panel, "phone-alarm.pwm-duty.decrease").isEnabled.not())
                check(find<Button>(panel, "phone-alarm.pwm-duty.increase").isEnabled)
                check(descendants(panel).filterIsInstance<TextView>().none { it.text.toString().contains("opaque-platform-id") })

                panel.render(PhoneAlarmPresentation(
                    preferences = MobilePhoneAlarmPreferencesDto("opaque-platform-id", true, 100u.toUByte(), 0u.toUByte()),
                    deviceLabel = activity.getString(R.string.phone_alarm_connected_device),
                    authorization = AndroidPhoneAlarmAuthorization.ALERTS_AND_SOUND,
                ))
                check(find<Button>(panel, "phone-alarm.pwm-duty.decrease").isEnabled)
                check(find<Button>(panel, "phone-alarm.pwm-duty.increase").isEnabled.not())
            }
        } finally {
            client.close()
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    private inline fun <reified T : View> find(root: View, tag: String): T =
        checkNotNull((root as? ViewGroup)?.findViewWithTag(tag) as? T) { "Missing $tag" }

    private inline fun <reified T : View> find(root: View, noinline predicate: (T) -> Boolean): T =
        checkNotNull(descendants(root).filterIsInstance<T>().firstOrNull(predicate))

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }
}
