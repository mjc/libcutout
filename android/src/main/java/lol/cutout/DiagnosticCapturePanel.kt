package lol.cutout

import android.app.AlertDialog
import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.text.InputType
import android.view.View
import android.view.ViewGroup
import android.widget.*
import uniffi.cutout_mobile_ffi.*
import java.io.File
import java.text.DateFormat
import java.util.Date

/** Android presentation for Rust-owned capture admission, recording lifecycle, and history. */
class DiagnosticCapturePanel(
    context: Context,
    private val bluetooth: AndroidBleClient,
    private val onShare: (File) -> Unit,
    private val onRequestBluetooth: () -> Unit = {},
    private val onTitleChanged: (String) -> Unit = {},
) : FrameLayout(context) {
    private val ui = CutoutUi(context)
    private val content = ui.column()
    private val library = ScrollView(context).apply {
        isFillViewport = true
        clipToPadding = false
        isFocusableInTouchMode = true
        descendantFocusability = ViewGroup.FOCUS_BEFORE_DESCENDANTS
        addView(content, LayoutParams(-1, -2))
    }
    private var detail: SavedCaptureDetailPanel? = null
    private var artifactDetail: ArtifactCaptureDetailPanel? = null
    private var showsActive = false
    val navigationTitle: String
        get() = when {
            detail != null -> ui.label("captures.detail.title")
            artifactDetail != null -> checkNotNull(artifactDetail).navigationTitle
            showsActive -> ui.label("captures.active")
            else -> ui.label("captures.title")
        }
    private val renderHandler = Handler(Looper.getMainLooper())
    private val queuedRender = Runnable { render() }
    private val elapsedHandler = Handler(Looper.getMainLooper())
    private val elapsedTick = object : Runnable {
        override fun run() {
            if (disposed || !isAttachedToWindow || windowVisibility != View.VISIBLE || !showsActive || !hasActiveCapture()) return
            activeCapture.updateElapsed(SystemClock.elapsedRealtime().toULong())
            elapsedHandler.postDelayed(this, 1_000L)
        }
    }
    private var captureState = bluetooth.diagnostics.current()
    private var bleState = bluetooth.latestPresentation
    private var disposed = false
    private var labels: CaptureLabelControls? = null
    private var labelGeneration: MobileCaptureGenerationDto? = null
    private var researchDialog: android.app.Dialog? = null
    private val activeCapture = ActiveCapturePanel(context, { expected, completed ->
        val current = captureState.lifecycle.attempt
        if (!disposed && current?.generation == expected && current.origin == MobileCaptureOriginDto.MANUAL) {
            bluetooth.finishManualCapture(completed)
        } else completed(false)
    }) {
        if (!disposed) Toast.makeText(context, R.string.capture_save_failed, Toast.LENGTH_LONG).show()
    }
    private val settingsSummary = CaptureSettingsObservationSummary(context, { done ->
        val generation = captureState.lifecycle.attempt?.generation?.copy()
        if (generation == null) done(Result.failure(IllegalStateException("No active capture")))
        else bluetooth.diagnostics.activeSettings(generation, done)
    }, {
        if (!disposed && showsActive) captureState.lifecycle.attempt?.generation?.copy()?.let(::showActiveSettingsReview)
    })
    private var renderedObservations = captureState.settingsObservations.toList()
    private var captureGeneration = captureState.lifecycle.attempt?.generation?.copy()
    private val captureObserver = bluetooth.diagnostics.observe { next ->
        renderCapture(next)
    }
    private val bleObserver = bluetooth.addObserver { next ->
        val relevantChange = next.phase != bleState.phase || next.devices.map { it.id to (it.advertisedName ?: it.name) } !=
            bleState.devices.map { it.id to (it.advertisedName ?: it.name) }
        if (!disposed) { bleState = next; if (relevantChange) scheduleRender() }
    }

    init {
        setBackgroundColor(ui.background)
        addView(library, LayoutParams(-1, -1))
        addView(activeCapture.footer, LayoutParams(-1, -2, android.view.Gravity.BOTTOM).apply {
            gravity = android.view.Gravity.BOTTOM
        })
        activeCapture.footer.elevation = ui.dp(4).toFloat()
        activeCapture.footer.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> updateFooterInset() }
        content.tag = "captures.home"
        render()
        refresh()
    }

    fun refresh() {
        if (disposed) return
        bluetooth.diagnostics.refresh()
    }

    fun dispose() {
        disposed = true
        renderHandler.removeCallbacks(queuedRender)
        elapsedHandler.removeCallbacks(elapsedTick)
        captureObserver.close()
        bleObserver.close()
        researchDialog?.dismiss()
        researchDialog = null
        detail?.dispose()
        detail = null
        artifactDetail?.dispose()
        artifactDetail = null
        activeCapture.dispose()
        settingsSummary.dispose()
    }

    /** Progress and admitted label updates retain controls and their accessibility focus. */
    internal fun renderCapture(next: DiagnosticCapturePresentation) {
        if (disposed) return
        val nextGeneration = next.lifecycle.attempt?.generation?.copy()
        if (nextGeneration != captureGeneration) {
            captureGeneration = nextGeneration
            settingsSummary.reset(next.settingsObservations)
            renderedObservations = next.settingsObservations.toList()
        }
        fun structure(value: DiagnosticCapturePresentation) = value.copy(
            writerStatus = null, notificationCount = 0UL, activeLabels = emptyList(),
            settingsObservations = emptyList(), startedElapsedMillis = null, finishedElapsedMillis = null,
        )
        val rebuild = structure(next) != structure(captureState) ||
            (next.writerStatus == null) != (captureState.writerStatus == null) ||
            next.writerStatus?.lastError != captureState.writerStatus?.lastError
        captureState = next
        if (rebuild) scheduleRender()
        labels?.render(next)
        if (next.settingsObservations != renderedObservations) {
            renderedObservations = next.settingsObservations.toList()
            settingsSummary.render(renderedObservations)
        }
        if (showsActive && hasActiveCapture()) {
            activeCapture.render(next, showFooter = true)
            ensureElapsedTick()
        } else elapsedHandler.removeCallbacks(elapsedTick)
    }

    private fun scheduleRender() {
        renderHandler.removeCallbacks(queuedRender)
        renderHandler.postDelayed(queuedRender, 150L)
    }

    private fun render() {
        if (disposed) return
        content.removeAllViews()
        elapsedHandler.removeCallbacks(elapsedTick)
        content.setPadding(ui.dp(20), ui.dp(12), ui.dp(20), ui.dp(28))
        val lifecycle = captureState.lifecycle
        val attempt = lifecycle.attempt
        val hasActive = attempt != null && attempt.stage !in setOf(MobileCaptureStageDto.SAVED, MobileCaptureStageDto.FAILED)
        if (showsActive && !hasActive) showsActive = false
        content.addView(ui.title(if (showsActive) ui.label("captures.active") else ui.label("captures.title")))
        content.addView(ui.space(18))

        if (hasActive && !showsActive) {
            section("") {
                addView(ui.navigationRow(ui.label("captures.active"), CutoutSymbol.SIGNAL) {
                    showsActive = true
                    library.requestFocus()
                    library.scrollTo(0, 0)
                    settingsSummary.refresh()
                    render()
                }.apply { tag = "captures.active.open" })
                captureState.deviceName?.takeIf(String::isNotBlank)?.let {
                    addView(ui.text(it, 14f, color = ui.muted))
                }
            }
            content.addView(ui.space(18))
        }
        if (showsActive && attempt != null) {
            section("") {
                (activeCapture.header.parent as? ViewGroup)?.removeView(activeCapture.header)
                addView(activeCapture.header)
                captureState.fileName?.let { addView(ui.text(it, 13f, color = ui.muted)) }
                captureState.note?.takeIf { !it.isNullOrBlank() && it != captureState.deviceKind }?.let {
                    addView(ui.text(it, 14f, color = ui.muted))
                }
            }
            content.addView(ui.space(14))
            if (labelGeneration != attempt.generation) {
                labelGeneration = attempt.generation.copy()
                labels = CaptureLabelControls(context, attempt.generation.copy(), bluetooth.diagnostics::changeLabel) { error ->
                    if (!disposed && captureState.lifecycle.attempt?.generation == attempt.generation) {
                        val key = when (error) {
                            is MobileCaptureAnnotationException.CapacityReached -> "captures.labels_full"
                            is MobileCaptureAnnotationException.NotRecording -> "captures.labels_not_recording"
                            else -> "captures.labels_failed"
                        }
                        AlertDialog.Builder(context).setTitle(ui.label("captures.label_not_recorded"))
                            .setMessage(ui.label(key)).setPositiveButton(android.R.string.ok, null).show()
                    }
                }
            }
            section("", "captures.observed-activity.section") {
                checkNotNull(labels).let { controls ->
                    (controls.parent as? ViewGroup)?.removeView(controls)
                    controls.render(captureState)
                    addView(controls, LinearLayout.LayoutParams(-1, -2))
                }
            }
            content.addView(ui.space(14))
            val canAnnotate = attempt.stage in setOf(MobileCaptureStageDto.RECORDING, MobileCaptureStageDto.SAVE_FAILED)
            section("Settings & actions", "capture.settings.actions.section") {
                addView(ui.action(ui.label("capture.settings.worksheet.title")) {
                    val expected = attempt.generation.copy()
                    researchDialog?.dismiss()
                    researchDialog = CaptureSettingsWorksheet(context) { draft, done ->
                        bluetooth.diagnostics.recordSettings(expected, draft) { result ->
                            done(result)
                            if (!disposed && result.isSuccess && captureState.lifecycle.attempt?.generation == expected) {
                                settingsSummary.refresh()
                            }
                        }
                    }.show()
                }.apply {
                    tag = "capture.settings.open"
                    isEnabled = canAnnotate
                    alpha = if (isEnabled) 1f else .45f
                })
                addView(ui.space(8))
                (settingsSummary.parent as? ViewGroup)?.removeView(settingsSummary)
                addView(settingsSummary, LinearLayout.LayoutParams(-1, -2))
                addView(ui.space(8))
                checkNotNull(labels).let { controls ->
                    (controls.actionMarkersSection.parent as? ViewGroup)?.removeView(controls.actionMarkersSection)
                    addView(controls.actionMarkersSection, LinearLayout.LayoutParams(-1, -2))
                }
            }
            content.addView(ui.space(14))
            section("") {
                (activeCapture.technical.parent as? ViewGroup)?.removeView(activeCapture.technical)
                addView(activeCapture.technical)
            }
            captureState.error?.let { content.addView(ui.text(it, 14f, color = ui.red)) }
            activeCapture.render(captureState, showFooter = true)
            updateFooterInset()
            ensureElapsedTick()
            onTitleChanged(navigationTitle)
            return
        }
        activeCapture.render(captureState, showFooter = false)
        updateFooterInset()

        section("") {
            addView(ui.action(ui.label("captures.new"), primary = true) {
                if (!bluetooth.hasPermissions() || !bluetooth.isEnabled()) onRequestBluetooth()
                else chooseDevice(lifecycle)
            }.apply {
                tag = "captures.manual.start"
                isEnabled = captureState.ready && lifecycle.canStart && bleState.phase == NativePhase.IDLE
                alpha = if (isEnabled) 1f else .45f
            })
            if (!lifecycle.canStart) addView(ui.text(stageLabel(attempt?.stage), 14f, color = ui.muted))
        }

        (captureState.error ?: captureState.historyError)?.let { message ->
            content.addView(ui.space(12))
            content.addView(ui.text(message, 14f, color = ui.red))
        }

        content.addView(ui.space(22))
        section(context.getString(R.string.capture_recent)) {
            if (captureState.loading) addView(ProgressBar(context).apply { isIndeterminate = true })
            if (captureState.historyError != null) {
                addView(ui.action(ui.label("captures.retry")) { refresh() }.apply { tag = "captures.history.retry" })
            }
            if (captureState.captures.isEmpty() && !captureState.loading) {
                addView(ui.text(context.getString(R.string.capture_empty), 15f, color = ui.muted))
            }
            captureState.captures.forEachIndexed { index, entry ->
                if (index > 0) { addView(ui.space(8)); addView(ui.divider()); addView(ui.space(8)) }
                addView(historyRow(entry))
            }
            captureState.nextCursor?.let {
                addView(ui.action(context.getString(R.string.capture_load_more)) {
                    bluetooth.diagnostics.refreshHistory(it)
                }.apply { isEnabled = !captureState.loading })
            }
            addView(ui.space(10))
            addView(ui.text(context.getString(R.string.capture_privacy), 13f, color = ui.muted))
        }
        if (captureState.sessionArtifacts.isNotEmpty()) {
            content.addView(ui.space(18))
            section(context.getString(R.string.capture_this_session)) {
                captureState.sessionArtifacts.asReversed().forEachIndexed { index, artifact ->
                    if (index > 0) { addView(ui.space(8)); addView(ui.divider()); addView(ui.space(8)) }
                    addView(sessionArtifactRow(artifact))
                }
            }
        }
        content.addView(ui.space(20))
        if (detail == null && artifactDetail == null) onTitleChanged(navigationTitle)
    }

    private fun section(title: String, tag: String? = null, build: LinearLayout.() -> Unit) {
        val block = ui.column().apply {
            this.tag = tag
            if (title.isNotEmpty()) {
                addView(ui.text(title, 20f, true).apply {
                    isAccessibilityHeading = true
                    this.tag = if (tag == "capture.settings.actions.section") "capture.settings.actions.title" else null
                })
                addView(ui.space(9))
            }
            addView(ui.column().apply {
                setPadding(ui.dp(16), ui.dp(14), ui.dp(16), ui.dp(14))
                background = ui.rounded(22)
                build()
            })
        }
        content.addView(block)
    }

    private fun hasActiveCapture(): Boolean {
        val stage = captureState.lifecycle.attempt?.stage ?: return false
        return stage !in setOf(MobileCaptureStageDto.SAVED, MobileCaptureStageDto.FAILED)
    }

    private fun updateFooterInset() {
        val bottom = if (showsActive && activeCapture.footer.visibility == View.VISIBLE)
            activeCapture.footer.height + ui.dp(16) else ui.dp(28)
        if (content.paddingBottom != bottom) {
            content.setPadding(content.paddingLeft, content.paddingTop, content.paddingRight, bottom)
        }
    }

    private fun ensureElapsedTick() {
        if (disposed || !isAttachedToWindow || windowVisibility != View.VISIBLE || !showsActive || !hasActiveCapture()) return
        elapsedHandler.removeCallbacks(elapsedTick)
        elapsedHandler.postDelayed(elapsedTick, 1_000L)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        ensureElapsedTick()
    }

    override fun onDetachedFromWindow() {
        elapsedHandler.removeCallbacks(elapsedTick)
        super.onDetachedFromWindow()
    }

    override fun onWindowVisibilityChanged(visibility: Int) {
        super.onWindowVisibilityChanged(visibility)
        if (visibility == View.VISIBLE) ensureElapsedTick() else elapsedHandler.removeCallbacks(elapsedTick)
    }

    private fun stageLabel(stage: MobileCaptureStageDto?): String = when (stage) {
        null, MobileCaptureStageDto.STARTING -> context.getString(R.string.capture_starting)
        MobileCaptureStageDto.RECORDING -> context.getString(R.string.capture_recording)
        MobileCaptureStageDto.SAVING, MobileCaptureStageDto.FINALIZING -> context.getString(R.string.capture_saving)
        MobileCaptureStageDto.SAVE_FAILED, MobileCaptureStageDto.FAILED -> context.getString(R.string.capture_failed)
        MobileCaptureStageDto.SAVED -> context.getString(R.string.capture_recent)
    }

    /** Native navigation only; a detail view never owns the recording or transfer lifetime. */
    fun handleBack(): Boolean {
        if (detail != null || artifactDetail != null) {
            researchDialog?.dismiss()
            researchDialog = null
            detail?.dispose()
            detail = null
            artifactDetail?.dispose()
            artifactDetail = null
            removeAllViews()
            addView(library, LayoutParams(-1, -1))
            addView(activeCapture.footer, LayoutParams(-1, -2, android.view.Gravity.BOTTOM).apply {
                gravity = android.view.Gravity.BOTTOM
            })
            onTitleChanged(navigationTitle)
            return true
        }
        if (showsActive) {
            showsActive = false
            elapsedHandler.removeCallbacks(elapsedTick)
            settingsSummary.reset(captureState.settingsObservations)
            render()
            return true
        }
        return false
    }

    private fun openSaved(entry: MobileLiveCaptureHistoryEntryDto) {
        if (disposed) return
        val source = MobileCaptureSubmissionSourceDto.LiveCapture(entry.liveCaptureId)
        val owner = (context.applicationContext as CutoutApplication).captureSubmissions
        detail?.dispose()
        artifactDetail?.dispose()
        artifactDetail = null
        detail = SavedCaptureDetailPanel(context, entry, bluetooth.diagnostics::export, onShare,
            owner.controller(source), { showSettingsReview(source) })
        removeAllViews()
        addView(detail, LayoutParams(-1, -1))
        onTitleChanged(navigationTitle)
    }

    private fun openArtifact(artifact: DiagnosticCaptureArtifact) {
        if (disposed) return
        val source = MobileCaptureSubmissionSourceDto.RecordedArtifact(artifact.id)
        val owner = (context.applicationContext as CutoutApplication).captureSubmissions
        detail?.dispose()
        detail = null
        artifactDetail?.dispose()
        artifactDetail = ArtifactCaptureDetailPanel(
            context, artifact, bluetooth.diagnostics::exportSessionArtifact, onShare,
            owner.controller(source), { showSettingsReview(source) },
        )
        removeAllViews()
        addView(artifactDetail, LayoutParams(-1, -1))
        onTitleChanged(navigationTitle)
    }

    private fun historyRow(entry: MobileLiveCaptureHistoryEntryDto): View {
        val row = ui.row().apply {
            minimumHeight = ui.dp(64)
            setPadding(0, ui.dp(8), 0, ui.dp(8))
            isClickable = true
            isFocusable = true
            tag = "captures.database.${entry.liveCaptureId}"
            setOnClickListener { openSaved(entry) }
        }
        row.addView(ui.column().apply {
            addView(ui.text(entry.recording?.advertisedName ?: entry.platformIdentifier, 17f, true))
            val started = Date(entry.startedAtMilliseconds.toLong())
            addView(ui.text(DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(started), 14f, color = ui.muted))
            val status = if (entry.interrupted) context.getString(R.string.capture_interrupted) else when (entry.integrity) {
                MobileCaptureIntegrityDto.Complete -> null
                is MobileCaptureIntegrityDto.Incomplete -> context.getString(R.string.capture_incomplete)
                MobileCaptureIntegrityDto.Unknown -> context.getString(R.string.capture_integrity_unknown)
            }
            status?.let { addView(ui.text(it, 14f, color = ui.orange)) }
        }, LinearLayout.LayoutParams(0, -2, 1f))
        row.addView(ui.icon(CutoutSymbol.CHEVRON, ui.muted, 16))
        fun textIn(view: View): List<String> = when (view) {
            is TextView -> listOf(view.text.toString())
            is ViewGroup -> (0 until view.childCount).flatMap { textIn(view.getChildAt(it)) }
            else -> emptyList()
        }
        row.contentDescription = textIn(row).joinToString(", ")
        row.descendantFocusability = ViewGroup.FOCUS_BLOCK_DESCENDANTS
        for (index in 0 until row.childCount) row.getChildAt(index).importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
        row.accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                super.onInitializeAccessibilityNodeInfo(host, info)
                info.className = Button::class.java.name
            }
        }
        return row
    }

    private fun sessionArtifactRow(artifact: DiagnosticCaptureArtifact): View {
        val title = artifact.deviceName?.takeIf(String::isNotBlank) ?: context.getString(R.string.capture_untitled)
        val started = Date(artifact.startedAtUnixMillis.toLong())
        val dateText = DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(started)
        val row = ui.column().apply {
            minimumHeight = ui.dp(64)
            setPadding(ui.dp(12), ui.dp(10), ui.dp(12), ui.dp(10))
            background = ui.rounded(14)
            isClickable = true
            isFocusable = true
            tag = "captures.session.${artifact.id}"
            setOnClickListener { openArtifact(artifact) }
        }
        row.addView(ui.text(title, 17f, true))
        row.addView(ui.text(dateText, 14f, color = ui.muted))
        artifact.description?.takeIf(String::isNotBlank)?.let { row.addView(ui.text(it, 14f, color = ui.muted)) }
        if (artifact.status.failed || artifact.status.droppedMessages > 0UL) {
            row.addView(ui.text(context.getString(R.string.capture_incomplete), 14f, color = ui.orange))
        }
        artifact.error?.let { row.addView(ui.text(it, 14f, color = ui.orange)) }
        row.contentDescription = buildList {
            add(title)
            add(dateText)
            artifact.description?.takeIf(String::isNotBlank)?.let(::add)
            artifact.error?.let(::add)
        }.joinToString(", ")
        return row
    }

    private fun showSettingsReview(source: MobileCaptureSubmissionSourceDto) {
        researchDialog?.dismiss()
        researchDialog = CaptureSettingsReview(context,
            { done -> bluetooth.diagnostics.savedSettings(source, done) },
            { requests, done -> bluetooth.diagnostics.savedSettingsReadback(source, requests, done) },
        ).show()
    }

    private fun showActiveSettingsReview(generation: MobileCaptureGenerationDto) {
        if (disposed || captureState.lifecycle.attempt?.generation != generation) return
        researchDialog?.dismiss()
        researchDialog = CaptureSettingsReview(context,
            { done -> bluetooth.diagnostics.activeSettings(generation, done) },
            { requests, done -> bluetooth.diagnostics.activeSettingsReadback(generation, requests, done) },
        ).show()
    }

    private fun chooseDevice(lifecycle: MobileCaptureLifecycleSnapshotDto) {
        val candidates = bleState.devices.filter { !it.id.isBlank() }
        if (candidates.isEmpty()) {
            researchDialog?.dismiss()
            researchDialog = AlertDialog.Builder(context).setMessage(R.string.capture_no_devices)
                .setPositiveButton(R.string.capture_scan) { _, _ -> requestBluetoothOrScan() }
                .setNegativeButton(android.R.string.cancel, null).show()
            return
        }
        val labels = candidates.map { item ->
            val name = item.advertisedName?.takeIf(String::isNotBlank) ?: item.name
            "$name  ·  ${item.id.takeLast(8)}"
        }.toTypedArray()
        researchDialog?.dismiss()
        researchDialog = AlertDialog.Builder(context)
            .setTitle(R.string.capture_choose_device)
            .setItems(labels) { _, which -> descriptionDialog(candidates[which], lifecycle) }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun descriptionDialog(device: NearbyPev, lifecycle: MobileCaptureLifecycleSnapshotDto) {
        val input = EditText(context).apply {
            hint = context.getString(R.string.capture_description)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES or InputType.TYPE_TEXT_FLAG_MULTI_LINE
            minLines = 2
            maxLines = 4
        }
        val wrapper = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(ui.dp(24), ui.dp(8), ui.dp(24), 0)
            addView(input, LinearLayout.LayoutParams(-1, -2))
        }
        val name = device.advertisedName?.takeIf(String::isNotBlank) ?: device.name
        researchDialog?.dismiss()
        researchDialog = AlertDialog.Builder(context).setTitle(name).setView(wrapper)
            .setPositiveButton(R.string.capture_start, null)
            .setNegativeButton(android.R.string.cancel, null)
            .create().also { dialog ->
                dialog.setOnShowListener {
                    dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                        if (disposed || !lifecycle.canStart || bleState.phase != NativePhase.IDLE) return@setOnClickListener
                        bluetooth.startManualCapture(device.id, null, input.text.toString()) { started ->
                            if (!disposed) {
                                if (!started) Toast.makeText(context, R.string.capture_start_failed, Toast.LENGTH_LONG).show()
                                else {
                                    showsActive = true
                                    settingsSummary.refresh()
                                }
                                refresh()
                            }
                        }
                        dialog.dismiss()
                    }
                }
            }.also { it.show() }
    }

    private fun requestBluetoothOrScan() {
        if (disposed) return
        if (!bluetooth.hasPermissions() || !bluetooth.isEnabled()) onRequestBluetooth()
        else bluetooth.scan()
    }
}
