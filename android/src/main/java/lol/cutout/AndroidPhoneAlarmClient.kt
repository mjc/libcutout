package lol.cutout

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.provider.Settings
import android.util.Log
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.MobileConnectionReadinessDto
import uniffi.cutout_mobile_ffi.MobileConnectionTransportStateDto
import uniffi.cutout_mobile_ffi.MobileDeviceSessionSnapshotDto
import uniffi.cutout_mobile_ffi.MobilePhoneAlarmActionsDto
import uniffi.cutout_mobile_ffi.MobilePhoneAlarmDeliveryCapabilityDto
import uniffi.cutout_mobile_ffi.MobilePhoneAlarmDeliveryRequestDto
import uniffi.cutout_mobile_ffi.MobilePhoneAlarmEventDto
import uniffi.cutout_mobile_ffi.MobilePhoneAlarmPreferencesDto
import uniffi.cutout_mobile_ffi.MobileVescRideStopReasonDto
import uniffi.cutout_mobile_ffi.MobileVescRideWarningDto
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

enum class AndroidPhoneAlarmAuthorization {
    UNAVAILABLE,
    NOT_REQUESTED,
    DISABLED,
    ALERTS_AND_SOUND,
    ALERTS_NO_SOUND,
}

data class PhoneAlarmPresentation(
    val preferences: MobilePhoneAlarmPreferencesDto? = null,
    val deviceLabel: String? = null,
    val authorization: AndroidPhoneAlarmAuthorization = AndroidPhoneAlarmAuthorization.UNAVAILABLE,
    val error: String? = null,
    val permissionRequestAvailable: Boolean = false,
) {
    val canSchedule: Boolean
        get() = authorization == AndroidPhoneAlarmAuthorization.ALERTS_AND_SOUND ||
            authorization == AndroidPhoneAlarmAuthorization.ALERTS_NO_SOUND
}

/** Executes only native notification effects requested by the Rust session owner. */
class AndroidPhoneAlarmClient(private val context: Context) {
    private val appContext = context.applicationContext
    private val thread = HandlerThread("CutoutPhoneAlarms").apply { start() }
    private val owner = Handler(thread.looper)
    private val main = Handler(context.mainLooper)
    private val notificationManager = appContext.getSystemService(NotificationManager::class.java)
    private val permissions = appContext.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
    private val observers = CopyOnWriteArrayList<(PhoneAlarmPresentation) -> Unit>()
    private val pendingObservation = AtomicReference<Pair<BlePresentation, CutoutSessionStateHandle>?>(null)
    private val observationScheduled = AtomicBoolean(false)
    @Volatile private var presentation = PhoneAlarmPresentation()
    private var activeIdentity: String? = null
    private var activeState: CutoutSessionStateHandle? = null
    private var activatedThisAttempt = false

    init {
        ensureChannels()
        publish()
    }

    fun currentPresentation(): PhoneAlarmPresentation = presentation

    fun addObserver(callback: (PhoneAlarmPresentation) -> Unit): AutoCloseable {
        observers += callback
        main.post { if (callback in observers) callback(presentation) }
        return AutoCloseable { observers.remove(callback) }
    }

    /** Called by the BLE app owner after each Rust-backed presentation update. */
    fun onSessionObservation(value: BlePresentation, state: CutoutSessionStateHandle) {
        pendingObservation.set(value to state)
        onSessionObservationReschedule()
    }

    /** Refreshes capability after returning from the platform permission request or settings. */
    fun refreshAuthorization(state: CutoutSessionStateHandle) = owner.post {
        safely("Could not refresh phone alarm authorization") {
            activeState = state
            updateCapability(state)
            drain(state)
            refreshPresentation(state)
        }
    }

    fun notePermissionRequestStarted() {
        permissions.edit().putBoolean(KEY_PERMISSION_REQUESTED, true).apply()
    }

    fun close() {
        thread.quitSafely()
    }

