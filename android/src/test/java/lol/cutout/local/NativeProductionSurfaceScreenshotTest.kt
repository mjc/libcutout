package lol.cutout.local

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
import uniffi.cutout_mobile_ffi.mobileDiscoveryCandidateFromAdvertisement
import java.io.File
import java.util.UUID

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativeProductionSurfaceScreenshotTest {
    @Test
    @Config(qualifiers = "w393dp-h852dp-xhdpi-night")
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
            val context = configuredContext(app, dark)
            val panel = DevicePickerPanel(context, {}, {}, {}, {})
            panel.render(BlePresentation(devices = devices), permitted = true, enabled = true, hasScanned = true)
            NativeSurfaceCapture.save(panel, "picker-${mode(dark)}")

            val ride = RideDashboardView(context, {}, {})
            ride.render(replay)
            NativeSurfaceCapture.save(ride, "ride-${mode(dark)}")

            val configuration = SettingsPanel(context) { _, _, _, _ -> }
            configuration.render(settings)
            NativeSurfaceCapture.save(configuration, "settings-${mode(dark)}")
        }
    }

    private fun configuredContext(context: Context, dark: Boolean): Context {
        val configuration = Configuration(context.resources.configuration)
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

private object NativeSurfaceCapture {
    private const val WIDTH_DP = 393
    private const val HEIGHT_DP = 852

    fun save(view: View, name: String) {
        val density = view.resources.displayMetrics.density
        val width = (WIDTH_DP * density).toInt()
        val height = (HEIGHT_DP * density).toInt()
        view.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
        )
        view.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        try {
            view.draw(Canvas(bitmap))
            val background = bitmap.getPixel(0, 0)
            val visibleSamples = (0 until height step 8).sumOf { y ->
                (0 until width step 8).count { x -> bitmap.getPixel(x, y) != background }
            }
            check(visibleSamples > 32) { "$name rendered a blank surface on its first layout" }
            val output = File(System.getProperty("cutout.ui.artifacts") ?: "build/test-artifacts/ui")
            check(output.isDirectory || output.mkdirs()) { "Could not create UI artifact directory: $output" }
            File(output, "$name.png").outputStream().use { stream ->
                check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, stream)) { "Could not encode $name.png" }
            }
        } finally {
            bitmap.recycle()
        }
    }
}
