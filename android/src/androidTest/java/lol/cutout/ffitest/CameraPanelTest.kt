package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.TextView
import android.app.Activity
import android.content.Intent
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidCameraClient
import lol.cutout.CameraPanel
import lol.cutout.CameraPanelState
import lol.cutout.CameraConnectionState
import lol.cutout.CameraRequestState
import lol.cutout.MainActivity
import org.junit.Test
import uniffi.cutout_mobile_ffi.*

/** Camera UI and codec-boundary checks without a network, camera, or device action. */
open class CameraPanelTest {
    @Test
    fun routeStartsTruthfullyDisconnectedAndDoesNotOfferManualOriginEntry() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        val state = CutoutSessionStateHandle()
        val client = AndroidCameraClient(context, { state })
        try {
            lateinit var panel: CameraPanel
            instrumentation.runOnMainSync {
                panel = CameraPanel(context, client)
                panel.render(CameraPanelState())
                val connect = panel.findViewWithTag<Button>("camera.connect")
                check(connect != null && !connect.isEnabled) {
                    "Camera access must wait for a native Wi-Fi path observation"
                }
                check(panel.findViewWithTag<View>("camera.truth.card")?.visibility == View.GONE) {
                    "Camera truth must remain hidden until Rust has accepted read-only evidence"
                }
                check(descendants(panel).none { it is TextView && it.hint?.contains("IP", ignoreCase = true) == true }) {
                    "The fixed Rust-owned camera origin must not gain an IP-entry field"
                }
            }
        } finally {
            client.close()
            state.close()
        }
    }

    @Test
    fun rustAvccAccessUnitsAreConvertedOnlyAtTheAndroidCodecBoundary() {
        val input = byteArrayOf(0, 0, 0, 2, 0x65, 0x55, 0, 0, 0, 1, 0x41)
        val expected = byteArrayOf(0, 0, 0, 1, 0x65, 0x55, 0, 0, 0, 1, 0x41)
        check(lol.cutout.AndroidCameraClient.annexBAccessUnit(input).contentEquals(expected)) {
            "AVCC four-byte NAL lengths should become Android Annex-B start codes"
        }
        check(runCatching { lol.cutout.AndroidCameraClient.annexBAccessUnit(byteArrayOf(0, 0, 0, 4, 0x65)) }.isFailure) {
            "Malformed NAL lengths must not reach MediaCodec"
        }
        check(lol.cutout.AndroidCameraClient.supportsCodec("avc1.4D401E")) {
            "The retained RFC 6381 H.264 codec identifier should be accepted"
        }
        check(!lol.cutout.AndroidCameraClient.supportsCodec("H264")) {
            "Unknown codec identifiers must not be guessed as H.264"
        }
    }

    @Test
    fun connectedCameraHidesReadActionAndDisablesCommandsWhileEitherRequestIsInflight() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(
            Intent(instrumentation.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        ) as Activity
        val state = CutoutSessionStateHandle()
        val client = AndroidCameraClient(activity.applicationContext, { state })
        try {
            instrumentation.runOnMainSync {
                val evidence = MobileNovatekReadOnlySnapshotDto(
                    "R3", "rtsp://192.168.1.254/movie", "", listOf(
                        MobileNovatekCommandStatusDto(2001u.toUShort(), 0u.toUShort()),
                        MobileNovatekCommandStatusDto(1001u.toUShort(), 0u.toUShort()),
                    ), true, emptyList(),
                )
                val panel = CameraPanel(activity, client)
                activity.setContentView(panel)
                panel.render(CameraPanelState(
                    wifiReady = true, connection = CameraConnectionState.CONNECTED, evidence = evidence,
                    recordingRequest = CameraRequestState.IN_FLIGHT,
                ))
                check(panel.findViewWithTag<Button>("camera.connect")?.visibility == View.GONE) {
                    "A connected session must not offer the initial evidence-read action again"
                }
                check(panel.findViewWithTag<Button>("camera.recording.request-start")?.isEnabled == false)
                check(panel.findViewWithTag<Button>("camera.recording.request-stop")?.isEnabled == false)
                check(panel.findViewWithTag<Button>("camera.still.request")?.isEnabled == false)

                panel.render(panelState(evidence, CameraRequestState.ACKNOWLEDGED, CameraRequestState.IN_FLIGHT))
                check(panel.findViewWithTag<Button>("camera.recording.request-start")?.isEnabled == false)
                check(panel.findViewWithTag<Button>("camera.recording.request-stop")?.isEnabled == false)
                check(panel.findViewWithTag<Button>("camera.still.request")?.isEnabled == false)

                panel.render(panelState(evidence, null, null))
                check(panel.findViewWithTag<Button>("camera.recording.request-start")?.isEnabled == true)
                check(panel.findViewWithTag<Button>("camera.recording.request-stop")?.isEnabled == true)
                check(panel.findViewWithTag<Button>("camera.still.request")?.isEnabled == true)
            }
        } finally {
            client.close()
            state.close()
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    private fun panelState(
        evidence: MobileNovatekReadOnlySnapshotDto,
        recording: CameraRequestState?,
        still: CameraRequestState?,
    ) = CameraPanelState(
        wifiReady = true, connection = CameraConnectionState.CONNECTED, evidence = evidence,
        recordingRequest = recording, stillRequest = still,
    )

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }
}
