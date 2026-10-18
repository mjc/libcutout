package lol.cutout.ffitest

import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.SeekBar
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import java.math.BigDecimal
import lol.cutout.MainActivity
import lol.cutout.CutoutUi
import lol.cutout.SettingsPanel
import lol.cutout.R
import uniffi.cutout_mobile_ffi.MobileSettingAccessDto
import uniffi.cutout_mobile_ffi.MobileSettingValueSourceDto
import uniffi.cutout_mobile_ffi.MobileSettingControlDto
import uniffi.cutout_mobile_ffi.MobileSettingGroupDto
import uniffi.cutout_mobile_ffi.MobileSettingStatusDto
import uniffi.cutout_mobile_ffi.MobileSettingSnapshotDto
import uniffi.cutout_mobile_ffi.MobileSettingValueDto
import org.junit.Test

open class SettingsPanelTest {

    @Test
    fun emptyReadbackAndSettingControlsExposeContextualAccessibilityState() {
        val initial = checkNotNull(productionReplayPresentation().settings) {
            "Production replay did not publish settings"
        }
        val booleanDescriptor = initial.settingDescriptors.firstOrNull {
            it.access == MobileSettingAccessDto.WRITABLE && it.control is MobileSettingControlDto.Boolean
        } ?: error("Production replay has no writable boolean setting descriptor")
        val numericDescriptor = initial.settingDescriptors.firstOrNull {
            it.access == MobileSettingAccessDto.WRITABLE && it.control is MobileSettingControlDto.Number &&
                it.group != MobileSettingGroupDto.DIAGNOSTICS
        } ?: error("Production replay has no writable numeric setting descriptor")
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(
            Intent(instrumentation.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        try {
            instrumentation.runOnMainSync {
                val panel = SettingsPanel(activity) { _, _, _, _ -> }
                val labels = CutoutUi(activity)
                activity.setContentView(panel)
                val unavailable = labels.label("settings.readback.unavailable")
                check(visibleText(panel, unavailable)) {
                    "Settings readback should be unavailable before the first owner snapshot"
                }
                panel.render(null)
                check(visibleText(panel, unavailable)) {
                    "Missing settings readback should be described as unavailable"
                }

                val booleanState = initial.settings.firstOrNull { it.id == booleanDescriptor.id }
                    ?: MobileSettingSnapshotDto(
                        id = booleanDescriptor.id,
                        current = null,
                        currentSource = null,
                        evidence = null,
                        requested = null,
                        requestId = null,
                        status = MobileSettingStatusDto.IDLE,
                        transport = null,
                        ageMs = null,
                        refusal = null,
                    )
                val currentReported = initial.copy(settings = initial.settings.filterNot {
                    it.id == booleanDescriptor.id
                } + booleanState.copy(
                    current = MobileSettingValueDto.Boolean(true),
                    requested = MobileSettingValueDto.Boolean(true),
                    currentSource = MobileSettingValueSourceDto.LIVE_READBACK,
                    status = MobileSettingStatusDto.CONFIRMED,
                    transport = null,
                ))
                panel.render(currentReported)
                val booleanRow = descendants(panel).firstOrNull { it.tag == booleanDescriptor.id }
                    ?: error("Missing boolean row")
                val reportedOn = descendants(booleanRow).firstOrNull { it is Button && it.tag == true } as? Button
                    ?: error("Missing reported boolean button")
                val settingName = labels.label(booleanDescriptor.labelKey)
                check(reportedOn.contentDescription.toString().contains(settingName))
                check(reportedOn.contentDescription.toString().contains(labels.label("controls.accessibility.reported"))) {
                    "Reported boolean selection was not announced"
                }
                check(reportedOn.isSelected) { "Reported boolean selection lacks selected semantics" }

                val replayBooleanSnapshot = currentReported.copy(settings = currentReported.settings.map {
                    if (it.id == booleanDescriptor.id) it.copy(
                        currentSource = MobileSettingValueSourceDto.CAPTURE_REPLAY,
                    ) else it
                })
                panel.render(replayBooleanSnapshot)
                val replayOn = descendants(booleanRow).firstOrNull { it is Button && it.tag == true } as? Button
                    ?: error("Missing replay boolean button")
                check(replayOn.contentDescription.toString().contains(labels.label("settings.value.replay"))) {
                    "Replay state was not distinguished from live wheel readback"
                }
                check(!replayOn.contentDescription.toString().contains(labels.label("controls.accessibility.reported"))) {
                    "Replay state was incorrectly announced as reported by the wheel"
                }

                val requestedOffSnapshot = currentReported.copy(settings = currentReported.settings.map {
                    if (it.id == booleanDescriptor.id) it.copy(
                        requested = MobileSettingValueDto.Boolean(false),
                        status = MobileSettingStatusDto.WAITING_FOR_CONFIRMATION,
                    ) else it
                })
                panel.render(requestedOffSnapshot)
                val requestedOff = descendants(booleanRow).firstOrNull { it is Button && it.tag == false } as? Button
                    ?: error("Missing requested boolean button")
                check(requestedOff.contentDescription.toString().contains(labels.label("controls.accessibility.requested"))) {
                    "Requested boolean selection was not announced"
                }
                check(requestedOff.isSelected) { "Requested boolean selection lacks selected semantics" }

                val numeric = numericDescriptor.control as MobileSettingControlDto.Number
                val currentNumber = initial.settings.firstOrNull { it.id == numericDescriptor.id }?.current as? MobileSettingValueDto.Number
                val raw = if (currentNumber?.value == numeric.minimum &&
                    numeric.minimum.toLong() + numeric.step.toLong() <= numeric.maximum.toLong()) {
                    numeric.minimum + numeric.step
                } else {
                    numeric.minimum
                }
                check(currentNumber == null || raw != currentNumber.value) {
                    "Replay numeric descriptor exposes no draft value distinct from readback"
                }
                val editor = tagged<EditText>(panel, "settings.editor.${numericDescriptor.id}")
                val apply = tagged<Button>(panel, "settings.apply.${numericDescriptor.id}")
                editor.setText(BigDecimal.valueOf(raw.toLong(), numeric.precision.toInt()).toPlainString())
                val numericName = labels.label(numericDescriptor.labelKey)
                check(editor.contentDescription.toString().startsWith("$numericName:")) {
                    "Numeric editor has no setting-specific accessibility label/value"
                }
                check(apply.contentDescription.toString().startsWith(
                    labels.label("controls.apply.accessibility").replace("%@", numericName) + ":",
                )) {
                    "Apply button does not identify the setting and draft value"
                }
                val numericRow = descendants(panel).firstOrNull { it.tag == numericDescriptor.id }
                    ?: error("Missing numeric row")
                val decrement = descendants(numericRow).filterIsInstance<TextView>()
                    .firstOrNull { it.text.toString() == "−" }
                val decrementLabel = activity.getString(R.string.setting_decrease, numericName, "")
                check(decrement?.contentDescription.toString().startsWith(decrementLabel.substringBeforeLast(":"))) {
                    "Numeric decrement control has no accessible purpose"
                }
                check(decrement?.createAccessibilityNodeInfo()?.className == Button::class.java.name) {
                    "Numeric stepper segment is not exposed with native button semantics"
                }
                val slider = tagged<SeekBar>(panel, "settings.slider.${numericDescriptor.id}")
                check(slider.max > 0 && slider.contentDescription.toString().startsWith("$numericName:")) {
                    "Numeric setting does not expose the descriptor-backed range slider"
                }
            }
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
        }
    }


    @Test
    fun numericStepperSnapsOffLatticeReadbackToSwiftWireStepsWithoutSubmitting() {
        val initial = checkNotNull(productionReplayPresentation().settings) {
            "Production replay did not publish settings"
        }
        val descriptor = initial.settingDescriptors.firstOrNull {
            it.access == MobileSettingAccessDto.WRITABLE && it.control is MobileSettingControlDto.Number &&
                it.group != MobileSettingGroupDto.DIAGNOSTICS
        } ?: error("Production replay has no writable numeric setting descriptor")
        val shape = (descriptor.control as MobileSettingControlDto.Number).copy(
            minimum = 0,
            maximum = 100,
            step = 10,
            editorStep = 3,
            precision = 1u.toUByte(),
        )
        val state = initial.settings.firstOrNull { it.id == descriptor.id }?.copy(
            current = MobileSettingValueDto.Number(11),
            currentSource = MobileSettingValueSourceDto.CAPTURE_REPLAY,
            requested = null,
            requestId = null,
            status = MobileSettingStatusDto.IDLE,
            transport = null,
        ) ?: MobileSettingSnapshotDto(
            id = descriptor.id,
            current = MobileSettingValueDto.Number(11),
            currentSource = MobileSettingValueSourceDto.CAPTURE_REPLAY,
            evidence = null,
            requested = null,
            requestId = null,
            status = MobileSettingStatusDto.IDLE,
            transport = null,
            ageMs = null,
            refusal = null,
        )
        val presentation = initial.copy(
            settingDescriptors = initial.settingDescriptors.map {
                if (it.id == descriptor.id) it.copy(control = shape) else it
            },
            settings = initial.settings.filterNot { it.id == descriptor.id } + state,
        )
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(
            Intent(instrumentation.targetContext, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        try {
            var submissions = 0
            instrumentation.runOnMainSync {
                val panel = SettingsPanel(activity) { _, _, _, _ -> submissions++ }
                activity.setContentView(panel)
                panel.render(presentation)
                val editor = tagged<EditText>(panel, "settings.editor.${descriptor.id}")
                val row = descendants(panel).first { it.tag == descriptor.id }
                val decrement = descendants(row).filterIsInstance<TextView>()
                    .first { it.text.toString() == "−" }
                val increment = descendants(row).filterIsInstance<TextView>()
                    .first { it.text.toString() == "+" }

                editor.setText("1.1")
                decrement.performClick()
                check(editor.text.toString() == "1.0") {
                    "Decrement from an off-lattice value should snap to the prior wire step"
                }
                check(tagged<Button>(panel, "settings.apply.${descriptor.id}").isEnabled)
                editor.setText("1.8")
                increment.performClick()
                check(editor.text.toString() == "2.0") {
                    "Increment from an off-lattice value should snap to the next wire step"
                }
                check(submissions == 0) { "Stepper edits submitted without explicit Apply" }
            }
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    @Test
    fun restoredSubmissionComesFromRustSnapshotRatherThanLocalAdmission() {
        val initial = checkNotNull(productionReplayPresentation().settings) {
            "Production replay did not publish settings"
        }
        val descriptor = initial.settingDescriptors.firstOrNull {
            it.access == MobileSettingAccessDto.WRITABLE &&
                it.control is MobileSettingControlDto.Number &&
                it.group != MobileSettingGroupDto.DIAGNOSTICS
        } ?: error("Production replay has no writable numeric setting descriptor")
        val number = descriptor.control as MobileSettingControlDto.Number
        val current = initial.settings.firstOrNull { it.id == descriptor.id }?.current as? MobileSettingValueDto.Number
        val raw = if (current?.value == number.minimum && number.minimum.toLong() + number.step <= number.maximum.toLong()) {
            number.minimum + number.step
        } else {
            number.minimum
        }
        check(current == null || raw != current.value) { "Replay has no writable value distinct from readback" }
        val value = MobileSettingValueDto.Number(raw)
        val text = BigDecimal.valueOf(raw.toLong(), number.precision.toInt()).toPlainString()
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(
            Intent(instrumentation.targetContext, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        try {
            val attempts = mutableListOf<MobileSettingValueDto>()
            lateinit var savedUiState: android.os.Bundle
            instrumentation.runOnMainSync {
                val firstPanel = SettingsPanel(activity) { _, _, submitted, _ -> attempts += submitted }
                activity.setContentView(firstPanel)
                firstPanel.render(initial) // Rust's request state remains IDLE and requestId remains null.
                tagged<EditText>(firstPanel, "settings.editor.${descriptor.id}").setText(text)
                check(tagged<Button>(firstPanel, "settings.apply.${descriptor.id}").performClick())
                check(attempts == listOf(value))
                savedUiState = firstPanel.saveUiState()

                val restoredPanel = SettingsPanel(activity) { _, _, submitted, _ -> attempts += submitted }
                activity.setContentView(restoredPanel)
                restoredPanel.restoreUiState(savedUiState)
                restoredPanel.render(initial)
                check(attempts.size == 1) { "Restoration submitted a command" }
                val restoredApply = tagged<Button>(restoredPanel, "settings.apply.${descriptor.id}")
                check(restoredApply.isEnabled) { "Locally submitted draft was stranded by an IDLE Rust snapshot" }
                check(restoredApply.performClick())
                check(attempts == listOf(value, value)) { "Explicit retry after restoration was not submitted" }

                val requestState = initial.settings.firstOrNull { it.id == descriptor.id }
                    ?: error("Writable descriptor has no setting snapshot")
                for (status in listOf(
                    MobileSettingStatusDto.WAITING_FOR_CONFIRMATION,
                    MobileSettingStatusDto.SENT_WITHOUT_CONFIRMATION,
                )) {
                    val rustActive = initial.copy(settings = initial.settings.map {
                        if (it.id == descriptor.id) it.copy(
                            requested = value,
                            requestId = 1UL,
                            status = status,
                        ) else it
                    })
                    val activePanel = SettingsPanel(activity) { _, _, submitted, _ -> attempts += submitted }
                    activity.setContentView(activePanel)
                    activePanel.restoreUiState(savedUiState)
                    activePanel.render(rustActive)
                    check(attempts.size == 2) { "Rust request restoration submitted another command" }
                    check(!tagged<Button>(activePanel, "settings.apply.${descriptor.id}").isEnabled) {
                        "$status with matching requested value should remain pending"
                    }
                    check(requestState.status == MobileSettingStatusDto.IDLE)
                }
            }
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    @Test
    fun numericDraftRefusalRefreshAndReplacementStayAttemptScoped() {
        val presentation = productionReplayPresentation()
        val initial = checkNotNull(presentation.settings) { "Production replay did not publish settings" }
        val descriptor = initial.settingDescriptors.firstOrNull {
            it.access == MobileSettingAccessDto.WRITABLE &&
                it.control is MobileSettingControlDto.Number &&
                it.group != MobileSettingGroupDto.DIAGNOSTICS
        } ?: error("Production replay has no writable numeric setting descriptor")
        val number = descriptor.control as MobileSettingControlDto.Number
        check(number.step > 0 && number.minimum <= number.maximum) {
            "Replay numeric setting has no valid fixed-point lattice"
        }
        val current = initial.settings.firstOrNull { it.id == descriptor.id }?.current as? MobileSettingValueDto.Number
        val raw = when {
            current?.value != number.minimum -> number.minimum
            number.minimum.toLong() + number.step.toLong() <= number.maximum.toLong() -> number.minimum + number.step
            else -> number.minimum
        }
        check(current == null || raw != current.value) {
            "Replay numeric descriptor exposes no editable value distinct from readback"
        }
        val value = MobileSettingValueDto.Number(raw)
        val text = BigDecimal.valueOf(raw.toLong(), number.precision.toInt()).toPlainString()

        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(
            Intent(instrumentation.targetContext, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        try {
            lateinit var panel: SettingsPanel
            val attempts = mutableListOf<Pair<uniffi.cutout_mobile_ffi.MobileConnectionAttemptTokenDto, MobileSettingValueDto>>()
            val refusals = mutableListOf<() -> Unit>()
            instrumentation.runOnMainSync {
                panel = SettingsPanel(activity) { token, id, submitted, refused ->
                    check(id == descriptor.id)
                    attempts += token to submitted
                    refusals += refused
                }
                activity.setContentView(panel)
                panel.render(initial)

                val editor = tagged<EditText>(panel, "settings.editor.${descriptor.id}")
                val apply = tagged<Button>(panel, "settings.apply.${descriptor.id}")
                editor.setText(text)
                check(editor.text.toString() == text)
                check(apply.performClick())
                check(attempts == listOf(initial.connection.token!! to value))
                check(!apply.isEnabled) { "Apply stayed enabled after submission" }
                apply.performClick()
                check(attempts.size == 1) { "A double tap submitted the same request twice" }

                refusals.single().invoke()
                check(visibleText(panel, activity.getString(R.string.setting_refused)))
                check(apply.isEnabled) { "A refused request should be retryable" }
                check(apply.performClick())
                check(attempts.size == 2) { "Retry after native refusal was not submitted" }
                check(!apply.isEnabled) { "Apply stayed enabled after retry submission" }

                editor.setText("")
                check(!apply.isEnabled)
                apply.performClick()
                check(attempts.size == 2) { "Empty numeric input resubmitted the previous draft" }

                editor.setText(text)
                editor.requestFocus()
                editor.setSelection(minOf(1, editor.length()))
                val caret = editor.selectionStart
                panel.render(initial.copy(atMs = initial.atMs + 1UL))
                val refreshedEditor = tagged<EditText>(panel, "settings.editor.${descriptor.id}")
                check(refreshedEditor === editor) { "Age refresh replaced the focused editor" }
                check(editor.hasFocus() && editor.text.toString() == text && editor.selectionStart == caret) {
                    "Age refresh changed editor focus, text, or caret"
                }

                // Refuse the retry, then retain its old editor and Apply view across a new token.
                refusals.last().invoke()
                val savedUiState = panel.saveUiState()
                val restoredPanel = SettingsPanel(activity) { token, id, submitted, refused ->
                    check(id == descriptor.id)
                    attempts += token to submitted
                    refusals += refused
                }
                activity.setContentView(restoredPanel)
                restoredPanel.restoreUiState(savedUiState)
                restoredPanel.render(initial)
                val restoredEditor = tagged<EditText>(restoredPanel, "settings.editor.${descriptor.id}")
                val restoredApply = tagged<Button>(restoredPanel, "settings.apply.${descriptor.id}")
                check(restoredEditor.text.toString() == text && restoredEditor.selectionStart == caret) {
                    "Same-attempt recreation did not restore editor text and caret"
                }
                check(restoredEditor.hasFocus()) { "Same-attempt recreation did not restore editor focus" }
                check(restoredApply.isEnabled) { "Same-attempt recreation did not restore the unsent draft" }

                val replacementGeneration = initial.connection.generation + 1UL
                val oldToken = checkNotNull(initial.connection.token)
                val replacementToken = oldToken.copy(
                    generation = replacementGeneration,
                    platformIdentifier = oldToken.platformIdentifier + "-replacement",
                )
                val replacement = initial.copy(
                    connection = initial.connection.copy(
                        generation = replacementGeneration,
                        token = replacementToken,
                    ),
                )
                restoredPanel.render(replacement)
                check(!isDescendant(restoredPanel, restoredEditor)) { "Replaced attempt kept the old editor attached" }
                check(!restoredApply.isEnabled || !restoredApply.isShown || !isDescendant(restoredPanel, restoredApply)) {
                    "Replaced attempt kept the old Apply control active"
                }
                val attemptCount = attempts.size
                restoredEditor.setText(text)
                restoredApply.performClick()
                check(attempts.size == attemptCount) { "Detached editor submitted with a replacement token" }

                val replacedPanel = SettingsPanel(activity) { token, id, submitted, refused ->
                    check(id == descriptor.id)
                    attempts += token to submitted
                    refusals += refused
                }
                activity.setContentView(replacedPanel)
                replacedPanel.restoreUiState(savedUiState)
                replacedPanel.render(replacement)
                val replacedEditor = tagged<EditText>(replacedPanel, "settings.editor.${descriptor.id}")
                val replacedApply = tagged<Button>(replacedPanel, "settings.apply.${descriptor.id}")
                val replacementNumber = replacement.settings.firstOrNull { it.id == descriptor.id }?.current
                    as? MobileSettingValueDto.Number
                val replacementText = replacementNumber?.let {
                    BigDecimal.valueOf(it.value.toLong(), number.precision.toInt()).toPlainString()
                }.orEmpty()
                check(replacedEditor.text.toString() == replacementText) {
                    "A different token restored the old attempt's draft instead of its current readback"
                }
                check(!replacedApply.isEnabled) { "A different token restored the old attempt's Apply state" }
            }
        } finally {
            instrumentation.runOnMainSync { activity.finish() }
        }
    }

    private fun descendants(view: View): Sequence<View> = sequence {
        if (view is ViewGroup) for (index in 0 until view.childCount) {
            val child = view.getChildAt(index)
            yield(child)
            yieldAll(descendants(child))
        }
    }

    private fun children(view: View): Sequence<View> = sequence {
        yield(view)
        if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(children(view.getChildAt(index)))
    }

    private inline fun <reified T : View> tagged(root: View, tag: String): T =
        children(root).filterIsInstance<T>().firstOrNull { it.tag == tag }
            ?: error("Missing ${T::class.java.simpleName} tagged $tag")

    private fun visibleText(root: View, text: String) =
        children(root).any { it.isShown && it is android.widget.TextView && it.text.toString() == text }

    private fun isDescendant(root: View, target: View) = children(root).any { it === target }
}
