package lol.cutout.local

import android.app.Activity
import android.content.Intent
import android.content.res.Configuration
import android.os.BatteryManager
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import lol.cutout.AndroidChargingStopPrompt
import lol.cutout.ChargingStopPolicyLifetime
import lol.cutout.R
import lol.cutout.RideMapPanel
import lol.cutout.TestRideMapSurface
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowDialog
import uniffi.cutout_mobile_ffi.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.json.JSONObject

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalChargingStopPromptTest {
    @Test
    fun promptActionsRemainMeasuredAtLargeAccessibilityTextScale() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val app = activity.application as LocalTestApplication
        val configuration = Configuration(activity.resources.configuration).apply { fontScale = 2.0f }
        val context = activity.createConfigurationContext(configuration)
        val prompt = AndroidChargingStopPrompt(
            context, app.rides, app::sessionStateForNativeOwner, ::sample,
            activity.getSharedPreferences("charging-prompt-large-text", 0), { true }, { false },
        ).apply { visibility = View.VISIBLE }
        val root = FrameLayout(context).apply {
            addView(prompt, FrameLayout.LayoutParams(-1, -2, android.view.Gravity.BOTTOM))
        }
        try {
            activity.setContentView(root)
            NativeSurfaceCapture.save(root, "charging-stop-prompt-large-text")
            listOf("charging.stop.dismiss", "charging.stop.add").forEach { tag ->
                val button = prompt.findViewWithTag<android.widget.Button>(tag)
                assertNotNull(button)
                assertTrue("$tag must preserve its native minimum touch target", button.measuredHeight >= button.minimumHeight)
                assertTrue("$tag must lay out its label", button.lineCount > 0)
            }
        } finally {
            prompt.close()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun detachedOfferDoesNotConsumeHistoryThenVisiblePromptAcknowledgesOnceAndDismissKeepsSuppression() {
        val host = Robolectric.buildActivity(Activity::class.java).setup()
        val activity = host.get()
        val app = activity.application as LocalTestApplication
        val preferences = activity.getSharedPreferences("cutout.presentation", 0).also { it.edit().clear().commit() }
        installExternalPower(activity, plugged = 0, status = BatteryManager.BATTERY_STATUS_FULL)
        startRide(app)
        var location = sample()
        val prompt = AndroidChargingStopPrompt(
            activity, app.rides, app::sessionStateForNativeOwner, { location }, preferences,
            { true }, { false },
        )
        try {
            prompt.refresh(true)
            awaitMain(activity) { prompt.isQueryIdle }
            assertEquals("A full battery without external power is not charging evidence", View.GONE, prompt.visibility)
            assertNull(preferences.getString(HISTORY_KEY, null))

            installExternalPower(activity)
            prompt.refresh(true)
            awaitMain(activity) { prompt.visibility == View.VISIBLE }
            assertNull("A detached view cannot acknowledge Rust's candidate", preferences.getString(HISTORY_KEY, null))

            val footer = TextView(activity).apply { text = "Following app content"; tag = "prompt.footer.sentinel" }
            val content = LinearLayout(activity).apply {
                orientation = LinearLayout.VERTICAL
                addView(prompt, LinearLayout.LayoutParams(-1, -2))
                addView(footer, LinearLayout.LayoutParams(-1, -2))
            }
            activity.setContentView(content)
            host.visible()
            NativeSurfaceCapture.save(content, "charging-stop-prompt-visible")
            val firstHistory = awaitMainValue(activity) { preferences.getString(HISTORY_KEY, null) }
            assertEquals(1, JSONObject(firstHistory).getJSONArray("locations").length())
            assertEquals(lol.cutout.CutoutUi(activity).label("charging.stop.title"), prompt.findViewWithTag<View>("charging.stop.title")?.let {
                (it as android.widget.TextView).text.toString()
            })

            prompt.refresh(true)
            awaitMain(activity) { prompt.isQueryIdle }
            assertEquals("A repeated visible receipt must not duplicate the same suppression location", firstHistory,
                preferences.getString(HISTORY_KEY, null))

            prompt.findViewWithTag<android.widget.Button>("charging.stop.dismiss")!!.performClick()
            assertEquals(View.GONE, prompt.visibility)
            prompt.refresh(true)
            awaitMain(activity) { prompt.isQueryIdle }
            assertEquals(View.GONE, prompt.visibility)
            assertEquals(firstHistory, preferences.getString(HISTORY_KEY, null))

            location = sample().copy(latitudeDegrees = 39.85, longitudeDegrees = -104.9)
            prompt.refresh(true)
            awaitMain(activity) { prompt.visibility == View.VISIBLE && prompt.isQueryIdle }
            assertSame("An offer must not reorder app content around the inline prompt", prompt, content.getChildAt(0))
            assertSame(footer, content.getChildAt(1))
        } finally {
            prompt.close()
            finishRide(app)
            host.pause().stop().destroy()
        }
    }

    @Test
    fun closingWithQueuedRustObservationRetiresViewAndDefersPolicyReleaseUntilCompletion() {
        val host = Robolectric.buildActivity(Activity::class.java).setup()
        val activity = host.get()
        val app = activity.application as LocalTestApplication
        val preferences = activity.getSharedPreferences("cutout.presentation", 0).also { it.edit().clear().commit() }
        installExternalPower(activity)
        startRide(app)
        val prompt = AndroidChargingStopPrompt(
            activity, app.rides, app::sessionStateForNativeOwner, ::sample, preferences,
            { true }, { false },
        )
        val content = FrameLayout(activity)
        content.addView(prompt)
        activity.setContentView(content)
        try {
            prompt.refresh(true)
            prompt.close()
            awaitMain(activity) { prompt.isPolicyReleased }
            assertEquals(View.GONE, prompt.visibility)
            assertNull("A late query must not acknowledge or persist an unseen candidate", preferences.getString(HISTORY_KEY, null))
        } finally {
            finishRide(app)
            host.pause().stop().destroy()
        }
    }

    @Test
    fun closeFenceWaitsForInFlightCallAndReleasesExactlyOnce() {
        var releases = 0
        val fence = ChargingStopPolicyLifetime { releases++ }
        assertTrue(fence.begin())
        fence.close()
        assertEquals(0, releases)
        assertFalse(fence.begin())
        fence.finish()
        fence.finish()
        fence.close()
        assertEquals(1, releases)
    }

    @Test
    fun acceptingPromptHandsCoordinateToExistingUnsavedEditorWithoutReplacingDraft() {
        val host = Robolectric.buildActivity(Activity::class.java).setup()
        val activity = host.get()
        val app = activity.application as LocalTestApplication
        val panel = RideMapPanel(activity, app.rides, app.charging, {}, {}, mapSurfaceFactory = { TestRideMapSurface(it) })
        activity.setContentView(panel)
        val coordinate = MobileMapCoordinateDto(39.7392, -104.9903)
        val prompt = MobileChargingStopPromptDto("gps-stop-test", coordinate)
        try {
            assertTrue(panel.beginChargingStop(prompt))
            val editorWindow = checkNotNull(ShadowDialog.getLatestDialog()?.window?.decorView)
            editorWindow.findViewWithTag<View>("charging.editor.add-details")!!.performClick()
            val editors = editFields(editorWindow)
            val latitude = editors.single { it.hint == activity.getString(R.string.charging_latitude) }
            val longitude = editors.single { it.hint == activity.getString(R.string.charging_longitude) }
            val name = checkNotNull(editorWindow.findViewWithTag<EditText>("charging_editor.name"))
            assertEquals(coordinate.latitudeDegrees.toString(), latitude.text.toString())
            assertEquals(coordinate.longitudeDegrees.toString(), longitude.text.toString())
            name.setText("Unsaved stop details")
            assertFalse("A second candidate cannot replace the active editor", panel.beginChargingStop(
                prompt.copy(coordinate = MobileMapCoordinateDto(39.8, -105.0)),
            ))
            assertSame(name, editorWindow.findViewWithTag<EditText>("charging_editor.name"))
            assertEquals("Unsaved stop details", name.text.toString())
            assertEquals(coordinate.latitudeDegrees.toString(), latitude.text.toString())
        } finally {
            panel.onHostDestroy()
            host.pause().stop().destroy()
        }
    }

    private fun startRide(app: LocalTestApplication) {
        await<MobileRideMapCoreSnapshotDto> { done -> app.rides.startGpsRide(completed = done) }
    }

    private fun finishRide(app: LocalTestApplication) {
        runCatching {
            val snapshot = await<MobileRideMapCoreSnapshotDto?> { done ->
                app.rides.withCore({ it.currentSnapshot(android.os.SystemClock.elapsedRealtime().toULong()) }, done)
            }
            if (snapshot?.state == MobileRideLifecycleStateDto.ACTIVE || snapshot?.state == MobileRideLifecycleStateDto.PAUSED) {
                await { done -> app.rides.stop(done) }
                await<MobileRideMapCoreSnapshotDto?> { done -> app.rides.discard(done) }
            }
        }
    }

    private fun installExternalPower(
        activity: Activity,
        plugged: Int = BatteryManager.BATTERY_PLUGGED_AC,
        status: Int = BatteryManager.BATTERY_STATUS_CHARGING,
    ) {
        activity.sendStickyBroadcast(Intent(Intent.ACTION_BATTERY_CHANGED)
            .putExtra(BatteryManager.EXTRA_STATUS, status)
            .putExtra(BatteryManager.EXTRA_PLUGGED, plugged))
    }

    private fun sample() = MobilePhoneLocationSampleDto(
        wallClockUnixMs = System.currentTimeMillis().toULong(),
        sourceTimestampUnixSeconds = System.currentTimeMillis() / 1_000.0,
        latitudeDegrees = 39.7392,
        longitudeDegrees = -104.9903,
        altitudeMeters = 1_609.0,
        horizontalAccuracyMeters = 5.0,
        verticalAccuracyMeters = null,
        speedMetersPerSecond = null,
        speedAccuracyMetersPerSecond = null,
        courseDegrees = null,
        courseAccuracyDegrees = null,
    )

    private fun editFields(root: View): List<EditText> {
        fun walk(view: View): List<EditText> = if (view is EditText) listOf(view) else if (view is ViewGroup) {
            (0 until view.childCount).flatMap { walk(view.getChildAt(it)) }
        } else emptyList()
        return walk(root)
    }

    private fun awaitMain(activity: Activity, condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10)
        val main = shadowOf(activity.mainLooper)
        while (!condition() && System.nanoTime() < deadline) {
            main.idle()
            Thread.sleep(10)
        }
        main.idle()
        assertTrue("Condition did not settle before timeout", condition())
    }

    private fun awaitMainValue(activity: Activity, read: () -> String?): String {
        awaitMain(activity) { !read().isNullOrEmpty() }
        return checkNotNull(read())
    }

    private fun <T> await(start: ((Result<T>) -> Unit) -> Unit): T {
        val latch = CountDownLatch(1)
        val result = AtomicReference<Result<T>?>()
        start { result.set(it); latch.countDown() }
        check(awaitWithMainLooper(latch, 15, TimeUnit.SECONDS)) { "Charging-stop test callback did not settle" }
        return checkNotNull(result.get()).getOrThrow()
    }

    companion object { private const val HISTORY_KEY = "charging-stop-prompt-history" }
}
