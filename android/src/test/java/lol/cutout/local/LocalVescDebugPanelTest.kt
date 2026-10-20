package lol.cutout.local

import android.app.Activity
import android.view.View
import android.widget.TextView
import lol.cutout.VescDebugPanel
import lol.cutout.R
import lol.cutout.DiagnosticCapturePresentation
import lol.cutout.CutoutUi
import lol.cutout.ffitest.productionReplayPresentation
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.*

/** DTO-shaped UI fixture; protocol admission is covered independently by Rust replay tests. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalVescDebugPanelTest {
    @Test
    fun admittedReadingsUpdateInPlaceAndDisappearWhenConnectionIsUnverified() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        try {
            val replay = productionReplayPresentation()
            val connection = checkNotNull(replay.session).connection
            val identity = checkNotNull(replay.session?.identity).copy(
                protocol = MobileProtocolFamilyDto.VESC, vehicleKind = MobileVehicleKindDto.ONEWHEEL,
                vescApplication = MobileVescApplicationDiscoveryDto.REFLOAT, model = "VESC test fixture")
            val fixture = replay.copy(session = MobileDeviceSessionSnapshotDto(connection, identity))
            val panel = VescDebugPanel(activity)
            activity.setContentView(panel)
            panel.render(fixture)
            val voltage = checkNotNull(panel.findViewWithTag<TextView>("vesc.debug.row.pack_voltage"))
            val original = voltage.text.toString()
            assertNotEquals("—", original)
            val telemetry = checkNotNull(fixture.telemetry)
            val reading = checkNotNull(telemetry.voltage)
            panel.render(fixture.copy(telemetry = telemetry.copy(voltage = reading.copy(value = Voltage(123_400)))))
            assertSame(voltage, panel.findViewWithTag<View>("vesc.debug.row.pack_voltage"))
            assertNotEquals(original, voltage.text.toString())
            NativeSurfaceCapture.save(panel, "vesc-debug-dark")
            panel.render(fixture.copy(session = fixture.session!!.copy(connection = connection.copy(readiness = MobileConnectionReadinessDto.PENDING))))
            assertFalse(voltage.isShown)
        } finally { host.pause().stop().destroy() }
    }

    @Test
    fun captureStatusTakesSubtitlePriorityAndConnectionPhaseRemainsFallback() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        try {
            val replay = productionReplayPresentation()
            val connection = checkNotNull(replay.session).connection
            val identity = checkNotNull(replay.session?.identity).copy(
                protocol = MobileProtocolFamilyDto.VESC, vehicleKind = MobileVehicleKindDto.ONEWHEEL,
                vescApplication = MobileVescApplicationDiscoveryDto.REFLOAT, model = "VESC test fixture")
            val fixture = replay.copy(session = MobileDeviceSessionSnapshotDto(connection, identity))
            val panel = VescDebugPanel(activity)
            activity.setContentView(panel)
            val labels = CutoutUi(activity)
            val capture = DiagnosticCapturePresentation(
                lifecycle = MobileCaptureLifecycleSnapshotDto(
                    MobileCaptureAttemptDto(MobileCaptureGenerationDto(1uL), MobileCaptureOriginDto.MANUAL,
                        MobileCaptureStageDto.RECORDING),
                    canStart = false,
                    canPair = true,
                ),
                notificationCount = 3UL,
            )
            panel.render(fixture, capture)
            val subtitle = checkNotNull(panel.findViewWithTag<TextView>("vesc.debug.subtitle"))
            assertEquals(labels.label("captures.receiving"), subtitle.text.toString())
            panel.render(fixture, null)
            assertEquals(activity.getString(R.string.connected), subtitle.text.toString())
        } finally { host.pause().stop().destroy() }
    }
}