    fun setEnabled(enabled: Boolean) = owner.post {
        val state = activeState
        val identity = activeIdentity
        if (state == null || identity == null) {
            publish(presentation.copy(error = appContext.getString(R.string.phone_alarm_error_no_active_device)))
            return@post
        }
        val capability = capability()
        if (enabled && !capability.canSchedule) {
            publish(presentation.copy(error = appContext.getString(R.string.phone_alarm_notifications_required)))
            refreshPresentation(state)
            return@post
        }
        try {
            execute(state.setPhoneAlarmEnabled(identity, enabled), state)
            refreshPresentation(state)
            publish(presentation.copy(error = null))
        } catch (error: Exception) { fail(error) }
    }

    fun setPwmDutyPercent(percent: Int) = owner.post {
        val state = activeState
        val identity = activeIdentity
        if (state == null || identity == null) {
            publish(presentation.copy(error = appContext.getString(R.string.phone_alarm_error_no_active_device)))
            return@post
        }
        val exact = percent.takeIf { it in 1..100 }?.toUByte()
        if (exact == null) return@post
        try {
            execute(state.setPhoneAlarmPwmDutyPercent(identity, exact), state)
            refreshPresentation(state)
            publish(presentation.copy(error = null))
        } catch (error: Exception) { fail(error) }
    }

    private fun onSessionObservationReschedule() {
        if (!observationScheduled.compareAndSet(false, true)) return
        owner.post {
            try {
                while (true) {
                    val latest = pendingObservation.getAndSet(null) ?: break
                    safely("Could not process phone alarm session observation") {
                        processObservation(latest.first, latest.second)
                    }
                }
            } finally {
                observationScheduled.set(false)
                if (pendingObservation.get() != null) onSessionObservationReschedule()
            }
        }
    }

    private fun processObservation(value: BlePresentation, state: CutoutSessionStateHandle) {
        activeState = state
        val session = value.session
        val identity = session?.takeIf {
            it.connection.readiness == MobileConnectionReadinessDto.VERIFIED &&
                it.connection.transport == MobileConnectionTransportStateDto.CONNECTED
        }?.connection?.token?.platformIdentifier

        if (identity == null) {
            if (activeIdentity != null) {
                try { execute(state.deactivatePhoneAlarmDevice(), state) }
                catch (error: Exception) { Log.w(TAG, "Could not deactivate phone alarms", error) }
            }
            activeIdentity = null
            activatedThisAttempt = false
            updateCapability(state)
            refreshPresentation(state)
            publish(presentation.copy(deviceLabel = null, error = null))
            return
        }

        if (activeIdentity != identity) {
            if (activeIdentity != null) {
                try { execute(state.deactivatePhoneAlarmDevice(), state) }
                catch (error: Exception) { Log.w(TAG, "Could not retire previous phone alarms", error) }
            }
            activeIdentity = identity
            activatedThisAttempt = false
        }
        updateCapability(state)
        if (!activatedThisAttempt) {
            try {
                state.activatePhoneAlarmDevice(identity)
                activatedThisAttempt = true
                publish(presentation.copy(error = null))
            }
            catch (error: Exception) {
                Log.w(TAG, "Could not load phone alarm preferences", error)
                publish(presentation.copy(preferences = null, deviceLabel = deviceLabel(session), error = errorText(error)))
            }
        }
        drain(state)
        refreshPresentation(state, session)
    }

    private fun updateCapability(state: CutoutSessionStateHandle) {
        try { execute(state.setPhoneAlarmDeliveryCapability(capability()), state) }
        catch (error: Exception) { Log.w(TAG, "Could not update phone alarm delivery capability", error) }
    }

    private fun execute(actions: MobilePhoneAlarmActionsDto, state: CutoutSessionStateHandle) {
        actions.cancelRequestIds.forEach(::cancel)
        actions.schedule.forEach { deliver(it, state) }
    }

    private fun drain(state: CutoutSessionStateHandle) {
        execute(state.drainPhoneAlarmActions(), state)
    }

