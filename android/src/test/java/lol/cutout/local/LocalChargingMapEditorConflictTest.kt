package lol.cutout.local

import android.app.Activity
import android.app.AlertDialog
import android.os.Looper
import android.view.View
import android.widget.Button
import android.widget.EditText
import lol.cutout.ChargingMapPanel
import lol.cutout.CutoutUi
import lol.cutout.TestRideMapSurface
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowAlertDialog
import org.robolectric.shadows.ShadowDialog
import uniffi.cutout_mobile_ffi.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalChargingMapEditorConflictTest {
    @Test
    fun staleSaveCanKeepDraftOnLatestRevisionAndEditorCannotCloseWhileSaveIsPending() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val app = activity.application as LocalTestApplication
        val coordinate = MobileMapCoordinateDto(39.7392, -104.9903)
        val original = input(coordinate, "Original note", "Original name")
        var panel: ChargingMapPanel? = null
        var surface: TestRideMapSurface? = null
        try {
            val created = await(activity) { done ->
                app.charging.withDatabase({ database ->
                    val receipt = database.saveChargingSpot(original)
                    checkNotNull(database.chargingSpot(receipt.id))
                }, done)
            }
            val mapSurface = TestRideMapSurface(activity)
            surface = mapSurface
            val charging = ChargingMapPanel(activity, app.charging, mapSurface, {})
            panel = charging
            activity.setContentView(charging)
            charging.activate()
            mapSurface.onViewportChanged?.invoke(MobileGeoBoundsDto(39.0, 40.0, -106.0, -104.0))
            checkNotNull(charging.findViewWithTag<Button>("charging_view_list")).performClick()
            awaitUi(activity) { charging.findViewWithTag<View>("charging.spot.${created.id}") != null }
            val spotRow = checkNotNull(charging.findViewWithTag<View>("charging.spot.${created.id}"))
            spotRow.performClick()
            awaitUi(activity) { sheetRoot()?.findViewWithTag<Button>("charging.details.edit") != null }
            checkNotNull(sheetRoot()?.findViewWithTag<Button>("charging.details.edit")).performClick()

            val draftName = checkNotNull(sheetRoot()?.findViewWithTag<EditText>("charging_editor.name"))
            val draftNote = checkNotNull(sheetRoot()?.findViewWithTag<EditText>("charging.editor.access-note"))
            draftName.setText("My retained draft")
            draftNote.setText("My revised access instructions")
            val submittedDraft = input(coordinate, draftNote.text.toString(), draftName.text.toString())
            await<Unit>(activity) { done ->
                app.charging.withDatabase({ database ->
                    database.editChargingSpot(created.id, created.localRevision, input(coordinate, "Changed elsewhere", "Remote name"))
                    Unit
                }, done)
            }

            val save = checkNotNull(sheetRoot()?.findViewWithTag<Button>("charging.editor.save"))
            save.performClick()
            assertFalse("the in-flight receipt disables repeated Save", save.isEnabled)
            assertTrue("Back remains consumed while the native save is pending", charging.handleBack())
            assertEquals("Back must leave the in-flight editor attached", draftName,
                sheetRoot()?.findViewWithTag<EditText>("charging_editor.name"))

            awaitUi(activity) {
                val dialog = ShadowAlertDialog.getLatestAlertDialog()
                dialog?.isShowing == true &&
                    dialog.getButton(AlertDialog.BUTTON_POSITIVE)?.text == CutoutUi(activity).label("charging.editor.review_latest") &&
                    dialog.findViewById<android.widget.TextView>(android.R.id.message)?.text ==
                    CutoutUi(activity).label("charging.editor.error.stale_revision")
            }
            val conflict = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            assertEquals(
                CutoutUi(activity).label("charging.editor.error.stale_revision"),
                conflict.findViewById<android.widget.TextView>(android.R.id.message).text.toString(),
            )
            conflict.getButton(AlertDialog.BUTTON_POSITIVE).performClick()
            awaitUi(activity) {
                ShadowAlertDialog.getLatestAlertDialog()?.getButton(AlertDialog.BUTTON_POSITIVE)?.text ==
                    CutoutUi(activity).label("charging.editor.keep_draft")
            }
            val latestDialog = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            assertEquals(CutoutUi(activity).label("charging.editor.keep_draft"),
                latestDialog.getButton(AlertDialog.BUTTON_POSITIVE).text)
            assertNotNull(latestDialog.getButton(AlertDialog.BUTTON_NEUTRAL))
            latestDialog.getButton(AlertDialog.BUTTON_POSITIVE).performClick()

            awaitUi(activity) {
                sheetRoot()?.findViewWithTag<EditText>("charging_editor.name")?.text?.toString() == submittedDraft.name
            }
            assertEquals(submittedDraft.accessNote,
                sheetRoot()?.findViewWithTag<EditText>("charging.editor.access-note")?.text?.toString())
            assertEquals(submittedDraft.coordinate.latitudeDegrees.toString(),
                sheetRoot()?.findViewWithTag<EditText>("charging_editor.latitude")?.text?.toString())

            val retriedSave = checkNotNull(sheetRoot()?.findViewWithTag<Button>("charging.editor.save"))
            assertTrue("a conflict receipt re-enables an explicit retry", retriedSave.isEnabled)
            retriedSave.performClick()
            assertFalse("the retry is immediately coalesced", retriedSave.isEnabled)
            assertTrue(charging.handleBack())
            awaitUi(activity) { sheetRoot()?.findViewWithTag<Button>("charging.details.edit") != null }
            val saved = await(activity) { done -> app.charging.withDatabase({ it.chargingSpot(created.id) }, done) }
            assertEquals(submittedDraft.name, saved?.input?.name)
            assertEquals(submittedDraft.accessNote, saved?.input?.accessNote)
        } finally {
            panel?.dispose()
            surface?.onHostDestroy()
            host.pause().stop().destroy()
        }
    }

    private fun input(
        coordinate: MobileMapCoordinateDto,
        note: String,
        name: String?,
    ) = MobileChargingInputDto(
        coordinate,
        note,
        MobileChargingCostDto.FREE,
        name,
        null,
        null,
        null,
        null,
        emptyList(),
    )

    private fun <T> await(activity: Activity, start: ((Result<T>) -> Unit) -> Unit): T {
        val latch = CountDownLatch(1)
        var result: Result<T>? = null
        start { value -> result = value; latch.countDown() }
        check(awaitWithMainLooper(latch, 5, TimeUnit.SECONDS)) { "charging database callback did not finish" }
        shadowOf(activity.mainLooper).idle()
        return checkNotNull(result).getOrThrow()
    }

    private fun awaitUi(activity: Activity, condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(5)
        while (!condition() && System.nanoTime() < deadline) {
            shadowOf(Looper.getMainLooper()).idle()
            if (!condition()) Thread.sleep(10)
        }
        shadowOf(activity.mainLooper).idle()
        assertTrue("expected charging UI transition did not complete", condition())
    }

    private fun sheetRoot(): View? = ShadowDialog.getShownDialogs()
        .lastOrNull { it.isShowing && it !is AlertDialog }
        ?.window?.decorView
}
