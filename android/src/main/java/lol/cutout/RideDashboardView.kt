package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.text.SpannableString
import android.text.Spanned
import android.text.style.ForegroundColorSpan
import android.text.style.RelativeSizeSpan
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.accessibility.AccessibilityNodeInfo
import android.text.TextUtils
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.TextView
import uniffi.cutout_mobile_ffi.*
import java.util.Locale

/** A fixed Ride instrument surface backed by Rust-selected dashboard and safety projections. */
class RideDashboardView(
    context: Context,
    private val onDisconnect: () -> Unit,
    private val onChooseDevice: () -> Unit,
) : FrameLayout(context) {
    private val ui = CutoutUi(context)
    private var surfaceKey: String? = null
    private var latestPresentation: BlePresentation? = null
    private var geometryWidth = 0
    private var geometryHeight = 0
    private var identityTitle = ""
    private var statusText = ""
    private var active = false
    private var warningStatus = false
    private var displayKind = DisplayKind.UNKNOWN
    private var details: List<Pair<String, String>> = emptyList()
    private var headingView: TextView? = null
    private var statusView: TextView? = null
    private var statusDot: View? = null
    private var closeView: TextView? = null
    private var emptyStatusView: TextView? = null
    private var speedView: TextView? = null
    private var speedUnitView: TextView? = null
    private var gaugeView: HeadroomGauge? = null
    private var footpadView: TextView? = null
    private var readingValues = mutableMapOf<String, TextView>()
    private var batteryProgress: ProgressBar? = null

    private enum class DisplayKind { EUC, VESC, UNKNOWN }

    init { setBackgroundColor(ui.background) }

    fun render(value: BlePresentation) {
        latestPresentation = value
        val identity = value.session?.identity
        identityTitle = identity?.model
            ?: value.session?.connection?.token?.platformIdentifier?.let { id -> value.devices.firstOrNull { it.id == id }?.name }
            ?: if (identity?.protocol == MobileProtocolFamilyDto.VESC) context.getString(R.string.ride_vesc_default_title)
                else context.getString(R.string.find_pev)
        active = value.session?.connection?.transport?.let { it != MobileConnectionTransportStateDto.DISCONNECTED } == true
        val readiness = value.session?.connection?.readiness
        val snapshot = value.telemetry.takeIf { readiness == MobileConnectionReadinessDto.VERIFIED }
        displayKind = when {
            identity?.vehicleKind == MobileVehicleKindDto.ELECTRIC_UNICYCLE -> DisplayKind.EUC
            identity?.protocol == MobileProtocolFamilyDto.VESC -> DisplayKind.VESC
            else -> DisplayKind.UNKNOWN
        }
        warningStatus = displayKind == DisplayKind.VESC && snapshot != null && vescWarningTitle(snapshot) != null
        statusText = when {
            value.problem || readiness == MobileConnectionReadinessDto.FAILED -> context.getString(R.string.connection_failed)
            readiness == MobileConnectionReadinessDto.VERIFIED && snapshot != null -> when (displayKind) {
                DisplayKind.EUC -> eucStatus(snapshot.operatingState)
                DisplayKind.VESC -> vescStatus(snapshot)
                DisplayKind.UNKNOWN -> context.getString(R.string.connected)
            }
            readiness == MobileConnectionReadinessDto.VERIFIED -> ui.label("euc.status.connected")
            readiness == MobileConnectionReadinessDto.RECORD_ONLY -> context.getString(R.string.record_only)
            readiness == MobileConnectionReadinessDto.CONFLICTED -> context.getString(R.string.connection_conflict)
            value.phase == NativePhase.CONNECTING -> context.getString(R.string.connecting)
            value.phase == NativePhase.DISCOVERING -> context.getString(R.string.discovering)
            value.phase == NativePhase.IDENTIFYING -> context.getString(R.string.identifying)
            else -> context.getString(R.string.disconnected)
        }
        val projection = snapshot?.let(::mobileRiderDashboardProjection)
        val metrics = projection?.dashboardMetrics.orEmpty()
        val battery = metrics.filterIsInstance<MobileRiderDashboardMetricDescriptorDto.BatteryLevel>()
            .firstOrNull()?.reading
        val voltage = metrics.filterIsInstance<MobileRiderDashboardMetricDescriptorDto.PackVoltage>().firstOrNull()?.voltage
        val power = metrics.filterIsInstance<MobileRiderDashboardMetricDescriptorDto.Power>().firstOrNull()?.power
        val thermal = metrics.filterIsInstance<MobileRiderDashboardMetricDescriptorDto.Thermal>().firstOrNull()?.readback
        val headroom = projection?.safetyMetrics.orEmpty()
            .filterIsInstance<MobileRiderSafetyMetricDescriptorDto.PwmHeadroom>().firstOrNull()?.value
        val speed = snapshot?.speed?.let { format(it.value.value.toDouble() * 0.0022369362920544, 1) }
            ?: context.getString(R.string.unavailable)
        val batteryText = battery?.let { (if (it.source == MobileValueSourceDto.ESTIMATED) "≈" else "") + it.value.value }
            ?: context.getString(R.string.unavailable)
        // Swift deliberately shows no range because limp-home distance is not a pace estimate.
        val unavailable = context.getString(R.string.unavailable)
        val voltageText = voltage?.let { format(it.value.toDouble() / 1000, 1) + " V" } ?: unavailable
        val powerText = power?.let { format(it.value.toDouble() / 1_000_000, 2) + " kW" } ?: unavailable
        val thermalText = thermal?.maximum?.let { format(it.value.toDouble() / 1000, 0) + " °C" } ?: unavailable
        val headroomText = (headroom as? MobileRiderHeadroomValueDto.Available)?.let { "${it.permille / 10U}%" } ?: unavailable
        val vescVoltageText = snapshot?.voltage?.value?.value?.let { format(it.toDouble() / 1000, 1) + " V" } ?: unavailable
        val motorCurrentText = snapshot?.motorCurrent?.value?.value?.let { format(it.toDouble() / 1000, 1) + " A" } ?: unavailable
        val boardAngleText = snapshot?.pitch?.value?.value?.let { format(it.toDouble() / 1000, 1) + "°" } ?: unavailable
        val vescTemperatureText = snapshot?.controllerTemperature?.value?.value?.let { format(it.toDouble() / 1000, 1) + " °C" } ?: unavailable
        val footpadText = snapshot?.footpad?.let(::footpadText).orEmpty()
        details = buildList {
            add(context.getString(R.string.speed) to "$speed ${context.getString(R.string.speed_unit)}")
            add(context.getString(R.string.pwm_headroom) to headroomText)
            if (displayKind == DisplayKind.VESC) {
                add(ui.label("vesc.metric.battery_voltage") to vescVoltageText)
                add(ui.label("vesc.metric.motor_current") to motorCurrentText)
                add(ui.label("vesc.metric.board_angle") to boardAngleText)
                add(ui.label("vesc.metric.controller") to vescTemperatureText)
                if (footpadText.isNotEmpty()) add(ui.label("footpad.title") to footpadText)
            }
            metrics.forEach { metric -> when (metric) {
                is MobileRiderDashboardMetricDescriptorDto.BatteryLevel -> add(context.getString(R.string.battery) to "$batteryText%")
                // The standalone Rust charge-estimator DTO is not carried by Android's
                // BlePresentation, so do not fabricate a display value here.
                is MobileRiderDashboardMetricDescriptorDto.ChargeEstimate -> Unit
                is MobileRiderDashboardMetricDescriptorDto.PackVoltage -> add(context.getString(R.string.voltage) to voltageText)
                is MobileRiderDashboardMetricDescriptorDto.Power -> add(context.getString(R.string.power) to powerText)
                is MobileRiderDashboardMetricDescriptorDto.Thermal -> add(context.getString(R.string.temperature) to thermalText)
                is MobileRiderDashboardMetricDescriptorDto.LimpHomeRange -> Unit
            } }
        }
        val showInstruments = snapshot != null || displayKind != DisplayKind.UNKNOWN
        val landscape = geometryWidth > geometryHeight && geometryWidth > 0
        val key = surfaceKey(showInstruments, landscape)
        if (key != surfaceKey) {
            surfaceKey = key
            createSurface(showInstruments, landscape)
        }
        updateSurface(speed, headroom, headroomText, batteryText, voltageText, powerText, thermalText, battery,
            vescVoltageText, motorCurrentText, boardAngleText, vescTemperatureText, footpadText)
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        geometryWidth = MeasureSpec.getSize(widthMeasureSpec)
        geometryHeight = MeasureSpec.getSize(heightMeasureSpec)
        // Rebuild before measuring children, but only when geometry changes the surface.
        // Replacing the hierarchy from onSizeChanged leaves new children unmeasured;
        // rerendering on every measure also needlessly updates the text hierarchy.
        val currentKey = surfaceKey
        val showInstruments = currentKey?.split(':')?.getOrNull(1) == "true"
        val landscape = geometryWidth > geometryHeight && geometryWidth > 0
        if (latestPresentation != null && surfaceKey(showInstruments, landscape) != currentKey) {
            render(checkNotNull(latestPresentation))
        }
        super.onMeasure(widthMeasureSpec, heightMeasureSpec)
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        latestPresentation?.let(::render)
    }

    private fun surfaceKey(showInstruments: Boolean, landscape: Boolean): String {
        val density = resources.displayMetrics.density.coerceAtLeast(1f)
        val widthDp = geometryWidth / density
        val heightDp = geometryHeight / density
        val sizeClass = when {
            widthDp < 320f || heightDp < 440f -> "compact"
            widthDp >= 600f -> "wide"
            else -> "regular"
        }
        val fontScaleClass = when {
            resources.configuration.fontScale >= 1.5f -> "large-text"
            resources.configuration.fontScale >= 1.2f -> "scaled-text"
            else -> "standard-text"
        }
        return "${displayKind.name}:$showInstruments:$landscape:$sizeClass:$fontScaleClass"
    }

    private fun eucStatus(state: RideOperatingState): String = when (state) {
        RideOperatingState.PARKED -> ui.label("euc.status.parked")
        RideOperatingState.STANDING -> ui.label("euc.status.standing")
        RideOperatingState.RIDING -> ui.label("euc.status.riding")
        RideOperatingState.CHARGING -> ui.label("euc.status.charging")
        RideOperatingState.UNKNOWN -> ui.label("euc.status.connected")
    }

    private fun vescStatus(snapshot: MobileTelemetrySnapshotDto): String {
        vescWarningTitle(snapshot)?.let { return it }
        when (snapshot.vescOperatingMode) {
            MobileVescRideOperatingModeDto.DARKRIDE -> return ui.label("vesc.mode.darkride")
            MobileVescRideOperatingModeDto.HANDTEST -> return ui.label("vesc.mode.handtest")
            MobileVescRideOperatingModeDto.FLYWHEEL -> return ui.label("vesc.mode.flywheel")
            else -> Unit
        }
        return when (snapshot.operatingState) {
            RideOperatingState.PARKED -> ui.label("vesc.debug.state.parked")
            RideOperatingState.STANDING -> ui.label("vesc.debug.state.standing")
            RideOperatingState.RIDING -> ui.label("vesc.debug.state.riding")
            RideOperatingState.CHARGING -> ui.label("vesc.debug.state.charging")
            // Swift's VESC screen presents an unknown state as generic connected.
            RideOperatingState.UNKNOWN -> ui.label("euc.status.connected")
        }
    }

    /** Mirrors Swift's warning title selection from the Rust-decoded warning/stop enums. */
    private fun vescWarningTitle(snapshot: MobileTelemetrySnapshotDto): String? = when (snapshot.vescWarning) {
        MobileVescRideWarningDto.LOW_VOLTAGE -> ui.label("vesc.warning.low_voltage")
        MobileVescRideWarningDto.HIGH_VOLTAGE -> ui.label("vesc.warning.high_voltage")
        MobileVescRideWarningDto.MOSFET_TEMPERATURE -> ui.label("vesc.warning.mosfet_temperature")
        MobileVescRideWarningDto.MOTOR_TEMPERATURE -> ui.label("vesc.warning.motor_temperature")
        MobileVescRideWarningDto.CURRENT -> ui.label("vesc.warning.current")
        MobileVescRideWarningDto.DUTY_PUSHBACK, MobileVescRideWarningDto.SPEED_PUSHBACK -> ui.label("vesc.warning.duty_pushback")
        MobileVescRideWarningDto.TEMPERATURE_PUSHBACK -> ui.label("vesc.warning.temperature_pushback")
        MobileVescRideWarningDto.WHEELSLIP -> ui.label("vesc.warning.wheelslip")
        MobileVescRideWarningDto.SENSORS -> ui.label("vesc.warning.sensors")
        MobileVescRideWarningDto.LOW_BATTERY -> ui.label("vesc.warning.low_battery")
        MobileVescRideWarningDto.ERROR, MobileVescRideWarningDto.BMS_CONNECTION -> ui.label("vesc.warning.error")
        MobileVescRideWarningDto.NONE -> when (snapshot.vescStopReason) {
            MobileVescRideStopReasonDto.PITCH -> ui.label("vesc.stop.pitch")
            MobileVescRideStopReasonDto.ROLL -> ui.label("vesc.stop.roll")
            MobileVescRideStopReasonDto.SWITCH_HALF -> ui.label("vesc.stop.switch_half")
            MobileVescRideStopReasonDto.SWITCH_FULL -> ui.label("vesc.stop.switch_full")
            MobileVescRideStopReasonDto.REVERSE -> ui.label("vesc.stop.reverse")
            MobileVescRideStopReasonDto.QUICK_STOP -> ui.label("vesc.stop.quick_stop")
            else -> null
        }
        // A missing/unknown warning is not itself evidence of a stop reason.
        else -> null
    }

    private fun footpadText(footpad: MobileFootpadTelemetryDto): String = when (footpad.contactState) {
        MobileFootpadContactState.NONE -> ui.label("footpad.contact.none")
        MobileFootpadContactState.LEFT -> ui.label("footpad.contact.left")
        MobileFootpadContactState.RIGHT -> ui.label("footpad.contact.right")
        MobileFootpadContactState.BOTH -> ui.label("footpad.contact.both")
        null -> String.format(Locale.getDefault(), ui.label("footpad.state").replace("%lld", "%d"), footpad.state.toInt())
    }

    private fun createSurface(showInstruments: Boolean, landscape: Boolean) {
        removeAllViews()
        readingValues.clear()
        gaugeView = null; batteryProgress = null; footpadView = null
        emptyStatusView = null
        val root = ui.column().apply { setPadding(ui.dp(24), ui.dp(4), ui.dp(24), ui.dp(12)) }
        addView(root, LayoutParams(-1, -1))

        val header = ui.row().apply { isBaselineAligned = true }
        headingView = ui.text(identityTitle, 19f, true).apply {
            maxLines = 1; setSingleLine(true); ellipsize = TextUtils.TruncateAt.END
            contentDescription = identityTitle
        }
        statusDot = View(context).apply {
            background = android.graphics.drawable.GradientDrawable().apply { shape = android.graphics.drawable.GradientDrawable.OVAL; setColor(ui.muted) }
        }
        statusView = ui.text(statusText, 14f, color = ui.muted).apply {
            maxLines = 1; setSingleLine(true); ellipsize = TextUtils.TruncateAt.END
        }
        header.addView(headingView, LinearLayout.LayoutParams(0, -2, 1f))
        header.addView(statusDot, LinearLayout.LayoutParams(ui.dp(6), ui.dp(6)).apply { marginEnd = ui.dp(6) })
        header.addView(statusView, LinearLayout.LayoutParams(0, -2, 0.7f))
        val close = TextView(context).apply {
            text = "×"; textSize = 25f; gravity = Gravity.CENTER
            setTextColor(ui.foreground); contentDescription = context.getString(if (active) R.string.disconnect else R.string.choose_device)
            tag = "dashboard.disconnect"; isClickable = true; isFocusable = true
            accessibilityDelegate = object : View.AccessibilityDelegate() {
                override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
                    super.onInitializeAccessibilityNodeInfo(host, info)
                    info.className = android.widget.Button::class.java.name
                }
            }
            setOnClickListener { if (active) onDisconnect() else onChooseDevice() }
        }
        closeView = close
        header.addView(close, LinearLayout.LayoutParams(ui.dp(48), ui.dp(48)))
        root.addView(header, LinearLayout.LayoutParams(-1, ui.dp(48)))

        if (!showInstruments) {
            val empty = ui.column().apply { gravity = Gravity.CENTER }
            empty.addView(ui.icon(CutoutSymbol.RIDE, ui.muted, 40)); empty.addView(ui.space(12))
            emptyStatusView = ui.text(statusText, 20f, true).also { empty.addView(it) }
            empty.addView(ui.space(18)); empty.addView(ui.action(context.getString(R.string.choose_device), true, onChooseDevice))
            root.addView(empty, LinearLayout.LayoutParams(-1, 0, 1f))
            return
        }
        if (resources.configuration.fontScale >= 1.5f) {
            val scroll = ScrollView(context).apply {
                isFillViewport = false
                descendantFocusability = ViewGroup.FOCUS_AFTER_DESCENDANTS
                tag = "ride.accessible.readings.scroll"
            }
            val readings = ui.column().apply { setPadding(0, ui.dp(12), 0, ui.dp(24)) }
            scroll.addView(readings, FrameLayout.LayoutParams(-1, -2))
            root.addView(scroll, LinearLayout.LayoutParams(-1, 0, 1f))
            createAccessibleReadings(readings)
            return
        }
        when (displayKind) {
            DisplayKind.EUC -> createEuc(root, landscape)
            DisplayKind.VESC -> createVesc(root, landscape)
            DisplayKind.UNKNOWN -> createUnknown(root, landscape)
        }
    }

    /** Large system fonts get the same essential readings in a native scrollable surface. */
    private fun createAccessibleReadings(parent: LinearLayout) {
        parent.addView(speedReading(), LinearLayout.LayoutParams(-1, -2))
        parent.addView(ui.space(12))
        when (displayKind) {
            DisplayKind.EUC -> {
                addAccessibleHeadroom(parent, ui.label("ride.safety.pwm_headroom"))
                parent.addView(ui.divider())
                addAccessibleMetric(parent, "range", context.getString(R.string.range), "— ${context.getString(R.string.distance_unit)}")
                addAccessibleMetric(parent, "battery", context.getString(R.string.battery), "— %")
                parent.addView(ui.divider())
                addAccessibleMetric(parent, "voltage", context.getString(R.string.voltage), "—")
                addAccessibleMetric(parent, "power", context.getString(R.string.power), "—")
                addAccessibleMetric(parent, "temperature", context.getString(R.string.temperature), "—")
            }
            DisplayKind.VESC -> {
                addAccessibleHeadroom(parent, ui.label("vesc.duty_headroom.label"))
                listOf(
                    "vesc_voltage" to ui.label("vesc.metric.battery_voltage"),
                    "motor_current" to ui.label("vesc.metric.motor_current"),
                    "board_angle" to ui.label("vesc.metric.board_angle"),
                    "controller_temperature" to ui.label("vesc.metric.controller"),
                ).forEach { (key, label) -> addAccessibleMetric(parent, key, label, "—") }
                footpadView = ui.text("", 16f, true, ui.muted).apply {
                    maxLines = 2; contentDescription = ui.label("footpad.title"); tag = "vesc.ride.footpad"
                }.also { parent.addView(it, LinearLayout.LayoutParams(-1, -2)) }
            }
            DisplayKind.UNKNOWN -> {
                listOf(
                    "battery" to context.getString(R.string.battery),
                    "voltage" to context.getString(R.string.voltage),
                    "power" to context.getString(R.string.power),
                    "temperature" to context.getString(R.string.temperature),
                ).forEach { (key, label) -> addAccessibleMetric(parent, key, label, "—") }
            }
        }
    }

    private fun addAccessibleMetric(parent: LinearLayout, key: String, label: String, initial: String) {
        val cell = ui.column().apply { setPadding(0, ui.dp(8), 0, ui.dp(8)) }
        cell.addView(ui.text(label, 18f, color = ui.muted))
        val value = ui.text(initial, 30f, true).apply { maxLines = 3; tag = "ride.accessible.metric.$key" }
        readingValues[key] = value
        cell.addView(value, LinearLayout.LayoutParams(-1, -2))
        parent.addView(cell, LinearLayout.LayoutParams(-1, -2))
    }

    private fun addAccessibleHeadroom(parent: LinearLayout, label: String) {
        val cell = ui.column().apply { setPadding(0, ui.dp(8), 0, ui.dp(8)) }
        cell.addView(ui.text(label, 18f, color = ui.muted))
        val value = ui.text("—", 30f, true).apply { maxLines = 2; tag = "ride.accessible.metric.headroom" }
        readingValues["headroom"] = value
        cell.addView(value)
        gaugeView = HeadroomGauge(ui, barOnly = true).apply {
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }.also { cell.addView(it, LinearLayout.LayoutParams(-1, ui.dp(16))) }
        parent.addView(cell, LinearLayout.LayoutParams(-1, -2))
    }

    private fun createEuc(root: LinearLayout, landscape: Boolean) {
        val contentWidth = (geometryWidth - ui.dp(48)).coerceAtLeast(ui.dp(160))
        if (landscape) {
            val row = ui.row().apply { gravity = Gravity.CENTER }
            val speed = speedReading((contentWidth * 0.22f).toInt() - ui.dp(44))
            row.addView(speed, LinearLayout.LayoutParams(0, -1, 1f))
            row.addView(createGauge(), LinearLayout.LayoutParams(0, -1, 1f))
            val side = ui.column()
            side.addView(ui.divider()); addEnergyRow(side)
            side.addView(ui.divider()); addSecondary(side)
            row.addView(side, LinearLayout.LayoutParams(0, -1, 1.1f))
            root.addView(row, LinearLayout.LayoutParams(-1, 0, 1f))
        } else {
            val primary = ui.column().apply { gravity = Gravity.CENTER }
            val speed = speedReading((contentWidth * 0.44f).toInt() - ui.dp(44))
            primary.addView(speed, LinearLayout.LayoutParams(-1, 0, 0.42f))
            primary.addView(ui.space(4))
            primary.addView(createGauge(), LinearLayout.LayoutParams(-1, 0, 0.58f))
            root.addView(primary, LinearLayout.LayoutParams(-1, 0, 1f))
            root.addView(ui.divider()); addEnergyRow(root); root.addView(ui.divider()); addSecondary(root)
        }
    }

    private fun createVesc(root: LinearLayout, landscape: Boolean) {
        val hero = ui.column().apply { gravity = Gravity.CENTER }
        hero.addView(speedReading()); hero.addView(ui.space(14)); addHeadroomBar(hero, ui.label("vesc.duty_headroom.label"))
        val grid = vescMetricGrid()
        footpadView = ui.text("", 13f, true, ui.muted).apply {
            gravity = Gravity.CENTER; maxLines = 1; setSingleLine(true)
            contentDescription = ui.label("footpad.title")
            tag = "vesc.ride.footpad"
        }
        if (landscape) {
            root.addView(ui.row().apply {
                addView(hero, LinearLayout.LayoutParams(0, -1, 1f)); addView(grid, LinearLayout.LayoutParams(0, -1, 1.3f))
            }, LinearLayout.LayoutParams(-1, 0, 1f))
        } else {
            root.addView(hero, LinearLayout.LayoutParams(-1, 0, 0.8f)); root.addView(grid, LinearLayout.LayoutParams(-1, 0, 1.2f))
        }
        root.addView(footpadView, LinearLayout.LayoutParams(-1, ui.dp(24)))
    }

    private fun createUnknown(root: LinearLayout, landscape: Boolean) {
        val hero = ui.column().apply { gravity = Gravity.CENTER }
        hero.addView(speedReading())
        if (landscape) root.addView(ui.row().apply {
            addView(hero, LinearLayout.LayoutParams(0, -1, 1f)); addView(metricGrid(), LinearLayout.LayoutParams(0, -1, 1f))
        }, LinearLayout.LayoutParams(-1, 0, 1f))
        else { root.addView(hero, LinearLayout.LayoutParams(-1, 0, 0.7f)); root.addView(metricGrid(), LinearLayout.LayoutParams(-1, 0, 1.3f)) }
    }

    private fun speedReading(maxSpeedWidth: Int = ui.dp(280)): LinearLayout = ui.row().apply {
        gravity = Gravity.CENTER
        val readout = ui.row().apply { gravity = Gravity.CENTER_HORIZONTAL or Gravity.TOP; isBaselineAligned = true }
        val speed = ui.text(context.getString(R.string.unavailable), 100f, true).apply {
            setSingleLine(true); maxLines = 1; maxWidth = maxSpeedWidth.coerceAtLeast(ui.dp(80))
            setAutoSizeTextTypeUniformWithConfiguration(28, 170, 1, android.util.TypedValue.COMPLEX_UNIT_SP)
        }
        speedView = speed
        speed.tag = "ride.hero.speed"
        readout.addView(speed, LinearLayout.LayoutParams(-2, -2))
        val unit = ui.text(" ${context.getString(R.string.speed_unit)}", 18f, true, ui.muted)
        speedUnitView = unit
        readout.addView(unit)
        addView(readout, LinearLayout.LayoutParams(-2, -2))
    }

    private fun createGauge(): View {
        val column = ui.column().apply { gravity = Gravity.CENTER }
        ui.text(context.getString(R.string.pwm_headroom), 20f, true).apply {
            gravity = Gravity.CENTER; maxLines = 1; setSingleLine(true)
            minimumHeight = ui.dp(32)
            tag = "euc.ride.headroom.label"
            setAutoSizeTextTypeUniformWithConfiguration(14, 24, 1, android.util.TypedValue.COMPLEX_UNIT_SP)
        }.also { column.addView(it, LinearLayout.LayoutParams(-1, -2)) }
        column.addView(ui.space(12))
        gaugeView = HeadroomGauge(ui).apply {
            contentDescription = context.getString(R.string.pwm_headroom)
            tag = "euc.ride.headroom"
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
        }.also { column.addView(it, LinearLayout.LayoutParams(-1, 0, 1f)) }
        return column
    }

    private fun addEnergyRow(parent: LinearLayout) {
        val row = ui.row().apply { setPadding(0, ui.dp(14), 0, ui.dp(14)) }
        addInstrument(row, "range", context.getString(R.string.range), "—", context.getString(R.string.distance_unit), 52f, 1f)
        val batteryCell = ui.column().apply {
            gravity = Gravity.END; isClickable = true; isFocusable = true; contentDescription = context.getString(R.string.battery)
            tag = "ride.readings.open"
            setOnClickListener { showDetails() }
        }
        addInstrument(batteryCell, "battery", context.getString(R.string.battery) + "  ›", "—", "%", 52f, 0f, Gravity.END)
        batteryProgress = ProgressBar(context, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = 100; progressTintList = android.content.res.ColorStateList.valueOf(ui.foreground)
            progressBackgroundTintList = android.content.res.ColorStateList.valueOf(ui.stroke)
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }.also { batteryCell.addView(it, LinearLayout.LayoutParams(ui.dp(93), ui.dp(4))) }
        row.addView(batteryCell, LinearLayout.LayoutParams(0, -2, 1f))
        parent.addView(row)
    }

    private fun addSecondary(parent: LinearLayout) {
        val row = ui.row().apply { gravity = Gravity.CENTER_VERTICAL }
        addInstrument(row, "voltage", context.getString(R.string.voltage), "—", "", 23f, 1f)
        addInstrument(row, "power", context.getString(R.string.power), "—", "", 23f, 1f, Gravity.CENTER_HORIZONTAL)
        addInstrument(row, "temperature", context.getString(R.string.temperature), "—", "", 23f, 1f, Gravity.END)
        parent.addView(row, LinearLayout.LayoutParams(-1, -2))
    }

    private fun addHeadroomBar(parent: LinearLayout, label: String) {
        parent.addView(ui.row().apply {
            addView(ui.text(label, 13f, color = ui.muted), LinearLayout.LayoutParams(0, -2, 1f))
            val value = ui.text("—", 14f, true)
            readingValues["headroom"] = value
            addView(value)
        })
        parent.addView(ui.space(6)); gaugeView = HeadroomGauge(ui, barOnly = true).also {
            parent.addView(it, LinearLayout.LayoutParams(-1, ui.dp(12)))
        }
    }

    private fun metricGrid(): LinearLayout = ui.column().apply {
        listOf("battery" to context.getString(R.string.battery), "voltage" to context.getString(R.string.voltage),
            "power" to context.getString(R.string.power), "temperature" to context.getString(R.string.temperature))
            .chunked(2).forEach { pair ->
                addView(ui.row().apply { pair.forEach { (key, label) -> addInstrument(this, key, label, "—", "", 20f, 1f, carded = true) } },
                    LinearLayout.LayoutParams(-1, 0, 1f))
            }
    }

    private fun vescMetricGrid(): LinearLayout = ui.column().apply {
        listOf("vesc_voltage" to ui.label("vesc.metric.battery_voltage"),
            "motor_current" to ui.label("vesc.metric.motor_current"),
            "board_angle" to ui.label("vesc.metric.board_angle"),
            "controller_temperature" to ui.label("vesc.metric.controller"))
            .chunked(2).forEach { pair ->
                addView(ui.row().apply {
                    pair.forEach { (key, label) -> addInstrument(this, key, label, "—", "", 20f, 1f, carded = true) }
                }, LinearLayout.LayoutParams(-1, 0, 1f))
            }
    }

    private fun addInstrument(parent: LinearLayout, key: String, label: String, initial: String, unit: String,
        size: Float, weight: Float, alignment: Int = Gravity.START, carded: Boolean = false) {
        val cell = ui.column().apply {
            gravity = alignment
            if (carded) { background = ui.rounded(16); setPadding(ui.dp(10), ui.dp(6), ui.dp(10), ui.dp(6)) }
        }
        cell.addView(ui.text(label, 13f, color = ui.muted).apply {
            maxLines = 1; setSingleLine(true); gravity = alignment
        })
        val valueRow = ui.row().apply { isBaselineAligned = true; gravity = alignment or Gravity.TOP }
        val value = ui.text(initial, size, true).apply {
            setSingleLine(true); maxLines = 1
            setAutoSizeTextTypeUniformWithConfiguration(16, size.toInt().coerceAtLeast(16), 1, android.util.TypedValue.COMPLEX_UNIT_SP)
        }
        readingValues[key] = value
        valueRow.addView(value)
        if (unit.isNotEmpty()) valueRow.addView(ui.text(" $unit", if (size > 30) 16f else 12f, color = ui.muted))
        cell.addView(valueRow)
        val horizontal = parent.orientation == LinearLayout.HORIZONTAL
        parent.addView(cell, LinearLayout.LayoutParams(if (horizontal) 0 else -1,
            if (carded) -1 else -2, if (horizontal) weight else 0f).apply {
            if (carded) setMargins(ui.dp(4), ui.dp(4), ui.dp(4), ui.dp(4))
        })
    }

    private fun updateSurface(speed: String, headroom: MobileRiderHeadroomValueDto?, headroomText: String,
        battery: String, voltage: String, power: String, temperature: String, batteryReading: BatteryLevelReading?,
        vescVoltage: String, motorCurrent: String, boardAngle: String, vescTemperature: String, footpad: String) {
        headingView?.text = identityTitle
        headingView?.contentDescription = identityTitle
        statusView?.text = statusText
        emptyStatusView?.text = statusText
        closeView?.contentDescription = context.getString(if (active) R.string.disconnect else R.string.choose_device)
        statusDot?.background = android.graphics.drawable.GradientDrawable().apply {
            shape = android.graphics.drawable.GradientDrawable.OVAL
            setColor(if (warningStatus || !active) ui.orange else ui.muted)
        }
        statusView?.contentDescription = statusText
        speedView?.text = speed
        val percent = (headroom as? MobileRiderHeadroomValueDto.Available)?.permille?.toInt()
        gaugeView?.update(headroom, percent, headroomText)
        readingValues["headroom"]?.text = headroomText
        val accessibleText = resources.configuration.fontScale >= 1.5f
        readingValues["range"]?.text = if (accessibleText) "${context.getString(R.string.unavailable)} ${context.getString(R.string.distance_unit)}"
            else context.getString(R.string.unavailable)
        readingValues["battery"]?.text = if (accessibleText) "$battery %" else battery
        setQuantity("voltage", voltage)
        setQuantity("power", power)
        setQuantity("temperature", temperature)
        setQuantity("vesc_voltage", vescVoltage)
        setQuantity("motor_current", motorCurrent)
        setQuantity("board_angle", boardAngle)
        setQuantity("controller_temperature", vescTemperature)
        footpadView?.text = footpad
        footpadView?.contentDescription = if (footpad.isEmpty()) ui.label("footpad.title") else "${ui.label("footpad.title")}: $footpad"
        batteryProgress?.progress = batteryReading?.value?.value?.toInt()?.coerceIn(0, 100) ?: 0
        val batteryCell = readingValues["battery"]?.parent?.parent as? View
        batteryCell?.contentDescription = if (batteryReading == null) context.getString(R.string.battery)
            else "${context.getString(R.string.battery)} $battery%"
    }

    private fun setQuantity(key: String, value: String) {
        val boundary = value.lastIndexOf(' ')
        readingValues[key]?.text = if (boundary < 0) value else SpannableString(value).apply {
            setSpan(RelativeSizeSpan(0.6f), boundary + 1, length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            setSpan(ForegroundColorSpan(ui.muted), boundary + 1, length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
    }

    private fun showDetails() {
        if (details.isEmpty()) return
        AlertDialog.Builder(context).setTitle(context.getString(R.string.ride_readings))
            .setMessage(details.joinToString("\n") { "${it.first}    ${it.second}" })
            .setPositiveButton(context.getString(R.string.done), null).show()
    }

    private fun format(value: Double, digits: Int): String = String.format(Locale.getDefault(), "%.$digits" + "f", value)
}

private class HeadroomGauge(private val ui: CutoutUi, private val barOnly: Boolean = false) : View(ui.context) {
    private var value: MobileRiderHeadroomValueDto? = null
    private var percentage: Int? = null
    private var display = "—"
    private val track = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; color = ui.stroke }
    private val accent = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE; strokeCap = Paint.Cap.ROUND; color = if (ui.dark) ui.yellow else ui.foreground }
    private val primary = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = accent.color; textAlign = Paint.Align.CENTER; typeface = android.graphics.Typeface.create("sans-serif", android.graphics.Typeface.BOLD) }
    private val unit = Paint(primary)
    private val endpoint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = ui.muted; textAlign = Paint.Align.CENTER; typeface = android.graphics.Typeface.create("sans-serif-medium", 0) }

    fun update(value: MobileRiderHeadroomValueDto?, percentage: Int?, display: String) {
        this.value = value; this.percentage = percentage; this.display = display
        contentDescription = "${context.getString(R.string.pwm_headroom)} $display"
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        if (barOnly) {
            val y = height / 2f; track.strokeWidth = ui.dp(8).toFloat(); accent.strokeWidth = track.strokeWidth
            canvas.drawLine(0f, y, width.toFloat(), y, track)
            percentage?.let { canvas.drawLine(0f, y, width * (it / 1000f).coerceIn(0f, 1f), y, accent) }
            return
        }
        // Match Swift's 0..100 semicircle while fitting the fixed area at large font scales.
        val line = ui.dp(10).toFloat()
        // Swift sizes the arc at min(360pt, availableWidth - 12pt,
        // 2 * (gaugeHeight - 28pt)); its surrounding title and endpoint row
        // consume the remaining 60pt in the parent gauge geometry.
        val diameter = (minOf(ui.dp(360).toFloat(), width.toFloat(), (height - ui.dp(28)) * 2f) - ui.dp(12))
            .coerceAtLeast(0f)
        if (diameter <= line * 2) return
        val centerY = height - ui.dp(24).toFloat()
        val rect = RectF((width - diameter) / 2f, centerY - diameter / 2f, (width + diameter) / 2f, centerY + diameter / 2f)
        track.strokeWidth = line; accent.strokeWidth = line
        canvas.drawArc(rect, 180f, 180f, false, track)
        percentage?.let { canvas.drawArc(rect, 180f, 180f * (it / 1000f).coerceIn(0f, 1f), false, accent) }
        val size = minOf(ui.dp(84).toFloat(), width * 0.23f, height * 0.42f).coerceAtLeast(ui.dp(22).toFloat())
        primary.textSize = minOf(size * resources.displayMetrics.scaledDensity / resources.displayMetrics.density,
            width * 0.30f)
        endpoint.textSize = 12f * resources.displayMetrics.scaledDensity
        val baseline = centerY + ui.dp(3).toFloat()
        if (percentage == null) canvas.drawText(display, width / 2f, baseline, primary)
        else {
            val number = display.removeSuffix("%")
            unit.textSize = primary.textSize * 0.4f
            val numberWidth = primary.measureText(number)
            val unitWidth = unit.measureText("%")
            val gap = ui.dp(4).toFloat()
            val left = (width - numberWidth - unitWidth - gap) / 2f
            canvas.drawText(number, left + numberWidth / 2f, baseline, primary)
            canvas.drawText("%", left + numberWidth + gap + unitWidth / 2f, baseline, unit)
        }
        val endBaseline = centerY + ui.dp(16).toFloat()
        canvas.drawText("0", rect.left, endBaseline, endpoint)
        canvas.drawText("100", rect.right, endBaseline, endpoint)
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
    }
}