    private fun deliver(request: MobilePhoneAlarmDeliveryRequestDto, state: CutoutSessionStateHandle) {
        var posted = false
        try {
            val (title, body) = request.event.notificationText(appContext)
            val channel = if (request.playsSound) CHANNEL_SOUND else CHANNEL_SILENT
            val notification = Notification.Builder(appContext, channel)
                .setSmallIcon(android.R.drawable.stat_notify_error)
                .setContentTitle(title)
                .setContentText(body)
                .setStyle(Notification.BigTextStyle().bigText(body))
                .setAutoCancel(true)
                .setCategory(Notification.CATEGORY_ALARM)
                .setPriority(Notification.PRIORITY_HIGH)
                .build()
            val tag = "$NOTIFICATION_TAG${request.id}"
            notificationManager.notify(tag, NOTIFICATION_ID, notification)
            posted = true
        } catch (error: Exception) {
            Log.w(TAG, "Could not post phone alarm notification", error)
            publish(presentation.copy(error = appContext.getString(R.string.phone_alarm_delivery_failed)))
        }
        val accepted = try {
            state.completePhoneAlarmDelivery(request.id, posted, SystemClock.elapsedRealtime().toULong())
        } catch (error: Exception) {
            Log.w(TAG, "Could not report phone alarm delivery receipt", error)
            publish(presentation.copy(error = appContext.getString(R.string.phone_alarm_receipt_failed)))
            false
        }
        if (posted && !accepted) {
            cancel(request.id)
            if (presentation.error == null) {
                publish(presentation.copy(error = appContext.getString(R.string.phone_alarm_receipt_failed)))
            }
        } else if (posted && accepted) {
            publish(presentation.copy(error = null))
        }
    }

    private fun cancel(id: ULong) {
        try { notificationManager.cancel("$NOTIFICATION_TAG$id", NOTIFICATION_ID) }
        catch (error: Exception) { Log.w(TAG, "Could not cancel phone alarm notification", error) }
    }

    private fun refreshPresentation(state: CutoutSessionStateHandle, session: MobileDeviceSessionSnapshotDto? = null) {
        val error = try { state.phoneAlarmActivationError()?.let(::errorText) }
        catch (failure: Exception) { errorText(failure) }
        val settings = try { state.phoneAlarmPreferences() }
        catch (failure: Exception) { null }
        publish(presentation.copy(
            preferences = settings,
            deviceLabel = session?.let(::deviceLabel) ?: presentation.deviceLabel,
            authorization = authorization(),
            error = error ?: presentation.error,
            permissionRequestAvailable = canRequestRuntimePermission(),
        ))
    }

    private fun deviceLabel(session: MobileDeviceSessionSnapshotDto): String =
        session.identity?.model?.takeIf(String::isNotBlank)
            ?: appContext.getString(R.string.phone_alarm_connected_device)

    private fun capability() = phoneAlarmDeliveryCapability(authorization())

    private fun authorization(): AndroidPhoneAlarmAuthorization {
        if (Build.VERSION.SDK_INT >= 33 &&
            appContext.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return if (permissions.getBoolean(KEY_PERMISSION_REQUESTED, false)) {
                AndroidPhoneAlarmAuthorization.DISABLED
            } else AndroidPhoneAlarmAuthorization.NOT_REQUESTED
        }
        if (!notificationManager.areNotificationsEnabled()) return AndroidPhoneAlarmAuthorization.DISABLED
        if (Build.VERSION.SDK_INT >= 26) {
            val sound = notificationManager.getNotificationChannel(CHANNEL_SOUND)
            val silent = notificationManager.getNotificationChannel(CHANNEL_SILENT)
            val anyVisible = listOfNotNull(sound, silent).any { it.importance != NotificationManager.IMPORTANCE_NONE }
            if (!anyVisible) return AndroidPhoneAlarmAuthorization.DISABLED
            val soundAllowed = sound != null && sound.importance != NotificationManager.IMPORTANCE_NONE && sound.sound != null
            return if (soundAllowed) AndroidPhoneAlarmAuthorization.ALERTS_AND_SOUND
            else AndroidPhoneAlarmAuthorization.ALERTS_NO_SOUND
        }
        return AndroidPhoneAlarmAuthorization.ALERTS_AND_SOUND
    }

