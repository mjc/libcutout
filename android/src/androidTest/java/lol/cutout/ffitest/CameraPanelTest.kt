package lol.cutout.ffitest

import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidCameraClient
import lol.cutout.CameraPanel
import lol.cutout.CameraPanelState
import org.junit.Test
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle

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

    private fun descendants(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
    }
}
