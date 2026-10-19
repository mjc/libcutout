package lol.cutout.local

import android.app.Activity
import android.os.Looper
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.Spinner
import lol.cutout.CaptureSettingsWorksheet
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalCaptureSettingsWorksheetTest {
    @Test
    fun worksheetSubmitsTypedNativeDraftAndWaitsForReceiptBeforeClosing() {
        val profile = captureResearchCatalogs().firstOrNull { catalog ->
            catalog.fields.any { it.control is MobileSettingControlDto.Number }
        }
        assertNotNull("Rust should expose at least one numeric research field", profile)
        val selectedProfile = checkNotNull(profile)
        val number = selectedProfile.fields.first { it.control is MobileSettingControlDto.Number }
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        var submitted: MobileCaptureSettingsDraftDto? = null
        var finish: ((Result<MobileCaptureSettingsObservationDto>) -> Unit)? = null
        val dialog = CaptureSettingsWorksheet(activity) { draft, done ->
            submitted = draft
            finish = done
        }.show()
        try {
            val profilePicker = findTagged<Spinner>(dialog, "capture.settings.profile")
            profilePicker.setSelection(captureResearchCatalogs().indexOfFirst { it.profileId == selectedProfile.profileId } + 1)
            shadowOf(Looper.getMainLooper()).idle()
            val value = findTagged<EditText>(dialog, "capture.settings.value.${number.id}")
            value.setText("42.5")
            findTagged<Spinner>(dialog, "capture.settings.source").setSelection(1)
            findTagged<android.widget.CheckBox>(dialog, "capture.settings.earlier").performClick()
            findTagged<EditText>(dialog, "capture.settings.time").setText("12.5")
            findTagged<EditText>(dialog, "capture.settings.note").setText("Observed on the display")
            shadowOf(Looper.getMainLooper()).idle()
            NativeSurfaceCapture.save(dialog.window!!.decorView, "capture-settings-worksheet-dark")

            val save = findTagged<Button>(dialog, "capture.settings.save")
            save.performClick()
            val draft = checkNotNull(submitted)
            assertFalse("pending save must prevent duplicate submissions", save.isEnabled)
            assertTrue("the worksheet stays open until Rust returns a receipt", dialog.isShowing)
            assertEquals(selectedProfile.profileId, draft.profile.profileId)
            assertEquals(12_500UL, draft.observedAtMs)
            assertEquals(MobileCaptureObservationSourceDto.WHEEL_DISPLAY, draft.source)
            assertEquals("Observed on the display", draft.note)
            val entered = draft.values.first { it.settingId == number.id }
            assertEquals(MobileCaptureObservedValueDto.Entered("42.5"), entered.value)

            val saved = MobileCaptureSettingsObservationDto(
                "capture-test", draft.profile, checkNotNull(draft.observedAtMs), 13_000UL,
                draft.source, draft.values, draft.note,
            )
            checkNotNull(finish)(Result.success(saved))
            shadowOf(Looper.getMainLooper()).idle()
            assertFalse("successful receipt closes the worksheet", dialog.isShowing)
        } finally {
            if (dialog.isShowing) dialog.dismiss()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun failedSaveKeepsTheEnteredDraftAndAllowsRetry() {
        val profile = captureResearchCatalogs().firstOrNull { catalog -> catalog.fields.isNotEmpty() }
        assertNotNull("Rust should expose research profiles", profile)
        val selectedProfile = checkNotNull(profile)
        val field = selectedProfile.fields.first()
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val callbacks = mutableListOf<(Result<MobileCaptureSettingsObservationDto>) -> Unit>()
        val dialog = CaptureSettingsWorksheet(activity) { _, done -> callbacks += done }.show()
        try {
            findTagged<Spinner>(dialog, "capture.settings.profile")
                .setSelection(captureResearchCatalogs().indexOfFirst { it.profileId == selectedProfile.profileId } + 1)
            shadowOf(Looper.getMainLooper()).idle()
            val valueView = findTaggedOrNull<EditText>(dialog, "capture.settings.value.${field.id}")
            if (valueView != null) valueView.setText("17")
            val button = findTagged<Button>(dialog, "capture.settings.save")
            button.performClick()
            assertEquals(1, callbacks.size)
            callbacks.removeAt(0)(Result.failure(IllegalStateException("temporary failure")))
            shadowOf(Looper.getMainLooper()).idle()
            assertTrue(dialog.isShowing)
            assertTrue(findTagged<android.widget.TextView>(dialog, "capture.settings.error").text.isNotEmpty())
            assertEquals("17", valueView?.text?.toString())
            assertTrue(button.isEnabled)
            button.performClick()
            assertEquals("a failed attempt can be retried", 1, callbacks.size)
        } finally {
            if (dialog.isShowing) dialog.dismiss()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun numberEditorPreservesOffUnobservedAndSelectableSpeedUnits() {
        val profile = captureResearchCatalogs().firstOrNull { catalog ->
            catalog.fields.any { (it.control as? MobileSettingControlDto.Number)?.canDisable == true } &&
                catalog.fields.any { (it.control as? MobileSettingControlDto.Number)?.unit == MobileSettingUnitDto.KILOMETRES_PER_HOUR }
        }
        assertNotNull("Rust should expose a disable-capable field and a speed field", profile)
        val selectedProfile = checkNotNull(profile)
        val disabledField = selectedProfile.fields.first { candidate ->
            (candidate.control as? MobileSettingControlDto.Number)?.canDisable == true
        }
        val speedField = selectedProfile.fields.first { candidate ->
            (candidate.control as? MobileSettingControlDto.Number)?.unit == MobileSettingUnitDto.KILOMETRES_PER_HOUR
        }
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        data class Request(
            val draft: MobileCaptureSettingsDraftDto,
            val done: (Result<MobileCaptureSettingsObservationDto>) -> Unit,
        )
        val requests = mutableListOf<Request>()
        val dialog = CaptureSettingsWorksheet(activity) { draft, done -> requests += Request(draft, done) }.show()
        try {
            findTagged<Spinner>(dialog, "capture.settings.profile")
                .setSelection(captureResearchCatalogs().indexOfFirst { it.profileId == selectedProfile.profileId } + 1)
            shadowOf(Looper.getMainLooper()).idle()
            val state = findTagged<Spinner>(dialog, "capture.settings.state.${disabledField.id}")
            val editorRow = findTagged<View>(dialog, "capture.settings.editor.${disabledField.id}")
            val value = findTagged<EditText>(dialog, "capture.settings.value.${speedField.id}")
            value.setText("33")
            val unit = findTagged<Spinner>(dialog, "capture.settings.unit.${speedField.id}")
            unit.setSelection(2)
            shadowOf(Looper.getMainLooper()).idle()

            state.setSelection(1)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(View.GONE, editorRow.visibility)
            assertEquals(View.VISIBLE, findTagged<View>(dialog, "capture.settings.editor.${speedField.id}").visibility)
            findTagged<Button>(dialog, "capture.settings.save").performClick()
            val off = requests.removeAt(0)
            val offValue = off.draft.values.single { it.settingId == disabledField.id }
            assertEquals(MobileCaptureObservedValueDto.Off, offValue.value)
            val unknownSpeedUnit = off.draft.values.single { it.settingId == speedField.id }
            assertEquals(MobileCaptureObservedValueDto.Entered("33"), unknownSpeedUnit.value)
            assertNull("unknown speed unit is represented as absent", unknownSpeedUnit.unit)
            off.done(Result.failure(IllegalStateException("keep worksheet open")))
            shadowOf(Looper.getMainLooper()).idle()

            state.setSelection(2)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(View.GONE, editorRow.visibility)
            findTagged<Button>(dialog, "capture.settings.save").performClick()
            val unobserved = requests.removeAt(0)
            val unobservedValue = unobserved.draft.values.single { it.settingId == disabledField.id }
            assertEquals(MobileCaptureObservedValueDto.Unobserved, unobservedValue.value)
            unobserved.done(Result.failure(IllegalStateException("keep worksheet open")))
            shadowOf(Looper.getMainLooper()).idle()

            state.setSelection(0)
            shadowOf(Looper.getMainLooper()).idle()
            assertEquals(View.VISIBLE, editorRow.visibility)
            unit.setSelection(1)
            shadowOf(Looper.getMainLooper()).idle()
            findTagged<Button>(dialog, "capture.settings.save").performClick()
            val entered = requests.removeAt(0).draft.values.single { it.settingId == speedField.id }
            assertEquals(MobileCaptureObservedValueDto.Entered("33"), entered.value)
            assertEquals("mph", entered.unit)
        } finally {
            if (dialog.isShowing) dialog.dismiss()
            host.pause().stop().destroy()
        }
    }

    private fun <T : android.view.View> findTagged(dialog: android.app.Dialog, tag: String, type: Class<T>): T {
        val root = dialog.window?.decorView ?: error("dialog has no content")
        return findTagged(root, tag, type) ?: error("missing view tag $tag")
    }

    private fun <T : android.view.View> findTagged(root: android.view.View, tag: String, type: Class<T>): T? {
        if (root.tag == tag && type.isInstance(root)) return type.cast(root)
        if (root is android.view.ViewGroup) for (index in 0 until root.childCount) {
            findTagged(root.getChildAt(index), tag, type)?.let { return it }
        }
        return null
    }

    private inline fun <reified T : android.view.View> findTagged(dialog: android.app.Dialog, tag: String): T =
        findTagged(dialog, tag, T::class.java)

    private inline fun <reified T : android.view.View> findTaggedOrNull(dialog: android.app.Dialog, tag: String): T? {
        val root = dialog.window?.decorView ?: return null
        return findTagged(root, tag, T::class.java)
    }
}
