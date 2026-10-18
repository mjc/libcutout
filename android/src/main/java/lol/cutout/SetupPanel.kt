package lol.cutout

import android.content.Context
import android.view.View
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch

/** General app settings stay separate from the device's ordinary configuration. */
class SetupPanel(
    context: Context,
    private val app: CutoutApplication,
    private val onMusic: () -> Unit,
    private val onLocation: () -> Unit,
    private val onNotifications: () -> Unit,
    private val onOpenUrl: (String) -> Unit,
) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val page = ui.column()
    private val error = ui.text("", 14f, color = ui.orange)
    private var binding = false
    private val phoneAlarms = PhoneAlarmPanel(context, app.phoneAlarms, onNotifications)
    private val autostart = toggle(context.getString(R.string.setup_ride_autostart)) { enabled ->
        app.rides.withCore({ it.setRideAutostartEnabled(enabled) }) { result ->
            result.onFailure { showFailure() }; reload()
        }
    }
    private val diagnostics = toggle(context.getString(R.string.setup_diagnostics_automatic)) { enabled ->
        app.bluetooth.setDiagnosticRecordingEnabled(enabled) { result ->
            result.onFailure { showFailure() }; reload()
        }
    }

    init {
        setBackgroundColor(ui.background)
        page.setPadding(ui.dp(24), ui.dp(20), ui.dp(24), ui.dp(28))
        addView(page)
        page.addView(ui.title(context.getString(R.string.setup)))
        page.addView(ui.space(20))
        page.addView(autostart)
        page.addView(ui.space(20))
        page.addView(ui.action(context.getString(R.string.music), onClick = onMusic))
        page.addView(ui.space(20))
        page.addView(phoneAlarms, LinearLayout.LayoutParams(-1, -2))
        page.addView(ui.space(20))
        page.addView(ui.text(context.getString(R.string.setup_diagnostics), 17f, true))
        page.addView(diagnostics)
        page.addView(ui.text(context.getString(R.string.setup_diagnostics_detail), 14f, color = ui.muted))
        page.addView(ui.space(20))
        page.addView(ui.action(context.getString(R.string.setup_location), onClick = onLocation))
        page.addView(ui.action(context.getString(R.string.setup_notifications), onClick = onNotifications))
        page.addView(ui.space(20))
        page.addView(ui.action(context.getString(R.string.setup_privacy)) { onOpenUrl("https://cutout.lol/privacy-policy") })
        page.addView(ui.action(context.getString(R.string.setup_support)) { onOpenUrl("https://cutout.lol/support") })
        page.addView(error)
        error.visibility = View.GONE
        reload()
    }

    fun reload() {
        app.refreshPhoneAlarmAuthorization()
        autostart.isEnabled = false; diagnostics.isEnabled = false
        app.rides.withCore({ it.rideAutostartEnabled() }) { result ->
            result.onSuccess { binding = true; autostart.isChecked = it; binding = false; autostart.isEnabled = true }
                .onFailure { showFailure() }
        }
        app.bluetooth.diagnosticRecordingEnabled { result ->
            result.onSuccess { binding = true; diagnostics.isChecked = it; binding = false; diagnostics.isEnabled = true }
                .onFailure { showFailure() }
        }
    }

    fun dispose() { phoneAlarms.dispose() }

    private fun toggle(title: String, changed: (Boolean) -> Unit) = Switch(context).apply {
        text = title; textSize = 17f; setTextColor(ui.foreground)
        setPadding(0, ui.dp(12), 0, ui.dp(12)); minHeight = ui.dp(48)
        isEnabled = false
        setOnCheckedChangeListener { _, selected -> if (!binding) { isEnabled = false; changed(selected) } }
    }

    private fun showFailure() { error.text = context.getString(R.string.setup_save_failed); error.visibility = View.VISIBLE }
}
