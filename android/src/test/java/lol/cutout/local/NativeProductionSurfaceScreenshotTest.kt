package lol.cutout.local

import android.app.Activity
import android.content.Context
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.View
import lol.cutout.BlePresentation
import lol.cutout.DevicePickerPanel
import lol.cutout.NearbyPev
import lol.cutout.RideDashboardView
import lol.cutout.SettingsPanel
import lol.cutout.CutoutApplication
import lol.cutout.CutoutUi
import lol.cutout.MainActivity
import lol.cutout.R
import lol.cutout.ffitest.productionReplayPresentation
import lol.cutout.wireBytes
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Robolectric
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.DiscoveryServiceUuid
import uniffi.cutout_mobile_ffi.MobileProtocolFamilyDto
import uniffi.cutout_mobile_ffi.MobileVehicleKindDto
import uniffi.cutout_mobile_ffi.MobileVehicleClassificationProvenanceDto
import uniffi.cutout_mobile_ffi.MobileVehicleClassificationRequirementDto
import uniffi.cutout_mobile_ffi.MobileVescApplicationDiscoveryDto
import uniffi.cutout_mobile_ffi.MobileVescDrivetrainContextDto
import uniffi.cutout_mobile_ffi.MobileVescDrivetrainSourceDto
import uniffi.cutout_mobile_ffi.MobileVescDrivetrainTargetDto
import uniffi.cutout_mobile_ffi.MobileValueSourceDto
import uniffi.cutout_mobile_ffi.RideOperatingState
import uniffi.cutout_mobile_ffi.Speed
import uniffi.cutout_mobile_ffi.Voltage
import uniffi.cutout_mobile_ffi.mobileDiscoveryCandidateFromAdvertisement
import java.io.File
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativeProductionSurfaceScreenshotTest {
    @Test
    @Config(qualifiers = "w393dp-h852dp-night-xhdpi")
    fun rendersConnectedRideWithProductionNavigation() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        try {
            (activity.application as CutoutApplication).bluetooth.observe(null)
            (activity.application as CutoutApplication).bluetooth.stopScan()
            activity.render(productionReplayPresentation())
            val navigation = descendants(activity.window.decorView).single {
                it.isClickable && it.contentDescription?.toString() == activity.getString(R.string.ride)
            }
            check(navigation.performClick())
            NativeSurfaceCapture.save(activity.window.decorView, "app-ride-dark")
            val tune = descendants(activity.window.decorView).single {
                it.isClickable && it.contentDescription?.toString() == activity.getString(R.string.configuration)
            }
            check(tune.performClick())
            NativeSurfaceCapture.save(activity.window.decorView, "app-settings-dark")
        } finally {
            controller.pause().stop().destroy()
        }
    }

    @Test
    fun rendersPickerRideAndSettingsInLightAndDark() {
        val app = RuntimeEnvironment.getApplication()
        val replay = productionReplayPresentation()
        val settings = checkNotNull(replay.settings) { "Rust replay did not produce settings" }
        val devices = listOf(
            device("nordic-ui-fixture", "VESC", "6e400001-b5a3-f393-e0a9-e50e24dcca9e"),
            device("ffe0-ui-fixture", "Wheel", "0000ffe0-0000-1000-8000-00805f9b34fb"),
        )

        for (dark in listOf(false, true)) {
            val controller = Robolectric.buildActivity(Activity::class.java).setup()
            val activity = controller.get()
            try {
                val context = configuredContext(app, dark)
                val panel = DevicePickerPanel(context, {}, {}, {}, {})
                panel.render(BlePresentation(devices = devices), permitted = true, enabled = true, hasScanned = true)
                activity.setContentView(panel)
                controller.visible()
                NativeSurfaceCapture.save(panel, "picker-${mode(dark)}")

                val ride = RideDashboardView(context, {}, {})
                ride.render(replay)
                activity.setContentView(ride)
                NativeSurfaceCapture.save(ride, "ride-${mode(dark)}")

                val configuration = SettingsPanel(context, "surface-fixture-owner") { _, _, _, _ -> }
                configuration.render(settings)
                activity.setContentView(configuration)
                NativeSurfaceCapture.save(configuration, "settings-${mode(dark)}")
            } finally {
                controller.pause().stop().destroy()
            }
        }
    }

    @Test
    @Config(qualifiers = "w393dp-h852dp-night-xhdpi")
    fun rendersMoreAndPackWithoutReplacingLiveReadingRows() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        try {
            (activity.application as CutoutApplication).bluetooth.observe(null)
            (activity.application as CutoutApplication).bluetooth.stopScan()
            val replay = productionReplayPresentation()
            activity.render(replay)
            fun open(label: Int) = descendants(activity.window.decorView).single {
                it.isShown && it.isClickable && it.contentDescription?.toString() == activity.getString(label)
            }.performClick()
            check(open(R.string.more))
            NativeSurfaceCapture.save(activity.window.decorView, "app-more-dark")
            check(open(R.string.pack))
            val tag = "pack.metric.${CutoutUi(activity).label("bms.no_data.pack_voltage")}"
            val voltage = checkNotNull(activity.window.decorView.findViewWithTag<android.widget.TextView>(tag))
            val text = voltage.text.toString()
            check(text != "—")
            activity.render(replay.copy())
            check(activity.window.decorView.findViewWithTag<android.widget.TextView>(tag) === voltage) {
                "Live pack updates must retain the visible reading instead of rebuilding the page"
            }
            check(voltage.text.toString() == text)
            NativeSurfaceCapture.save(activity.window.decorView, "app-pack-dark")
        } finally {
            controller.pause().stop().destroy()
        }
    }

    /** DTO-shaped host presentation captures; these do not exercise Bluetooth or vehicle detection. */
    @Test
    @Config(qualifiers = "w430dp-h932dp-xhdpi")
    fun rendersStandardVescRideAtApprovedViewportInLightDarkAndLargeText() {
        val app = RuntimeEnvironment.getApplication()
        val replay = productionReplayPresentation()
        for (dark in listOf(false, true)) {
            for (fontScale in listOf(1f, 2f)) {
                val controller = Robolectric.buildActivity(Activity::class.java).setup()
                val activity = controller.get()
                try {
                    val context = configuredContext(app, dark, fontScale)
                    val ride = RideDashboardView(context, {}, {})
                    activity.setContentView(ride)
                    controller.visible()
                    for (knownSpeed in listOf(false, true)) {
                        val presentation = standardVescPresentationFixture(replay, knownSpeed)
                        ride.render(presentation)
                        val state = if (knownSpeed) "known-speed" else "missing-geometry"
                        val textSize = if (fontScale >= 1.5f) "large-text" else "standard-text"
                        NativeSurfaceCapture.save(ride,
                            "host-standard-vesc-ride-$state-${mode(dark)}-$textSize",
                            widthDp = 430, heightDp = 932)
                        val speed = checkNotNull(ride.findViewWithTag<android.widget.TextView>("ride.hero.speed"))
                        check((speed.text.toString() != "--") == knownSpeed) {
                            "$state capture did not display the intended speed availability"
                        }
                        check((presentation.telemetry?.vescDrivetrainContext != null) == knownSpeed)
                        val energyTag = if (fontScale >= 1.5f) "ride.accessible.metric.energy" else "ride.metric.energy"
                        val energy = checkNotNull(ride.findViewWithTag<android.widget.TextView>(energyTag))
                        check(energy.text.toString().endsWith(" V")) { "Standard Ride did not show its pack-voltage fallback" }
                        val energyLabel = descendants(ride).filterIsInstance<android.widget.TextView>().single {
                            it.text.toString() == lol.cutout.CutoutUi(context).label("vesc.metric.pack_voltage")
                        }
                        fun textCenter(view: android.widget.TextView): Float {
                            val position = IntArray(2)
                            view.getLocationOnScreen(position)
                            val text = checkNotNull(view.layout)
                            return position[0] + view.totalPaddingLeft - view.scrollX +
                                (text.getLineLeft(0) + text.getLineRight(0)) / 2f
                        }
                        check(kotlin.math.abs(textCenter(energyLabel) - textCenter(energy)) <= context.resources.displayMetrics.density) {
                            "Pack-voltage label is not centered over its energy value"
                        }
                        check(ride.findViewWithTag<View>("ride.readings.open") != null)
                        check(ride.findViewWithTag<View>("ride.open-tune") != null)
                        check(descendants(ride).none { it.tag == "vesc.ride.footpad" ||
                            it.tag == "euc.ride.headroom" || it.tag == "ride.metric.board_angle" ||
                            it.tag == "ride.accessible.metric.board_angle" || it.tag == "ride.accessible.metric.headroom" }) {
                            "A standard VESC host capture included balance-board instruments"
                        }
                    }
                } finally {
                    controller.pause().stop().destroy()
                }
            }
        }
    }

    /** Copy exported DTO shapes, with explicit standard application/class and local geometry. */
    private fun standardVescPresentationFixture(replay: BlePresentation, knownSpeed: Boolean): BlePresentation {
        val session = checkNotNull(replay.session)
        val telemetry = checkNotNull(replay.telemetry)
        // Same 90 mm / 7 pole-pair / 2.5 reduction example as the Rust drivetrain tests.
        val geometry = if (knownSpeed) MobileVescDrivetrainContextDto(
            target = MobileVescDrivetrainTargetDto.Local,
            connectionAttempt = checkNotNull(session.connection.token), revision = 1UL,
            source = MobileVescDrivetrainSourceDto.USER_CONFIGURED,
            wheelDiameterMicrometres = 90_000u, wheelCircumferenceMm = 283UL,
            motorPolePairs = 7u, motorToWheelRatioMilli = 2_500u,
        ) else null
        return replay.copy(session = session.copy(identity = checkNotNull(session.identity).copy(
            protocol = MobileProtocolFamilyDto.VESC, vehicleKind = MobileVehicleKindDto.ESKATE,
            classificationProvenance = MobileVehicleClassificationProvenanceDto.USER_PROVIDED,
            classificationRequirement = MobileVehicleClassificationRequirementDto.RESOLVED,
            vescApplication = MobileVescApplicationDiscoveryDto.ABSENT, vescApplicationQuery = null, model = null)),
            telemetry = telemetry.copy(
                speed = if (knownSpeed) checkNotNull(telemetry.speed).copy(value = Speed(10_000),
                    source = MobileValueSourceDto.CALCULATED) else null,
                speedObservedAtMs = if (knownSpeed) telemetry.speedObservedAtMs else null,
                vescDrivetrainContext = geometry,
                operatingState = if (knownSpeed) RideOperatingState.RIDING else RideOperatingState.PARKED,
                voltage = checkNotNull(telemetry.voltage).copy(value = Voltage(50_400)),
                batteryLevelReported = null, batteryLevelEstimated = null,
                vescOperatingMode = null, vescWarning = null, vescStopReason = null,
                pitch = null, balanceAngle = null, roll = null, footpad = null, pwm = null))
    }

    private fun configuredContext(context: Context, dark: Boolean, fontScale: Float? = null): Context {
        val configuration = Configuration(context.resources.configuration)
        fontScale?.let { configuration.fontScale = it }
        val nightMask = Configuration.UI_MODE_NIGHT_MASK
        configuration.uiMode = (configuration.uiMode and nightMask.inv()) or
            (if (dark) Configuration.UI_MODE_NIGHT_YES else Configuration.UI_MODE_NIGHT_NO)
        return context.createConfigurationContext(configuration)
    }

    private fun mode(dark: Boolean) = if (dark) "dark" else "light"

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is android.view.ViewGroup) {
            for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
        }
    }

    private fun device(id: String, name: String, serviceUuid: String): NearbyPev {
        val candidate = mobileDiscoveryCandidateFromAdvertisement(
            id,
            name,
            listOf(DiscoveryServiceUuid(UUID.fromString(serviceUuid).wireBytes())),
        )
        return NearbyPev(id, candidate.displayName, -54, candidate.isPickerCandidate, candidate, name)
    }
}

