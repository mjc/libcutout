package lol.cutout

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import uniffi.cutout_mobile_ffi.*
import java.io.Closeable

/** Native presentation around Rust's per-ride charging-stop admission and suppression owner. */
internal class AndroidChargingStopPrompt(
    context: Context,
    private val runtime: RideMapRuntime,
    private val sessionState: () -> CutoutSessionStateHandle,
    private val currentLocation: () -> MobilePhoneLocationSampleDto?,
    private val preferences: android.content.SharedPreferences,
    private val canPresentNow: () -> Boolean,
    private val onAccept: (MobileChargingStopPromptDto) -> Boolean,
) : FrameLayout(context), Closeable {
    private data class Query(val ticket: Long, val context: MobileChargingStopContextDto)
    private data class VisibleCandidate(val value: MobileChargingStopPromptDto, val generation: Long)

    private val ui = CutoutUi(context)
    private val main = Handler(Looper.getMainLooper())
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val policy = MobileChargingStopPrompts(
        preferences.getString(HISTORY_KEY, null),
    )
    private val policyLifetime = ChargingStopPolicyLifetime {
        scope.cancel()
        policy.close()
    }
    private val heading: TextView = ui.text(ui.label("charging.stop.title"), 17f, true).apply {
        isAccessibilityHeading = true
        tag = "charging.stop.title"
    }
    private val dismissButton = ui.action(ui.label("charging.stop.dismiss")) { dismissPrompt() }.apply {
        tag = "charging.stop.dismiss"
        contentDescription = ui.label("charging.stop.dismiss")
    }
    private val acceptButton = ui.action(ui.label("charging.add"), primary = true) {
        val current = candidate ?: return@action
        if (isClosed || !canPresentNow() || current.generation != presentationGeneration) return@action
        if (onAccept(current.value)) dismissPrompt()
    }.apply {
        tag = "charging.stop.add"
        contentDescription = ui.label("charging.add")
    }
    private val card = ui.column().apply {
        background = ui.rounded(20)
        setPadding(ui.dp(16), ui.dp(12), ui.dp(16), ui.dp(12))
        addView(heading, LinearLayout.LayoutParams(-1, -2))
        val actions = ui.row().apply {
            gravity = Gravity.CENTER_VERTICAL
            addView(dismissButton, LinearLayout.LayoutParams(0, -2, 1f))
            addView(acceptButton, LinearLayout.LayoutParams(0, -2, 1f).apply {
                marginStart = ui.dp(8)
            })
        }
        addView(actions, LinearLayout.LayoutParams(-1, -2).apply { topMargin = ui.dp(6) })
    }

    private var active = false
    private var isClosed = false
    private var queryInFlight = false
    private var queryTicket = 0L
    private var nextQuery: Query? = null
    private var pendingPolicyDismiss = false
    private var presentationGeneration = 0L
    private var candidate: VisibleCandidate? = null
    private var acknowledgedGeneration: Long? = null

    internal val isPolicyReleased: Boolean get() = policyLifetime.isReleased()
    internal val isQueryIdle: Boolean get() = !queryInFlight && nextQuery == null

    init {
        tag = "charging.stop.prompt"
        visibility = View.GONE
        clipToPadding = false
        addView(card, LayoutParams(-1, -2, Gravity.BOTTOM))
        setPadding(ui.dp(16), ui.dp(8), ui.dp(16), ui.dp(12))
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
        addOnAttachStateChangeListener(object : OnAttachStateChangeListener {
            override fun onViewAttachedToWindow(view: View) = acknowledgeWhenVisible()
            override fun onViewDetachedFromWindow(view: View) = Unit
        })
        addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> acknowledgeWhenVisible() }
    }

    /** Called by the Activity's foreground-only two-second clock. Concurrent reads coalesce. */
    fun refresh(canPresent: Boolean) {
        if (isClosed) return
        if (!canPresent || !canPresentNow()) {
            deactivate()
            return
        }
        active = true
        val query = Query(++queryTicket, MobileChargingStopContextDto(
            location = currentLocation(),
            nowMs = SystemClock.elapsedRealtime().toULong(),
            nowUnixSeconds = System.currentTimeMillis() / 1_000.0,
            canPresent = true,
            phoneConnectedToPower = isConnectedToExternalPower(),
        ))
        if (queryInFlight) {
            nextQuery = query
        } else {
            observe(query)
        }
    }

    private fun observe(query: Query) {
        if (!policyLifetime.begin()) return
        queryInFlight = true
        runtime.withCore({ it }) { coreResult ->
            if (isClosed || query.ticket != queryTicket) {
                finishQuery()
                return@withCore
            }
            coreResult.fold(
                onSuccess = { core ->
                    runtime.withDatabase({ sessionState() }) { sessionResult ->
                        if (isClosed || query.ticket != queryTicket) {
                            finishQuery()
                            return@withDatabase
                        }
                        sessionResult.fold(
                            onSuccess = { session ->
                                scope.launch {
                                    val result = try {
                                        Result.success(policy.observe(core, session, query.context))
                                    } catch (error: Exception) {
                                        Result.failure(error)
                                    }
                                    main.post {
                                        if (!isClosed && query.ticket == queryTicket) {
                                            result.onSuccess(::applyDecision)
                                        }
                                        finishQuery()
                                    }
                                }
                            },
                            onFailure = {
                                finishQuery()
                            },
                        )
                    }
                },
                onFailure = { finishQuery() },
            )
        }
    }

    private fun finishQuery() {
        if (!queryInFlight) return
        queryInFlight = false
        if (pendingPolicyDismiss && !isClosed) {
            pendingPolicyDismiss = false
            policy.dismiss()
        }
        policyLifetime.finish()
        if (isClosed) return
        val pending = nextQuery
        nextQuery = null
        if (pending != null && !isClosed && active && pending.ticket == queryTicket) observe(pending)
        else candidate?.let { current ->
            if (current.generation != acknowledgedGeneration) post { acknowledgeWhenVisible(current) }
        }
    }

    private fun applyDecision(decision: MobileChargingStopDecisionDto) {
        if (!active || !canPresentNow()) {
            deactivate()
            return
        }
        val offered = (decision as? MobileChargingStopDecisionDto.Offer)?.prompt
        if (offered == null) {
            retireCandidate()
            return
        }
        val current = candidate
        if (current?.value != offered) {
            presentationGeneration++
            acknowledgedGeneration = null
            val admitted = VisibleCandidate(offered, presentationGeneration)
            candidate = admitted
            visibility = View.VISIBLE
            post { acknowledgeWhenVisible(admitted) }
        }
    }

    private fun acknowledgeWhenVisible() {
        candidate?.let(::acknowledgeWhenVisible)
    }

    private fun acknowledgeWhenVisible(expected: VisibleCandidate) {
        val current = candidate ?: return
        if (current.generation != expected.generation || current.value != expected.value) return
        if (isClosed || !active || acknowledgedGeneration == current.generation) return
        if (queryInFlight) return
        if (!canPresentNow() || !isShown || windowVisibility != View.VISIBLE || width <= 0 || height <= 0) return
        val history = policy.presented(current.value)
        if (history == null) {
            retireCandidate()
            return
        }
        preferences.edit().putString(HISTORY_KEY, history).apply()
        acknowledgedGeneration = current.generation
        announceForAccessibility(heading.text)
    }

    private fun dismissPrompt() {
        if (isClosed) return
        queryTicket++
        nextQuery = null
        dismissPolicyWhenIdle()
        retireCandidate()
    }

    private fun deactivate() {
        if (!active && candidate == null && nextQuery == null) return
        active = false
        queryTicket++
        nextQuery = null
        dismissPolicyWhenIdle()
        retireCandidate()
    }

    private fun dismissPolicyWhenIdle() {
        if (queryInFlight) pendingPolicyDismiss = true else policy.dismiss()
    }

    private fun retireCandidate() {
        presentationGeneration++
        candidate = null
        acknowledgedGeneration = null
        visibility = View.GONE
    }

    private fun isConnectedToExternalPower(): Boolean {
        val battery = context.registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED)) ?: return false
        val status = battery.getIntExtra(BatteryManager.EXTRA_STATUS, BatteryManager.BATTERY_STATUS_UNKNOWN)
        val plugged = battery.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0)
        return plugged != 0 && (status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL)
    }

    override fun close() {
        if (isClosed) return
        isClosed = true
        active = false
        queryTicket++
        nextQuery = null
        candidate = null
        acknowledgedGeneration = null
        visibility = View.GONE
        policyLifetime.close()
    }

    companion object { private const val HISTORY_KEY = "charging-stop-prompt-history" }
}

/** Defers releasing the native policy handle until its sole asynchronous observation completes. */
internal class ChargingStopPolicyLifetime(private val release: () -> Unit) {
    private var inFlight = false
    private var closeRequested = false
    private var released = false

    @Synchronized fun isReleased(): Boolean = released

    @Synchronized fun begin(): Boolean {
        if (closeRequested || released || inFlight) return false
        inFlight = true
        return true
    }

    @Synchronized fun finish() {
        if (!inFlight) return
        inFlight = false
        releaseIfReady()
    }

    @Synchronized fun close() {
        closeRequested = true
        releaseIfReady()
    }

    private fun releaseIfReady() {
        if (!closeRequested || inFlight || released) return
        released = true
        release()
    }
}
