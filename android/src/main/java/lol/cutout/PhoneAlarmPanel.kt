package lol.cutout

import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.view.View
import android.view.Gravity
import android.widget.LinearLayout
import android.widget.Switch
import android.widget.TextView
import uniffi.cutout_mobile_ffi.MobilePhoneAlarmPreferencesDto

/** Setup-route presentation for the Rust-owned phone alarm settings. */
class PhoneAlarmPanel(
    context: Context,
    private val client: AndroidPhoneAlarmClient,
    private val onRequestNotificationPermission: () -> Unit,
) : LinearLayout(context) {
    private val ui = CutoutUi(context)
    private val title = ui.text(context.getString(R.string.phone_alarm_title), 22f, true).apply {
        isAccessibilityHeading = true
    }
    private val summary = ui.text(context.getString(R.string.phone_alarm_summary), 15f, color = ui.muted)
    private val unavailable = ui.text(context.getString(R.string.phone_alarm_no_device), 15f, color = ui.muted)
    private val deviceTitle = ui.text("", 17f, true).apply { isAccessibilityHeading = true }
    private val enabled = Switch(context).apply {
        text = context.getString(R.string.phone_alarm_enabled)
        textSize = 17f
        setTextColor(ui.foreground)
        minimumHeight = ui.dp(48)
        tag = "phone-alarm.enabled"
    }
    private val dutyTitle = ui.text(context.getString(R.string.phone_alarm_pwm_section), 17f, true).apply { isAccessibilityHeading = true }
    private val dutyValue = ui.text("", 17f, true)
    private val dutyMinus = ui.action("−") { changeDuty(-1) }.apply {
        tag = "phone-alarm.pwm-duty.decrease"
        contentDescription = context.getString(R.string.phone_alarm_decrease_pwm)
    }
    private val dutyPlus = ui.action("+") { changeDuty(1) }.apply {
        tag = "phone-alarm.pwm-duty.increase"
        contentDescription = context.getString(R.string.phone_alarm_increase_pwm)
    }
    private val headroom = ui.text("", 17f)
    private val pwmExplanation = ui.text(context.getString(R.string.phone_alarm_pwm_explanation), 14f, color = ui.muted)
    private val sourceTitle = ui.text(context.getString(R.string.phone_alarm_source_section), 17f, true).apply { isAccessibilityHeading = true }
    private val sourceExplanation = ui.text(context.getString(R.string.phone_alarm_source_explanation), 14f, color = ui.muted)
    private val notificationsTitle = ui.text(context.getString(R.string.phone_alarm_notifications_section), 17f, true).apply { isAccessibilityHeading = true }
    private val authorization = ui.text("", 15f, color = ui.muted)
    private val requestPermission = ui.action(context.getString(R.string.phone_alarm_request_permission), primary = true) {
        client.notePermissionRequestStarted()
        onRequestNotificationPermission()
    }.apply { tag = "phone-alarm.permission.request" }
    private val openSettings = ui.action(context.getString(R.string.phone_alarm_open_settings)) { openNotificationSettings() }
        .apply { tag = "phone-alarm.permission.settings" }
    private val error = ui.text("", 14f, color = ui.red)
    private var latest: PhoneAlarmPresentation = client.currentPresentation()
    private var observer: AutoCloseable? = null
    private var rendering = false

    init {
        orientation = VERTICAL
        setBackgroundColor(ui.background)
        setPadding(0, 0, 0, 0)
        addView(title)
        addView(ui.space(8))
        addView(summary)
        addView(ui.space(24))
        addView(unavailable)
        deviceTitle.visibility = GONE
        unavailable.visibility = GONE
        addView(deviceTitle)
        addView(ui.space(12))
        addCard(listOf(enabled))
        addView(ui.space(20))
        addView(dutyTitle)
        addView(ui.space(10))
        val dutyLine = ui.row().apply {
            addView(ui.text(context.getString(R.string.phone_alarm_pwm_duty), 16f), LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            addView(dutyValue, LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT))
        }
        val controls = ui.row().apply {
            gravity = Gravity.CENTER_VERTICAL
            addView(dutyMinus, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            addView(dutyPlus, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        }
        addCard(listOf(dutyLine, controls, headroom, pwmExplanation))
        addView(ui.space(20))
        addView(sourceTitle)
        addView(ui.space(10))
        addCard(listOf(sourceExplanation))
        addView(ui.space(20))
        addView(notificationsTitle)
        addView(ui.space(10))
        addCard(listOf(authorization, requestPermission, openSettings, error))
        enabled.setOnCheckedChangeListener { _, checked ->
            if (rendering) return@setOnCheckedChangeListener
            if (checked && !latest.canSchedule) {
                render(latest)
                if (latest.permissionRequestAvailable) {
                    client.notePermissionRequestStarted()
                    onRequestNotificationPermission()
                } else openNotificationSettings()
            } else client.setEnabled(checked)
        }
        render(latest)
    }

    fun render(value: PhoneAlarmPresentation) {
        latest = value
        val settings = value.preferences
        rendering = true
        try {
            val hasSettings = settings != null
            unavailable.visibility = if (hasSettings) GONE else VISIBLE
            deviceTitle.visibility = if (hasSettings) VISIBLE else GONE
            enabled.visibility = if (hasSettings) VISIBLE else GONE
            dutyTitle.visibility = if (hasSettings) VISIBLE else GONE
            dutyValue.visibility = if (hasSettings) VISIBLE else GONE
            dutyMinus.visibility = if (hasSettings) VISIBLE else GONE
            dutyPlus.visibility = if (hasSettings) VISIBLE else GONE
            headroom.visibility = if (hasSettings) VISIBLE else GONE
            pwmExplanation.visibility = if (hasSettings) VISIBLE else GONE
            sourceTitle.visibility = if (hasSettings) VISIBLE else GONE
            sourceExplanation.visibility = if (hasSettings) VISIBLE else GONE
            deviceTitle.text = value.deviceLabel.orEmpty()
            settings?.let { renderPreferences(it) }
            authorization.text = context.getString(authorizationString(value.authorization))
            requestPermission.visibility = if (value.permissionRequestAvailable) VISIBLE else GONE
            openSettings.visibility = if (value.authorization == AndroidPhoneAlarmAuthorization.DISABLED) VISIBLE else GONE
            error.text = value.error.orEmpty()
            error.visibility = if (value.error.isNullOrBlank()) GONE else VISIBLE
        } finally { rendering = false }
    }

    private fun renderPreferences(settings: MobilePhoneAlarmPreferencesDto) {
        enabled.isChecked = settings.enabled
        enabled.isEnabled = true
        dutyValue.text = context.getString(R.string.phone_alarm_percent, settings.pwmDutyPercent.toInt())
        dutyValue.contentDescription = context.getString(R.string.phone_alarm_pwm_duty_value, settings.pwmDutyPercent.toInt())
        headroom.text = context.getString(R.string.phone_alarm_pwm_headroom_value, settings.pwmHeadroomPercent.toInt())
        pwmExplanation.text = context.getString(
            R.string.phone_alarm_pwm_explanation,
            settings.pwmDutyPercent.toInt(), settings.pwmHeadroomPercent.toInt(),
        )
        dutyMinus.isEnabled = settings.pwmDutyPercent.toInt() > 1
        dutyPlus.isEnabled = settings.pwmDutyPercent.toInt() < 100
    }

    private fun changeDuty(delta: Int) {
        val current = latest.preferences?.pwmDutyPercent?.toInt() ?: return
        val next = (current + delta).coerceIn(1, 100)
        if (next != current) client.setPwmDutyPercent(next)
    }

    private fun authorizationString(value: AndroidPhoneAlarmAuthorization): Int = when (value) {
        AndroidPhoneAlarmAuthorization.UNAVAILABLE -> R.string.phone_alarm_authorization_unavailable
        AndroidPhoneAlarmAuthorization.NOT_REQUESTED -> R.string.phone_alarm_authorization_not_requested
        AndroidPhoneAlarmAuthorization.DISABLED -> R.string.phone_alarm_authorization_denied
        AndroidPhoneAlarmAuthorization.ALERTS_AND_SOUND -> R.string.phone_alarm_authorization_alerts_and_sound
        AndroidPhoneAlarmAuthorization.ALERTS_NO_SOUND -> R.string.phone_alarm_authorization_alerts_no_sound
    }

    private fun openNotificationSettings() {
        val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).apply {
            putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        runCatching { context.startActivity(intent) }
    }

    private fun addCard(children: List<View>) {
        val card = ui.column().apply {
            background = ui.rounded(18)
            setPadding(ui.dp(16), ui.dp(8), ui.dp(16), ui.dp(8))
        }
        children.forEachIndexed { index, child ->
            if (index > 0) card.addView(ui.space(8))
            card.addView(child, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
        }
        addView(card, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        observer?.close()
        observer = client.addObserver(::render)
    }

    override fun onDetachedFromWindow() {
        observer?.close()
        observer = null
        super.onDetachedFromWindow()
    }

    fun dispose() {
        observer?.close()
        observer = null
    }
}