    private fun canRequestRuntimePermission() = Build.VERSION.SDK_INT >= 33 &&
        appContext.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED &&
        !permissions.getBoolean(KEY_PERMISSION_REQUESTED, false)

    private fun ensureChannels() {
        if (Build.VERSION.SDK_INT < 26) return
        val sound = NotificationChannel(CHANNEL_SOUND, appContext.getString(R.string.phone_alarm_channel_sound), NotificationManager.IMPORTANCE_DEFAULT).apply {
            description = appContext.getString(R.string.phone_alarm_channel_description)
            setSound(Settings.System.DEFAULT_NOTIFICATION_URI, AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_NOTIFICATION_EVENT).build())
        }
        val silent = NotificationChannel(CHANNEL_SILENT, appContext.getString(R.string.phone_alarm_channel_silent), NotificationManager.IMPORTANCE_DEFAULT).apply {
            description = appContext.getString(R.string.phone_alarm_channel_description)
            setSound(null, null)
            enableVibration(false)
        }
        notificationManager.createNotificationChannels(listOf(sound, silent))
    }

    private fun errorText(error: Throwable): String = when (error) {
        is uniffi.cutout_mobile_ffi.MobilePhoneAlarmException.NoActiveDevice -> appContext.getString(R.string.phone_alarm_error_no_active_device)
        is uniffi.cutout_mobile_ffi.MobilePhoneAlarmException.InvalidDeviceIdentity -> appContext.getString(R.string.phone_alarm_error_invalid_identity)
        is uniffi.cutout_mobile_ffi.MobilePhoneAlarmException.InvalidPwmDutyThreshold -> appContext.getString(R.string.phone_alarm_error_invalid_duty)
        is uniffi.cutout_mobile_ffi.MobilePhoneAlarmException.InvalidPwmHeadroomThreshold -> appContext.getString(R.string.phone_alarm_error_invalid_headroom)
        is uniffi.cutout_mobile_ffi.MobilePhoneAlarmException.TooManyDevices -> appContext.getString(R.string.phone_alarm_error_too_many)
        is uniffi.cutout_mobile_ffi.MobilePhoneAlarmException.DeviceIdentityChanged -> appContext.getString(R.string.phone_alarm_error_device_changed)
        is uniffi.cutout_mobile_ffi.MobilePhoneAlarmException.StorageFailure -> appContext.getString(R.string.phone_alarm_error_storage)
        else -> appContext.getString(R.string.phone_alarm_delivery_failed)
    }

    private fun fail(error: Exception) {
        Log.w(TAG, "Phone alarm state operation failed", error)
        publish(presentation.copy(error = errorText(error)))
    }

    private inline fun safely(description: String, action: () -> Unit) {
        try { action() }
        catch (error: Exception) {
            Log.w(TAG, description, error)
            fail(error)
        }
    }

    private fun publish(value: PhoneAlarmPresentation = presentation) {
        val next = value.copy(authorization = authorization(), permissionRequestAvailable = canRequestRuntimePermission())
        presentation = next
        main.post { observers.forEach { it(next) } }
    }

    companion object {
        private const val TAG = "CutoutPhoneAlarms"
        private const val PREFERENCES = "phone_alarm.native"
        private const val KEY_PERMISSION_REQUESTED = "notification_permission_requested"
        private const val CHANNEL_SOUND = "phone_ride_alarm_sound_v1"
        private const val CHANNEL_SILENT = "phone_ride_alarm_silent_v1"
        private const val NOTIFICATION_TAG = "phone-ride-alarm-"
        private const val NOTIFICATION_ID = 1
    }
}

internal fun MobilePhoneAlarmEventDto.notificationText(context: Context): Pair<String, String> {
    return when (this) {
        is MobilePhoneAlarmEventDto.PwmDuty -> context.getString(R.string.phone_alarm_notification_pwm_title) to
            context.getString(R.string.phone_alarm_notification_pwm_body, dutyPercent.toInt(), headroomPercent.toInt())
        is MobilePhoneAlarmEventDto.ControllerWarning -> context.getString(R.string.phone_alarm_notification_warning_title) to
            warning.localizedAlarmName(context)
        is MobilePhoneAlarmEventDto.ControllerStop -> context.getString(R.string.phone_alarm_notification_stop_title) to
            reason.localizedAlarmName(context)
    }
}

