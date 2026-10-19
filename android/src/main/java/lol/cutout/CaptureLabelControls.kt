package lol.cutout

import android.content.Context
import android.view.View
import android.widget.Button
import android.widget.GridLayout
import android.widget.LinearLayout
import uniffi.cutout_mobile_ffi.*

/** Native controls for the shared label vocabulary; Rust owns intervals and exclusivity. */
internal class CaptureLabelControls(
    context: Context,
    private val generation: MobileCaptureGenerationDto,
    private val change: (MobileCaptureGenerationDto, MobileCaptureLabelActionDto, (Result<List<MobileCaptureLabelDto>>) -> Unit) -> Unit,
    private val onFailure: (Throwable) -> Unit,
) : LinearLayout(context) {
    private val ui = CutoutUi(context)
    private val buttons = linkedMapOf<MobileCaptureLabelDto, Button>()
    private val compactLabels = mutableSetOf<MobileCaptureLabelDto>()
    private val titles = mutableMapOf<MobileCaptureLabelDto, String>()
    private val startTitle = ui.label("capture.label.start")
    private val stopTitle = ui.label("capture.label.stop")
    private val activeTitle = ui.label("capture.label.active")
    private val idleTitle = ui.label("capture.label.idle")
    private var active: List<MobileCaptureLabelDto> = emptyList()
    private var canAnnotate = false
    private var pending = false
    private var expanded = false

    init {
        orientation = VERTICAL
        tag = "captures.labels"
        addView(ui.text(ui.label("captures.observed_activity"), 17f, true).apply { isAccessibilityHeading = true })
        addView(ui.space(8))
        val activities = listOf(
            MobileCaptureLabelDto.POWERED_ON_STATIONARY, MobileCaptureLabelDto.BALANCING,
            MobileCaptureLabelDto.ROLLING_FORWARD, MobileCaptureLabelDto.ROLLING_BACKWARD,
            MobileCaptureLabelDto.LIFTED_WHEEL, MobileCaptureLabelDto.RIDE, MobileCaptureLabelDto.CHARGING,
        )
        val columns = if (resources.configuration.fontScale >= 1.3f) 1 else 2
        addView(GridLayout(context).apply {
            columnCount = columns
            activities.forEachIndexed { index, label ->
                addView(control(label, compact = true), GridLayout.LayoutParams(
                    GridLayout.spec(index / columns), GridLayout.spec(index % columns, 1f),
                ).apply { width = 0; height = -2; setMargins(ui.dp(3), ui.dp(3), ui.dp(3), ui.dp(3)) })
            }
        }, LayoutParams(-1, -2))
        val actions = ui.column().apply { visibility = View.GONE }
        val disclosure = ui.action(ui.label("captures.more_actions")) {
            expanded = !expanded
            actions.visibility = if (expanded) View.VISIBLE else View.GONE
        }.apply { tag = "captures.labels.more" }
        addView(disclosure, LayoutParams(-1, -2))
        listOf(
            MobileCaptureLabelDto.DISCONNECT_RECONNECT, MobileCaptureLabelDto.POWER_CYCLE,
            MobileCaptureLabelDto.LOW_BEAM_ON, MobileCaptureLabelDto.LOW_BEAM_OFF,
            MobileCaptureLabelDto.HIGH_BEAM_ON, MobileCaptureLabelDto.HIGH_BEAM_OFF,
            MobileCaptureLabelDto.HORN, MobileCaptureLabelDto.PEDALS_HARD,
            MobileCaptureLabelDto.PEDALS_MEDIUM, MobileCaptureLabelDto.PEDALS_SOFT,
            MobileCaptureLabelDto.RESET_TRIP, MobileCaptureLabelDto.SOFTWARE_LOCK,
            MobileCaptureLabelDto.SOFTWARE_UNLOCK, MobileCaptureLabelDto.TILTBACK_SPEED,
            MobileCaptureLabelDto.ALARM_SPEED, MobileCaptureLabelDto.ALARM_CHANGE,
            MobileCaptureLabelDto.ANGLE_ADJUSTMENT, MobileCaptureLabelDto.RIDE_MODE,
            MobileCaptureLabelDto.PWM_PERCENT, MobileCaptureLabelDto.PEDAL_HARDNESS,
            MobileCaptureLabelDto.DISPLAY_BRIGHTNESS, MobileCaptureLabelDto.DYNAMIC_ASSIST,
            MobileCaptureLabelDto.PEDAL_DIP_COMPENSATION, MobileCaptureLabelDto.LATERAL_CUTOFF,
            MobileCaptureLabelDto.VOLTAGE_CORRECTION, MobileCaptureLabelDto.DISPLAY_UNITS,
            MobileCaptureLabelDto.BRAKE_OVERPRESSURE_ALARM, MobileCaptureLabelDto.BEEPER_VOLUME,
            MobileCaptureLabelDto.ROLL_ANGLE_MODE, MobileCaptureLabelDto.LED_PATTERN,
        ).forEach { actions.addView(control(it, compact = false), LayoutParams(-1, -2)) }
        addView(actions, LayoutParams(-1, -2))
        updateButtons()
    }

    fun render(value: DiagnosticCapturePresentation) {
        val attempt = value.lifecycle.attempt
        val admitted = attempt != null && attempt.generation == generation && attempt.stage in setOf(
            MobileCaptureStageDto.RECORDING, MobileCaptureStageDto.SAVE_FAILED,
        )
        if (canAnnotate == admitted && active == value.activeLabels) return
        canAnnotate = admitted
        active = value.activeLabels
        updateButtons()
    }

    private fun control(label: MobileCaptureLabelDto, compact: Boolean): Button {
        val slug = captureLabelSlug(label)
        val title = ui.label("capture.label.$slug")
        titles[label] = title
        return ui.action(title) {
            if (!canAnnotate || pending) return@action
            val action = if (label in active) MobileCaptureLabelActionDto.Stop(label) else MobileCaptureLabelActionDto.Start(label)
            pending = true
            updateButtons()
            change(generation, action) { result ->
                pending = false
                result.onSuccess { active = it }.onFailure(onFailure)
                updateButtons()
            }
        }.apply {
            tag = "capture.label.$slug.action"
            if (compact) compactLabels += label
            buttons[label] = this
        }
    }

    private fun updateButtons() {
        buttons.forEach { (label, button) ->
            val isActive = label in active
            val title = checkNotNull(titles[label])
            val action = (if (isActive) stopTitle else startTitle).replace("%@", title)
            button.text = if (label in compactLabels) title else action
            button.contentDescription = action
            button.stateDescription = if (isActive) activeTitle else idleTitle
            button.isSelected = isActive
            button.isEnabled = canAnnotate && !pending
            button.setTextColor(if (!button.isEnabled) ui.muted else if (isActive) ui.orange else ui.brand)
        }
    }
}
