package lol.cutout

import android.Manifest
import android.app.Activity
import android.app.AlertDialog
import android.bluetooth.BluetoothAdapter
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.text.InputType
import android.text.TextUtils
import android.view.Gravity
import android.view.View
import android.view.WindowInsets
import android.widget.*
import org.json.JSONObject
import java.math.BigDecimal
import java.util.Locale
import uniffi.cutout_mobile_ffi.*

/** Android presentation only: all device/session/settings decisions are Rust projections. */
class MainActivity : Activity() {
    private val client get() = (application as CutoutApplication).bluetooth
    private val accent = Color.rgb(248, 224, 0)
    private val muted = Color.rgb(165, 165, 170)
    private var page = R.string.ride
    private var latest = BlePresentation()
    private var scanned = false
    private var configurationSignature = ""
    private var devicesSignature = ""
    private lateinit var root: LinearLayout
    private lateinit var content: FrameLayout
    private lateinit var ride: LinearLayout
    private lateinit var devicePage: LinearLayout
    private lateinit var configuration: LinearLayout
    private lateinit var status: TextView
    private lateinit var deviceName: TextView
    private lateinit var speed: TextView
    private lateinit var battery: TextView
    private lateinit var voltage: TextView
    private lateinit var power: TextView
    private lateinit var temperature: TextView
    private lateinit var headroom: TextView
    private lateinit var connectionButton: Button
    private lateinit var scanButton: Button
    private lateinit var permissionDetail: TextView
    private lateinit var settingsButton: Button
    private lateinit var devicesList: LinearLayout
    private val navigation = mutableMapOf<Int, Button>()
    private val labels by lazy {
        resources.openRawResource(R.raw.shared_labels).bufferedReader().use { JSONObject(it.readText()) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        page = savedInstanceState?.getInt("page") ?: R.string.ride
        root = column().apply { setBackgroundColor(Color.BLACK) }
        root.setOnApplyWindowInsetsListener { view, insets ->
            val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            view.setPadding(bars.left + dp(20), bars.top + dp(12), bars.right + dp(20), bars.bottom + dp(12))
            insets
        }
        val heading = row()
        heading.addView(text(getString(R.string.app_name), 26f, true), LinearLayout.LayoutParams(0, dp(52), 1f))
        status = text(getString(R.string.disconnected), 15f, true).apply { gravity = Gravity.CENTER_VERTICAL or Gravity.END }
        heading.addView(status)
        root.addView(heading)
        content = FrameLayout(this)
        root.addView(content, LinearLayout.LayoutParams(-1, 0, 1f))
        buildRide()
        buildDevices()
        configuration = column()
        val configScroll = ScrollView(this).apply { addView(configuration); tag = R.string.configuration }
        content.addView(configScroll)
        val tabs = row().apply { background = rounded(Color.rgb(27, 27, 29)); setPadding(dp(4), dp(4), dp(4), dp(4)) }
        for (title in listOf(R.string.ride, R.string.devices, R.string.configuration)) {
            val button = button(title) { showPage(title) }
            navigation[title] = button
            tabs.addView(button, LinearLayout.LayoutParams(0, -2, 1f))
        }
        root.addView(tabs)
        setContentView(root)
        showPage(page)
    }

    override fun onResume() { super.onResume(); client.observe(::render) }
    override fun onStop() { client.observe(null); client.stopScan(); super.onStop() }
    override fun onSaveInstanceState(outState: Bundle) { outState.putInt("page", page); super.onSaveInstanceState(outState) }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grants: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grants)
        if (requestCode == 1 && client.hasPermissions()) { scanned = true; client.scan() }
        render(latest)
    }

    private fun buildRide() {
        ride = column().apply { tag = R.string.ride }
        val header = row()
        deviceName = text(getString(R.string.find_pev), 20f, true).apply {
            maxLines = 1; ellipsize = TextUtils.TruncateAt.END
        }
        header.addView(deviceName, LinearLayout.LayoutParams(0, dp(56), 1f))
        connectionButton = button(R.string.connect) {
            if (latest.session?.connection?.transport == MobileConnectionTransportStateDto.CONNECTED ||
                latest.session?.connection?.transport == MobileConnectionTransportStateDto.CONNECTING) client.disconnect()
            else showPage(R.string.devices)
        }
        header.addView(connectionButton)
        ride.addView(header)
        val hero = column().apply { gravity = Gravity.CENTER }
        speed = text(getString(R.string.unavailable), 76f, true).apply { gravity = Gravity.CENTER }
        hero.addView(speed)
        hero.addView(text(getString(R.string.speed_unit), 18f).apply { gravity = Gravity.CENTER; setTextColor(muted) })
        ride.addView(hero, LinearLayout.LayoutParams(-1, 0, 1f))
        headroom = text(getString(R.string.unavailable), 18f, true)
        ride.addView(headroom, LinearLayout.LayoutParams(-1, dp(48)))
        val metrics = column()
        val first = row(); val second = row()
        battery = metric(first, R.string.battery)
        voltage = metric(first, R.string.voltage)
        power = metric(second, R.string.power)
        temperature = metric(second, R.string.temperature)
        metrics.addView(first); metrics.addView(second)
        ride.addView(metrics)
        content.addView(ride)
    }

    private fun buildDevices() {
        devicePage = column()
        devicePage.addView(text(getString(R.string.nearby_devices), 26f, true), LinearLayout.LayoutParams(-1, dp(60)))
        permissionDetail = text(getString(R.string.find_pev_detail), 16f).apply { setTextColor(muted) }
        devicePage.addView(permissionDetail)
        scanButton = button(R.string.scan) { scanOrRequestPermission() }
        devicePage.addView(scanButton)
        settingsButton = button(R.string.open_settings) {
            startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
        }
        devicePage.addView(settingsButton)
        devicesList = column()
        devicePage.addView(ScrollView(this).apply { addView(devicesList) }, LinearLayout.LayoutParams(-1, 0, 1f))
        content.addView(devicePage.apply { tag = R.string.devices })
    }

    private fun scanOrRequestPermission() {
        if (!client.hasPermissions()) {
            requestPermissions(arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT), 1)
        } else if (!client.isEnabled()) {
            startActivity(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE))
        } else if (latest.scanning) client.stopScan()
        else { scanned = true; client.scan() }
    }

    private fun render(value: BlePresentation) {
        latest = value
        val readiness = value.session?.connection?.readiness
        status.text = getString(when {
            value.problem -> R.string.connection_failed
            readiness == MobileConnectionReadinessDto.VERIFIED -> R.string.connected
            readiness == MobileConnectionReadinessDto.RECORD_ONLY -> R.string.record_only
            readiness == MobileConnectionReadinessDto.CONFLICTED -> R.string.connection_conflict
            readiness == MobileConnectionReadinessDto.FAILED -> R.string.connection_failed
            value.phase == NativePhase.CONNECTING -> R.string.connecting
            value.phase == NativePhase.DISCOVERING -> R.string.discovering
            value.phase == NativePhase.IDENTIFYING -> R.string.identifying
            else -> R.string.disconnected
        })
        status.setTextColor(if (readiness == MobileConnectionReadinessDto.VERIFIED) accent else muted)
        deviceName.text = value.session?.identity?.model
            ?: value.session?.connection?.token?.platformIdentifier?.let { id -> value.devices.firstOrNull { it.id == id }?.name }
            ?: getString(R.string.find_pev)
        val active = value.session?.connection?.transport?.let { it != MobileConnectionTransportStateDto.DISCONNECTED } == true
        connectionButton.setText(if (active) R.string.disconnect else R.string.connect)
        val snapshot = value.telemetry.takeIf { readiness == MobileConnectionReadinessDto.VERIFIED }
        speed.text = snapshot?.speed?.let { format(it.value.value.toDouble() * 0.0036, 1) } ?: getString(R.string.unavailable)
        val dashboard = snapshot?.let(::mobileRiderDashboardProjection)
        val projection = dashboard?.dashboardMetrics.orEmpty()
        val pwm = dashboard?.safetyMetrics?.filterIsInstance<MobileRiderSafetyMetricDescriptorDto.PwmHeadroom>()
            ?.firstOrNull()?.value as? MobileRiderHeadroomValueDto.Available
        headroom.text = getString(R.string.pwm_headroom) + "   " +
            (pwm?.let { format(it.permille.toDouble() / 10, 0) + "%" } ?: getString(R.string.unavailable))
        val level = projection.filterIsInstance<MobileRiderDashboardMetricDescriptorDto.BatteryLevel>().firstOrNull()?.reading
        battery.text = level?.let { (if (it.source == MobileValueSourceDto.ESTIMATED) "≈" else "") + "${it.value.value}%" }
            ?: getString(R.string.unavailable)
        voltage.text = projection.filterIsInstance<MobileRiderDashboardMetricDescriptorDto.PackVoltage>().firstOrNull()?.voltage
            ?.let { format(it.value.toDouble() / 1000, 1) + " V" } ?: getString(R.string.unavailable)
        power.text = projection.filterIsInstance<MobileRiderDashboardMetricDescriptorDto.Power>().firstOrNull()?.power
            ?.let { format(it.value.toDouble() / 1_000_000, 2) + " kW" } ?: getString(R.string.unavailable)
        temperature.text = projection.filterIsInstance<MobileRiderDashboardMetricDescriptorDto.Thermal>().firstOrNull()?.readback
            ?.let { format(it.maximum.value.toDouble() / 1000, 0) + " °C" } ?: getString(R.string.unavailable)
        renderDevices(value)
        renderConfiguration(value.settings)
    }

    private fun renderDevices(value: BlePresentation) {
        val permitted = client.hasPermissions()
        settingsButton.visibility = if (permitted) View.GONE else View.VISIBLE
        permissionDetail.setText(when {
            !permitted -> R.string.permission_detail
            !client.isEnabled() -> R.string.bluetooth_off
            value.scanning -> R.string.scanning
            scanned && value.devices.isEmpty() -> R.string.scan_empty
            else -> R.string.find_pev_detail
        })
        scanButton.setText(when {
            !permitted -> R.string.allow_bluetooth
            !client.isEnabled() -> R.string.enable_bluetooth
            value.scanning -> R.string.stop_scan
            else -> R.string.scan
        })
        val signature = value.devices.toString()
        if (signature == devicesSignature) return
        devicesSignature = signature
        devicesList.removeAllViews()
        value.devices.forEach { device ->
            devicesList.addView(buttonText("${device.name}\n${getString(R.string.signal, device.rssi)}") {
                client.connect(device.id); showPage(R.string.ride)
            })
        }
    }

    private fun renderConfiguration(settings: MobileSettingsDto?) {
        val signature = settings?.let { "${it.connection.token}:${it.connection.readiness}:" +
            it.settingDescriptors.toString() + it.settings.map { row -> "${row.id}:${row.current}:${row.status}:${row.refusal}" } }
            ?: "none"
        if (signature == configurationSignature) return
        configurationSignature = signature
        configuration.removeAllViews()
        configuration.addView(text(getString(R.string.configuration), 26f, true), LinearLayout.LayoutParams(-1, dp(60)))
        if (settings == null || settings.settingDescriptors.isEmpty()) {
            configuration.addView(text(getString(R.string.no_settings), 17f))
            return
        }
        for (descriptor in settings.settingDescriptors.sortedBy { it.order }) {
            val observed = settings.settings.firstOrNull { it.id == descriptor.id }
            val label = label(descriptor.labelKey)
            val detail = when (observed?.status) {
                MobileSettingStatusDto.WAITING_FOR_CONFIRMATION -> getString(R.string.request_waiting)
                MobileSettingStatusDto.SENT_WITHOUT_CONFIRMATION -> getString(R.string.request_sent)
                MobileSettingStatusDto.TIMED_OUT, MobileSettingStatusDto.FAILED,
                MobileSettingStatusDto.REFUSED -> getString(R.string.request_failed)
                else -> settingValue(observed?.current, descriptor.control)
            }
            val row = buttonText("$label\n$detail") { editSetting(settings, descriptor, observed?.current) }
            row.isEnabled = settings.connection.readiness == MobileConnectionReadinessDto.VERIFIED &&
                descriptor.access == MobileSettingAccessDto.WRITABLE && observed?.refusal == null &&
                descriptor.control != MobileSettingControlDto.ReadOnly
            if (!row.isEnabled) row.contentDescription = "$label, $detail, ${getString(R.string.read_only)}"
            configuration.addView(row)
        }
    }

    private fun editSetting(settings: MobileSettingsDto, descriptor: MobileSettingDescriptorDto, value: MobileSettingValueDto?) {
        val token = settings.connection.token ?: return
        fun submit(next: MobileSettingValueDto) = client.submit(token, descriptor.id, next) {
            Toast.makeText(this, R.string.setting_refused, Toast.LENGTH_LONG).show()
        }
        val builder = AlertDialog.Builder(this).setTitle(label(descriptor.labelKey)).setNegativeButton(R.string.cancel, null)
        when (val control = descriptor.control) {
            MobileSettingControlDto.Boolean -> builder.setItems(arrayOf(getString(R.string.on), getString(R.string.off))) { _, choice ->
                submit(MobileSettingValueDto.Boolean(choice == 0))
            }.show()
            is MobileSettingControlDto.Choices -> {
                val choices = control.choices.filter { it.writable }
                builder.setItems(choices.map { label(it.labelKey) }.toTypedArray()) { _, choice ->
                    submit(MobileSettingValueDto.Choice(choices[choice].id))
                }.show()
            }
            is MobileSettingControlDto.Number -> {
                val input = EditText(this).apply {
                    inputType = InputType.TYPE_CLASS_NUMBER or InputType.TYPE_NUMBER_FLAG_DECIMAL or InputType.TYPE_NUMBER_FLAG_SIGNED
                    if (value is MobileSettingValueDto.Number) setText(BigDecimal.valueOf(value.value.toLong(), control.precision.toInt()).toPlainString())
                    setSelectAllOnFocus(true)
                }
                val scale = control.precision.toInt()
                builder.setMessage("${BigDecimal.valueOf(control.minimum.toLong(), scale).toPlainString()} – " +
                    BigDecimal.valueOf(control.maximum.toLong(), scale).toPlainString() + " " + unit(control.unit)).setView(input)
                    .setPositiveButton(R.string.apply, null)
                if (control.canDisable) builder.setNeutralButton(R.string.disabled) { _, _ -> submit(MobileSettingValueDto.Disabled) }
                val dialog = builder.create()
                dialog.setOnShowListener {
                    dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                        val number = try { input.text.toString().trim().replace(',', '.').toBigDecimalOrNull()
                            ?.movePointRight(scale)?.intValueExact() } catch (_: ArithmeticException) { null }
                        if (number == null || number !in control.minimum..control.maximum) input.error = getString(R.string.invalid_number)
                        else { submit(MobileSettingValueDto.Number(number)); dialog.dismiss() }
                    }
                    input.requestFocus()
                    dialog.window?.setSoftInputMode(android.view.WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_VISIBLE)
                }
                dialog.show()
            }
            MobileSettingControlDto.ReadOnly -> Unit
        }
    }

    private fun settingValue(value: MobileSettingValueDto?, control: MobileSettingControlDto): String = when (value) {
        is MobileSettingValueDto.Boolean -> getString(if (value.value) R.string.on else R.string.off)
        is MobileSettingValueDto.Number -> BigDecimal.valueOf(value.value.toLong(),
            (control as? MobileSettingControlDto.Number)?.precision?.toInt() ?: 0).toPlainString() +
                (control as? MobileSettingControlDto.Number)?.unit?.let { " " + unit(it) }.orEmpty()
        is MobileSettingValueDto.Choice -> (control as? MobileSettingControlDto.Choices)?.choices
            ?.firstOrNull { it.id == value.id }?.let { label(it.labelKey) } ?: getString(R.string.setting_unknown)
        MobileSettingValueDto.Disabled -> getString(R.string.disabled)
        null -> getString(R.string.setting_unknown)
    }

    private fun label(key: String): String {
        for (language in listOf(Locale.getDefault().toLanguageTag(), Locale.getDefault().language, "en")) {
            val value = labels.optJSONObject(language)?.optString(key).orEmpty()
            if (value.isNotEmpty()) return value
        }
        return getString(R.string.setting_unknown)
    }

    private fun unit(value: MobileSettingUnitDto): String = when (value) {
        MobileSettingUnitDto.PERCENT, MobileSettingUnitDto.PWM_DUTY_PERCENT -> "%"
        MobileSettingUnitDto.KILOMETRES_PER_HOUR -> "km/h"
        MobileSettingUnitDto.DEGREES -> "°"
        MobileSettingUnitDto.SECONDS -> "s"
        MobileSettingUnitDto.MINUTES -> "min"
        MobileSettingUnitDto.VOLTS -> "V"
        MobileSettingUnitDto.AMPERES -> "A"
        MobileSettingUnitDto.HERTZ -> "Hz"
        MobileSettingUnitDto.DEGREES_PER_SECOND -> "°/s"
        MobileSettingUnitDto.CELSIUS -> "°C"
        MobileSettingUnitDto.WATTS -> "W"
        MobileSettingUnitDto.MILLISECONDS -> "ms"
        MobileSettingUnitDto.REVOLUTIONS_PER_MINUTE -> "rpm"
        MobileSettingUnitDto.MILLIMETRES -> "mm"
        MobileSettingUnitDto.AMP_HOURS -> "Ah"
        MobileSettingUnitDto.LEVEL, MobileSettingUnitDto.SCALAR, MobileSettingUnitDto.RAW -> ""
    }

    private fun showPage(title: Int) {
        page = title
        for (index in 0 until content.childCount) content.getChildAt(index).let { it.visibility = if (it.tag == title) View.VISIBLE else View.GONE }
        navigation.forEach { (id, button) ->
            button.setTextColor(if (id == title) Color.BLACK else Color.WHITE)
            button.backgroundTintList = ColorStateList.valueOf(if (id == title) accent else Color.TRANSPARENT)
        }
    }

    private fun metric(row: LinearLayout, title: Int): TextView {
        val card = column().apply {
            background = rounded(Color.rgb(29, 29, 31)); setPadding(dp(14), dp(10), dp(14), dp(10))
        }
        card.addView(text(getString(title), 15f).apply { setTextColor(muted) })
        val value = text(getString(R.string.unavailable), 25f, true)
        card.addView(value)
        row.addView(card, LinearLayout.LayoutParams(0, -2, 1f).apply { setMargins(dp(3), dp(3), dp(3), dp(3)) })
        return value
    }
    private fun column() = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
    private fun row() = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
    private fun text(value: String, size: Float, bold: Boolean = false) = TextView(this).apply {
        text = value; textSize = size; setTextColor(Color.WHITE)
        if (bold) setTypeface(typeface, Typeface.BOLD)
    }
    private fun button(title: Int, action: () -> Unit) = buttonText(getString(title), action)
    private fun buttonText(title: String, action: () -> Unit) = Button(this).apply {
        text = title; isAllCaps = false; minHeight = dp(48); textSize = 15f
        maxLines = 2; ellipsize = TextUtils.TruncateAt.END
        setOnClickListener { action() }
    }
    private fun rounded(color: Int) = GradientDrawable().apply { setColor(color); cornerRadius = dp(20).toFloat() }
    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()
    private fun format(value: Double, places: Int) = String.format(Locale.getDefault(), "%.${places}f", value)
}