private fun MobileVescRideWarningDto.localizedAlarmName(context: Context): String = when (this) {
    MobileVescRideWarningDto.NONE, MobileVescRideWarningDto.UNKNOWN -> context.getString(R.string.phone_alarm_notification_warning_unknown)
    MobileVescRideWarningDto.LOW_VOLTAGE -> context.getString(R.string.phone_alarm_warning_low_voltage)
    MobileVescRideWarningDto.HIGH_VOLTAGE -> context.getString(R.string.phone_alarm_warning_high_voltage)
    MobileVescRideWarningDto.MOSFET_TEMPERATURE -> context.getString(R.string.phone_alarm_warning_mosfet_temperature)
    MobileVescRideWarningDto.MOTOR_TEMPERATURE -> context.getString(R.string.phone_alarm_warning_motor_temperature)
    MobileVescRideWarningDto.CURRENT -> context.getString(R.string.phone_alarm_warning_current)
    MobileVescRideWarningDto.DUTY_PUSHBACK -> context.getString(R.string.phone_alarm_warning_duty_pushback)
    MobileVescRideWarningDto.SPEED_PUSHBACK -> context.getString(R.string.phone_alarm_warning_speed_pushback)
    MobileVescRideWarningDto.TEMPERATURE_PUSHBACK -> context.getString(R.string.phone_alarm_warning_temperature_pushback)
    MobileVescRideWarningDto.WHEELSLIP -> context.getString(R.string.phone_alarm_warning_wheelslip)
    MobileVescRideWarningDto.SENSORS -> context.getString(R.string.phone_alarm_warning_sensors)
    MobileVescRideWarningDto.LOW_BATTERY -> context.getString(R.string.phone_alarm_warning_low_battery)
    MobileVescRideWarningDto.ERROR -> context.getString(R.string.phone_alarm_warning_error)
    MobileVescRideWarningDto.BMS_CONNECTION -> context.getString(R.string.phone_alarm_warning_bms_connection)
}

private fun MobileVescRideStopReasonDto.localizedAlarmName(context: Context): String = when (this) {
    MobileVescRideStopReasonDto.NONE -> context.getString(R.string.phone_alarm_notification_stop_unknown)
    MobileVescRideStopReasonDto.PITCH -> context.getString(R.string.phone_alarm_stop_pitch)
    MobileVescRideStopReasonDto.ROLL -> context.getString(R.string.phone_alarm_stop_roll)
    MobileVescRideStopReasonDto.SWITCH_HALF -> context.getString(R.string.phone_alarm_stop_switch_half)
    MobileVescRideStopReasonDto.SWITCH_FULL -> context.getString(R.string.phone_alarm_stop_switch_full)
    MobileVescRideStopReasonDto.REVERSE -> context.getString(R.string.phone_alarm_stop_reverse)
    MobileVescRideStopReasonDto.QUICK_STOP -> context.getString(R.string.phone_alarm_stop_quick_stop)
}

internal fun phoneAlarmDeliveryCapability(authorization: AndroidPhoneAlarmAuthorization): MobilePhoneAlarmDeliveryCapabilityDto =
    when (authorization) {
        AndroidPhoneAlarmAuthorization.ALERTS_AND_SOUND -> MobilePhoneAlarmDeliveryCapabilityDto(true, true)
        AndroidPhoneAlarmAuthorization.ALERTS_NO_SOUND -> MobilePhoneAlarmDeliveryCapabilityDto(true, false)
        AndroidPhoneAlarmAuthorization.UNAVAILABLE,
        AndroidPhoneAlarmAuthorization.NOT_REQUESTED,
        AndroidPhoneAlarmAuthorization.DISABLED -> MobilePhoneAlarmDeliveryCapabilityDto(false, false)
    }
