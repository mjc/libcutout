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
    private val onAccountDeleted: () -> Unit = {},
) : ScrollView(context) {
    private enum class SavedDeviceRetryAction { LOAD, FORGET }

    private val ui = CutoutUi(context)
    private val page = ui.column()
    private val autostartError = ui.text("", 14f, color = ui.red).apply { tag = "setup.ride-autostart.error" }
    private val retryAutostart = ui.action(ui.label("setup.ride_autostart.retry")) {
        reloadRideAutostart()
    }.apply { tag = "setup.ride-autostart.retry"; visibility = View.GONE }
    private val diagnosticsError = ui.text("", 14f, color = ui.red).apply { tag = "setup.diagnostics.error" }
    private val retryDiagnostics = ui.action(ui.label("setup.diagnostics.retry")) {
        reloadDiagnostics()
    }.apply { tag = "setup.diagnostics.retry"; visibility = View.GONE }
    private val savedDeviceSection = ui.column().apply {
        tag = "setup.saved_device.section"
        background = ui.rounded(18)
        setPadding(ui.dp(16), ui.dp(12), ui.dp(16), ui.dp(12))
        addView(ui.text(ui.label("setup.saved_device.title"), 17f, true), LayoutParams(-1, -2))
        addView(ui.space(8))
    }
    private val savedDeviceStatus = ui.text(ui.label("setup.saved_device.loading"), 14f, color = ui.muted).apply {
        tag = "setup.saved_device.status"
    }
    private val forgetSavedDevice = ui.action(ui.label("setup.saved_device.forget")) {
        val dialog = android.app.AlertDialog.Builder(context)
            .setTitle(ui.label("setup.saved_device.confirm_title"))
            .setMessage(ui.label("setup.saved_device.confirm_message"))
            .setNegativeButton(R.string.cancel, null)
            .setPositiveButton(ui.label("setup.saved_device.forget"), null)
            .create()
        savedDeviceDialog = dialog
        dialog.setOnDismissListener { if (savedDeviceDialog === dialog) savedDeviceDialog = null }
        dialog.setOnShowListener {
            dialog.getButton(android.app.AlertDialog.BUTTON_POSITIVE).setOnClickListener {
                dialog.dismiss()
                if (!disposed) forgetDevice()
            }
        }
        dialog.show()
    }.apply {
        tag = "setup.saved_device.forget"
        visibility = View.GONE
    }
    private val retryForgetSavedDevice = ui.action(ui.label("setup.saved_device.retry")) {
        when (savedDeviceRetryAction) {
            SavedDeviceRetryAction.LOAD -> refreshSavedDevice()
            SavedDeviceRetryAction.FORGET -> forgetDevice()
            null -> Unit
        }
    }.apply {
        tag = "setup.saved_device.retry"
        visibility = View.GONE
    }
    private var savedDeviceBusy = false
    private var savedDeviceRetryAction: SavedDeviceRetryAction? = null
    private var savedDeviceReadGeneration = 0L
    private var disposed = false
    private var savedDeviceDialog: android.app.AlertDialog? = null
    private var binding = false
    private val autostart = toggle(ui.label("setup.ride_autostart")) { enabled ->
        app.rides.withCore({ it.setRideAutostartEnabled(enabled) }) { result ->
            finishAutostartChange(result)
        }
    }.apply {
        tag = "setup.ride-autostart"
        accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                super.onInitializeAccessibilityNodeInfo(host, info)
                info.hintText = ui.label("setup.ride_autostart.hint")
                info.isShowingHintText = true
            }
        }
    }
    private val diagnostics = toggle(ui.label("setup.diagnostics.automatic")) { enabled ->
        app.bluetooth.setDiagnosticRecordingEnabled(enabled) { result ->
            finishDiagnosticsChange(result)
        }
    }.apply { tag = "setup.diagnostics.automatic" }
    private val accountStatus = ui.text(context.getString(R.string.setup_account_loading), 14f, color = ui.muted).apply {
        tag = "setup.account.status"
    }
    private val accountError = ui.text("", 13f, color = ui.red).apply { tag = "setup.account.error" }
    private val accountReload = ui.action(context.getString(R.string.setup_account_reload)) { refreshAccount() }.apply { visibility = View.GONE }
    private val accountDelete = ui.action(context.getString(R.string.setup_account_delete), primary = true) {
        android.app.AlertDialog.Builder(context)
            .setTitle(R.string.setup_account_confirm_title)
            .setMessage(listOf(
                ui.label("setup.account.confirm_message"),
                ui.label("setup.account.before_delete"),
                ui.label("setup.account.local_data"),
                ui.label("setup.account.retention"),
            ).joinToString("\n\n"))
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
        savedDeviceSection.addView(savedDeviceStatus, LinearLayout.LayoutParams(-1, -2))
        savedDeviceSection.addView(forgetSavedDevice, LinearLayout.LayoutParams(-1, -2))
        savedDeviceSection.addView(retryForgetSavedDevice, LinearLayout.LayoutParams(-1, -2))
        savedDeviceSection.visibility = View.GONE
        page.addView(savedDeviceSection)
        page.addView(ui.space(20))
        page.addView(autostart)
        page.addView(autostartError)
        autostartError.visibility = View.GONE
        page.addView(retryAutostart)
        page.addView(ui.space(20))
        page.addView(ui.action(context.getString(R.string.music), onClick = onMusic))
        page.addView(ui.space(20))
        page.addView(ui.text(context.getString(R.string.setup_diagnostics), 17f, true))
        page.addView(diagnostics)
        page.addView(ui.text(ui.label("setup.diagnostics.explanation"), 14f, color = ui.muted).apply {
            tag = "setup.diagnostics.explanation"
        })
        page.addView(diagnosticsError)
        diagnosticsError.visibility = View.GONE
        page.addView(retryDiagnostics)
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
        reload()
    }

    fun reload() {
        refreshSavedDevice()
        refreshAccount()
        autostart.isEnabled = false; diagnostics.isEnabled = false
        loadRideAutostart()
        loadDiagnostics()
    }

    /** Stop rendering asynchronous results after the owning Activity discards this panel. */
    fun dispose() {
        disposed = true
        savedDeviceDialog?.dismiss()
        savedDeviceDialog = null
    }

    private fun refreshSavedDevice() {
        if (savedDeviceBusy) return
        savedDeviceRetryAction = null
        retryForgetSavedDevice.visibility = View.GONE
        val generation = ++savedDeviceReadGeneration
        savedDeviceStatus.text = ui.label("setup.saved_device.loading")
        app.bluetooth.savedDevice { result ->
            if (disposed || generation != savedDeviceReadGeneration) return@savedDevice
            result.onSuccess { id ->
                val present = id != null
                savedDeviceRetryAction = null
                savedDeviceSection.visibility = if (present || savedDeviceBusy) View.VISIBLE else View.GONE
                if (present) {
                    savedDeviceStatus.text = ui.label("setup.saved_device.status")
                    forgetSavedDevice.visibility = if (savedDeviceBusy) View.GONE else View.VISIBLE
                    forgetSavedDevice.isEnabled = !savedDeviceBusy
                } else {
                    forgetSavedDevice.visibility = View.GONE
                    retryForgetSavedDevice.visibility = View.GONE
                }
            }.onFailure {
                savedDeviceSection.visibility = View.VISIBLE
                savedDeviceStatus.text = ui.label("setup.saved_device.load_failed")
                forgetSavedDevice.visibility = View.GONE
                savedDeviceRetryAction = SavedDeviceRetryAction.LOAD
                retryForgetSavedDevice.visibility = View.VISIBLE
                retryForgetSavedDevice.isEnabled = true
            }
        }
    }

    private fun forgetDevice() {
        if (savedDeviceBusy || disposed) return
        savedDeviceBusy = true
        savedDeviceRetryAction = null
        retryForgetSavedDevice.visibility = View.GONE
        val generation = ++savedDeviceReadGeneration
        savedDeviceSection.visibility = View.VISIBLE
        savedDeviceStatus.text = ui.label("setup.saved_device.forgetting")
        forgetSavedDevice.isEnabled = false
        retryForgetSavedDevice.isEnabled = false
        app.bluetooth.forgetSavedDevice { result ->
            if (disposed || generation != savedDeviceReadGeneration) return@forgetSavedDevice
            savedDeviceBusy = false
            if (result.isSuccess) {
                savedDeviceRetryAction = null
                savedDeviceStatus.text = ui.label("setup.saved_device.success")
                forgetSavedDevice.visibility = View.GONE
                retryForgetSavedDevice.visibility = View.GONE
            } else {
                savedDeviceRetryAction = SavedDeviceRetryAction.FORGET
                savedDeviceStatus.text = ui.label("setup.saved_device.failure")
                forgetSavedDevice.visibility = View.GONE
                retryForgetSavedDevice.visibility = View.VISIBLE
                retryForgetSavedDevice.isEnabled = true
            }
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
            if (disposed) return@refreshAccountCredentialState
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
        app.charging.deleteAccount(::accountDeletionCompleted)
    }

    internal fun accountDeletionCompleted(result: Result<uniffi.cutout_mobile_ffi.MobileCommunityAccountDeletionReceiptDto>) {
        accountBusy = false
        result.onSuccess {
            // The app-level effect is receipt-gated and must survive leaving Setup
            // while the Rust-owned deletion is still in flight.
            onAccountDeleted()
            if (disposed) return
            accountState = app.charging.status.credentialState
            accountStatus.text = context.getString(R.string.setup_account_deleted)
            accountError.visibility = View.GONE
        }.onFailure {
            if (disposed) return
            accountState = app.charging.status.credentialState
            accountStatus.text = context.getString(R.string.setup_account_retry_description)
            accountError.text = context.getString(R.string.setup_account_delete_failed)
            accountError.visibility = View.VISIBLE
        }
        if (disposed) return
        accountReload.isEnabled = true
        accountDelete.visibility = if (accountState == MobileChargingSyncCredentialState.DELETION_PENDING || accountState == MobileChargingSyncCredentialState.READY) View.VISIBLE else View.GONE
        accountDelete.text = context.getString(if (accountState == MobileChargingSyncCredentialState.DELETION_PENDING) R.string.setup_account_retry_delete else R.string.setup_account_delete)
        accountDelete.isEnabled = !accountBusy
        accountReload.visibility = View.VISIBLE
    }

    private fun section(title: Int, content: LinearLayout.() -> Unit): LinearLayout = ui.column().apply {
        background = ui.rounded(18)
        setPadding(ui.dp(16), ui.dp(12), ui.dp(16), ui.dp(12))
        addView(ui.text(context.getString(title), 15f, true), LayoutParams(-1, -2))
        addView(ui.space(8))
        content()
    }

    private fun finishAutostartChange(result: Result<*>) {
        if (disposed) return
        if (result.isSuccess) {
            clearAutostartFailure()
            loadRideAutostart()
        } else {
            showAutostartFailure(ui.label("setup.ride_autostart.save_failed"), canRetryLoad = false)
            loadRideAutostart(clearFailureOnSuccess = false)
        }
    }

    private fun finishDiagnosticsChange(result: Result<*>) {
        if (disposed) return
        if (result.isSuccess) {
            clearDiagnosticsFailure()
            loadDiagnostics()
        } else {
            showDiagnosticsFailure(ui.label("setup.diagnostics.failed"))
            loadDiagnostics(clearFailureOnSuccess = false)
        }
    }

    private fun reloadRideAutostart() {
        clearAutostartFailure()
        autostart.isEnabled = false
        loadRideAutostart()
    }

    private fun reloadDiagnostics() {
        clearDiagnosticsFailure()
        diagnostics.isEnabled = false
        loadDiagnostics()
    }

    private fun loadRideAutostart(clearFailureOnSuccess: Boolean = true) {
        app.rides.withCore({ it.rideAutostartEnabled() }) { result ->
            if (disposed) return@withCore
            result.onSuccess {
                binding = true
                autostart.isChecked = it
                binding = false
                autostart.isEnabled = true
                if (clearFailureOnSuccess) clearAutostartFailure()
            }.onFailure {
                autostart.isEnabled = false
                showAutostartFailure(ui.label("setup.ride_autostart.load_failed"))
            }
        }
    }

    private fun loadDiagnostics(clearFailureOnSuccess: Boolean = true) {
        app.bluetooth.diagnosticRecordingEnabled { result ->
            if (disposed) return@diagnosticRecordingEnabled
            result.onSuccess {
                binding = true
                diagnostics.isChecked = it
                binding = false
                diagnostics.isEnabled = true
                if (clearFailureOnSuccess) clearDiagnosticsFailure()
            }.onFailure {
                diagnostics.isEnabled = false
                showDiagnosticsFailure(ui.label("setup.diagnostics.failed"))
            }
        }
    }

    private fun showAutostartFailure(message: String, canRetryLoad: Boolean = true) {
        autostartError.text = message
        autostartError.visibility = View.VISIBLE
        retryAutostart.visibility = if (canRetryLoad) View.VISIBLE else View.GONE
    }

    private fun clearAutostartFailure() {
        autostartError.visibility = View.GONE
        retryAutostart.visibility = View.GONE
    }

    private fun showDiagnosticsFailure(message: String) {
        diagnosticsError.text = message
        diagnosticsError.visibility = View.VISIBLE
        retryDiagnostics.visibility = View.VISIBLE
    }

    private fun clearDiagnosticsFailure() {
        diagnosticsError.visibility = View.GONE
        retryDiagnostics.visibility = View.GONE
    }

    private fun toggle(title: String, changed: (Boolean) -> Unit) = Switch(context).apply {
        text = title; textSize = 17f; setTextColor(ui.foreground)
        setPadding(0, ui.dp(12), 0, ui.dp(12)); minHeight = ui.dp(48)
        isEnabled = false
        setOnCheckedChangeListener { _, selected -> if (!binding) { isEnabled = false; changed(selected) } }
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