internal object NativeSurfaceCapture {
    private const val WIDTH_DP = 393
    private const val HEIGHT_DP = 852

    fun save(view: View, name: String, widthDp: Int = WIDTH_DP, heightDp: Int = HEIGHT_DP) {
        val density = view.resources.displayMetrics.density
        val width = (widthDp * density).toInt()
        val height = (heightDp * density).toInt()
        var settled = false
        repeat(4) {
            if (!settled) {
                view.measure(
                    View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
                )
                view.layout(0, 0, width, height)
                // TextView finishes scrolling at pre-draw; auto-size can request another layout.
                val cancelled = view.viewTreeObserver.dispatchOnPreDraw()
                settled = !cancelled && !view.isLayoutRequested
            }
        }
        check(settled) { "$name did not settle after four native layout/pre-draw traversals" }
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        try {
            val canvas = Canvas(bitmap)
            canvas.translate(-view.scrollX.toFloat(), -view.scrollY.toFloat())
            view.draw(canvas)
            val background = bitmap.getPixel(0, 0)
            val visibleSamples = (0 until height step 8).sumOf { y ->
                (0 until width step 8).count { x -> bitmap.getPixel(x, y) != background }
            }
            check(visibleSamples > 32) { "$name rendered a blank surface on its first layout" }
            val output = File(System.getProperty("cutout.ui.artifacts") ?: "build/test-artifacts/ui")
            check(output.isDirectory || output.mkdirs()) { "Could not create UI artifact directory: $output" }
            File(output, "$name.layout.txt").writeText(layoutDescription(view))
            File(output, "$name.png").outputStream().use { stream ->
                check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, stream)) { "Could not encode $name.png" }
            }
        } finally {
            bitmap.recycle()
        }
    }

    private fun layoutDescription(view: View, depth: Int = 0): String = buildString {
        append("  ".repeat(depth))
        append("${view.javaClass.simpleName} tag=${view.tag} bounds=${view.left},${view.top},${view.right},${view.bottom}")
        append(" measured=${view.measuredWidth}x${view.measuredHeight} scroll=${view.scrollX},${view.scrollY} visibility=${view.visibility} alpha=${view.alpha}")
        if (view is android.widget.TextView) {
            append(" text=${view.text} size=${view.textSize} color=${view.currentTextColor} gravity=${view.gravity}")
            append(" paintWidth=${view.paint.measureText(view.text.toString())}")
            view.layout?.takeIf { it.lineCount > 0 }?.let {
                append(" line=${it.getLineLeft(0)},${it.getLineWidth(0)},${it.getLineRight(0)} layoutWidth=${it.width}")
            }
        }
        appendLine()
        if (view is android.view.ViewGroup) {
            for (index in 0 until view.childCount) append(layoutDescription(view.getChildAt(index), depth + 1))
        }
    }
}
