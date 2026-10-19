package lol.cutout

import android.content.Context
import android.view.View
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Switch
import uniffi.cutout_mobile_ffi.MobileChargingSyncCredentialState

/** General app settings stay separate from the device's ordinary configuration. */
class SetupPanel(
    context: Context,
    private val app: CutoutApplication,
    private val onMusic: () -> Unit,
    private val onCaptures: () -> Unit,
    private val onLocation: () -> Unit,
    private val onNotifications: () -> Unit,
    private val onOpenUrl: (String) -> Unit,
) : ScrollView(context) {
    private val ui = CutoutUi(context)
    private val page = ui.column()
    private val error = ui.text("", 14f, color = ui.orange)
    private val retrySettings = ui.action(context.getString(R.string.setup_retry)) { reload() }.apply {
        tag = "setup.retry"
        visibility = View.GONE
    }
    private var binding = false
    private val autostart = toggle(context.getString(R.string.setup_ride_autostart)) { enabled ->
        app.rides.withCore({ it.setRideAutostartEnabled(enabled) }) { result ->
            finishSettingChange(result)
        }
    }
    private val diagnostics = toggle(context.getString(R.string.setup_diagnostics_automatic)) { enabled ->
        app.bluetooth.setDiagnosticRecordingEnabled(enabled) { result ->
            finishSettingChange(result)
        }
    }
    private val accountStatus = ui.text(context.getString(R.string.setup_account_loading), 14f, color = ui.muted)
    private val accountError = ui.text("", 13f, color = ui.red)
    private val accountReload = ui.action(context.getString(R.string.setup_account_reload)) { refreshAccount() }.apply { visibility = View.GONE }
    private val accountDelete = ui.action(context.getString(R.string.setup_account_delete), primary = true) {
        android.app.AlertDialog.Builder(context)
            .setTitle(R.string.setup_account_confirm_title)
            .setMessage(R.string.setup_account_confirm_message)
            .setNegativeButton(R.string.cancel, null)
            .setPositiveButton(R.string.setup_account_confirm_delete) { _, _ -> deleteAccount() }
            .show()
    }.apply { visibility = View.GONE }
    private var accountState = MobileChargingSyncCredentialState.MISSING
    private var accountBusy = false

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
        page.addView(ui.text(context.getString(R.string.setup_diagnostics), 17f, true))
        page.addView(diagnostics)
        page.addView(ui.text(context.getString(R.string.setup_diagnostics_detail), 14f, color = ui.muted))
        page.addView(ui.action(context.getString(R.string.captures), onClick = onCaptures))
        page.addView(ui.space(20))
        page.addView(ui.action(context.getString(R.string.setup_location), onClick = onLocation))
        page.addView(ui.action(context.getString(R.string.setup_notifications), onClick = onNotifications))
        page.addView(ui.space(20))
        page.addView(ui.action(context.getString(R.string.setup_privacy)) { onOpenUrl("https://cutout.lol/privacy-policy") })
        page.addView(ui.action(context.getString(R.string.setup_support)) { onOpenUrl("https://cutout.lol/support") })
        page.addView(ui.space(20))
        page.addView(section(R.string.setup_account_section) {
            addView(accountStatus, LayoutParams(-1, -2))
            addView(accountError, LayoutParams(-1, -2))
            addView(accountReload, LayoutParams(-1, -2))
            addView(accountDelete, LayoutParams(-1, -2))
        })
        page.addView(error)
        error.visibility = View.GONE
        page.addView(retrySettings)
        reload()
    }

    fun reload(clearFailure: Boolean = true) {
        if (clearFailure) {
            error.visibility = View.GONE
            retrySettings.visibility = View.GONE
            refreshAccount()
        }
        autostart.isEnabled = false; diagnostics.isEnabled = false
        app.rides.withCore({ it.rideAutostartEnabled() }) { result ->
            result.onSuccess { binding = true; autostart.isChecked = it; binding = false; autostart.isEnabled = true }
                .onFailure { showFailure(R.string.setup_load_failed) }
        }
        app.bluetooth.diagnosticRecordingEnabled { result ->
            result.onSuccess { binding = true; diagnostics.isChecked = it; binding = false; diagnostics.isEnabled = true }
                .onFailure { showFailure(R.string.setup_load_failed) }
        }
    }

    private fun refreshAccount() {
        if (accountBusy) return
        accountBusy = true
        accountError.visibility = View.GONE
        accountReload.isEnabled = false
        accountDelete.isEnabled = false
        accountStatus.text = context.getString(R.string.setup_account_loading)
        app.charging.refreshAccountCredentialState { result ->
            accountBusy = false
            result.onSuccess { state ->
                accountState = state
                accountStatus.text = context.getString(state.accountStatusLabel())
                accountError.visibility = View.GONE
            }.onFailure {
                accountState = MobileChargingSyncCredentialState.UNREADABLE
                accountStatus.text = context.getString(R.string.setup_account_load_failed)
                accountError.text = context.getString(R.string.setup_account_load_failed)
                accountError.visibility = View.VISIBLE
            }
            accountReload.isEnabled = !accountBusy
            accountReload.visibility = if (accountError.visibility == View.VISIBLE || accountState == MobileChargingSyncCredentialState.REGISTRATION_STARTED) View.VISIBLE else View.GONE
            accountDelete.visibility = if (accountState == MobileChargingSyncCredentialState.READY || accountState == MobileChargingSyncCredentialState.DELETION_PENDING) View.VISIBLE else View.GONE
            accountDelete.text = context.getString(if (accountState == MobileChargingSyncCredentialState.DELETION_PENDING) R.string.setup_account_retry_delete else R.string.setup_account_delete)
            accountDelete.isEnabled = !accountBusy
            accountReload.text = context.getString(R.string.setup_account_reload)
        }
    }

    private fun deleteAccount() {
        accountBusy = true
        accountStatus.text = context.getString(R.string.setup_account_deleting)
        accountDelete.isEnabled = false
        app.charging.deleteAccount { result ->
            accountBusy = false
            result.onSuccess { receipt ->
                accountState = app.charging.status.credentialState
                accountStatus.text = context.getString(R.string.setup_account_deleted)
                accountError.visibility = View.GONE
            }.onFailure {
                accountState = app.charging.status.credentialState
                accountStatus.text = context.getString(R.string.setup_account_retry_description)
                accountError.text = context.getString(R.string.setup_account_delete_failed)
                accountError.visibility = View.VISIBLE
            }
            accountReload.isEnabled = true
            accountDelete.visibility = if (accountState == MobileChargingSyncCredentialState.DELETION_PENDING || accountState == MobileChargingSyncCredentialState.READY) View.VISIBLE else View.GONE
            accountDelete.text = context.getString(if (accountState == MobileChargingSyncCredentialState.DELETION_PENDING) R.string.setup_account_retry_delete else R.string.setup_account_delete)
            accountDelete.isEnabled = !accountBusy
            accountReload.visibility = View.VISIBLE
        }
    }

    private fun section(title: Int, content: LinearLayout.() -> Unit): LinearLayout = ui.column().apply {
        background = ui.rounded(18)
        setPadding(ui.dp(16), ui.dp(12), ui.dp(16), ui.dp(12))
        addView(ui.text(context.getString(title), 15f, true), LayoutParams(-1, -2))
        addView(ui.space(8))
        content()
    }

    private fun <T> finishSettingChange(result: Result<T>) {
        if (result.isSuccess) {
            reload()
        } else {
            showFailure()
            // Read back the effective value without clearing the write failure.
            reload(clearFailure = false)
        }
    }

    private fun toggle(title: String, changed: (Boolean) -> Unit) = Switch(context).apply {
        text = title; textSize = 17f; setTextColor(ui.foreground)
        setPadding(0, ui.dp(12), 0, ui.dp(12)); minHeight = ui.dp(48)
        isEnabled = false
        setOnCheckedChangeListener { _, selected -> if (!binding) { isEnabled = false; changed(selected) } }
    }

    private fun showFailure(message: Int = R.string.setup_save_failed) {
        error.text = context.getString(message)
        error.visibility = View.VISIBLE
        retrySettings.visibility = View.VISIBLE
    }
}

private fun MobileChargingSyncCredentialState.accountStatusLabel(): Int = when (this) {
    MobileChargingSyncCredentialState.MISSING -> R.string.setup_account_no_account
    MobileChargingSyncCredentialState.REGISTRATION_STARTED -> R.string.setup_account_registration_uncertain
    MobileChargingSyncCredentialState.READY -> R.string.setup_account_active
    MobileChargingSyncCredentialState.DELETION_PENDING -> R.string.setup_account_retry_description
    MobileChargingSyncCredentialState.DELETED -> R.string.setup_account_deleted
    MobileChargingSyncCredentialState.UNREADABLE -> R.string.setup_account_load_failed
}
