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
import lol.cutout.CameraAutoReadGate
import lol.cutout.MainActivity
import org.junit.Test
import uniffi.cutout_mobile_ffi.*

/** Camera UI and codec-boundary checks without a network, camera, or device action. */
open class CameraPanelTest {
    @Test
    fun automaticReadAdmissionIsOneShotPerActiveWifiIdentity() {
        val gate = CameraAutoReadGate<Any>()
        val firstNetwork = Any()
        val secondNetwork = Any()
        check(!gate.claim(active = false, wifiReady = true, isReading = false, hasEvidence = false, network = firstNetwork))
        check(!gate.claim(active = true, wifiReady = false, isReading = false, hasEvidence = false, network = firstNetwork))
        check(gate.claim(active = true, wifiReady = true, isReading = false, hasEvidence = false, network = firstNetwork))
        check(!gate.claim(active = true, wifiReady = true, isReading = false, hasEvidence = false, network = firstNetwork)) {
            "Repeated path callbacks for one Wi-Fi network must not enqueue another automatic read"
        }
        check(!gate.claim(active = true, wifiReady = true, isReading = true, hasEvidence = false, network = secondNetwork)) {
            "An in-flight read must not admit another automatic request"
        }
        check(gate.claim(active = true, wifiReady = true, isReading = false, hasEvidence = false, network = secondNetwork)) {
            "A newly observed Wi-Fi network should receive its own automatic read attempt"
        }
        gate.reset()
        check(gate.claim(active = true, wifiReady = true, isReading = false, hasEvidence = false, network = firstNetwork)) {
            "Re-entering the active route should permit one fresh read"
        }
    }

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

    @Test
    fun replacingMediaResultsReturnsPaginationToFirstPage() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(
            Intent(instrumentation.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        ) as Activity
        val state = CutoutSessionStateHandle()
        val client = AndroidCameraClient(activity.applicationContext, { state })
        try {
            instrumentation.runOnMainSync {
                val media = (0 until 101).map { index -> mediaEntry(index) }
                val evidence = MobileNovatekReadOnlySnapshotDto(
                    "R3", "rtsp://192.168.1.254/movie", "", emptyList(), true, media,
                )
                val panel = CameraPanel(activity, client)
                activity.setContentView(panel)
                panel.render(CameraPanelState(wifiReady = true, connection = CameraConnectionState.CONNECTED, evidence = evidence))
                panel.findViewWithTag<Button>("camera.files.load-more")?.performClick()
                check(descendants(panel).filterIsInstance<TextView>().any { it.text == "camera-file-100.mp4" }) {
                    "The second media page should include the 101st camera record"
                }

                val replacement = media.toMutableList().apply { this[0] = mediaEntry(0, name = "refreshed-file-0.mp4") }
                panel.render(CameraPanelState(
                    wifiReady = true, connection = CameraConnectionState.CONNECTED,
                    evidence = evidence.copy(media = replacement),
                ))
                check(descendants(panel).filterIsInstance<TextView>().none { it.text == "camera-file-100.mp4" }) {
                    "A replacement media result must reset the visible list to page one"
                }
                check(descendants(panel).filterIsInstance<TextView>().any { it.text == "refreshed-file-0.mp4" })
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

    private fun mediaEntry(index: Int, name: String = "camera-file-$index.mp4") = MobileNovatekMediaEntryDto(
        name, "/$name", 100uL, index.toULong(), "00:00:$index", 0u,
    )

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }
}
