package lol.cutout

import android.Manifest
import android.app.Activity
import android.bluetooth.BluetoothAdapter
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.drawable.RippleDrawable
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.view.Gravity
import android.view.View
import android.view.WindowInsets
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.*

/** Owns Android navigation and permissions. Device truth stays in the shared Rust session. */
class MainActivity : Activity() {
    private val client get() = (application as CutoutApplication).bluetooth
    private val ui by lazy { CutoutUi(this) }
    private var page = R.string.devices
    private var latest = BlePresentation()
    private var scanned = false
    private var firstPresentation = true
    private lateinit var content: FrameLayout
    private lateinit var ride: RideDashboardView
    private lateinit var picker: DevicePickerPanel
    private lateinit var settings: SettingsPanel
    private lateinit var bottomNavigation: LinearLayout
    private val navigation = linkedMapOf<Int, LinearLayout>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        page = savedInstanceState?.getInt("page") ?: R.string.devices
        val root = ui.column().apply { setBackgroundColor(ui.background) }
        root.setOnApplyWindowInsetsListener { view, insets ->
            val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            val keyboard = insets.getInsets(WindowInsets.Type.ime()).bottom
            view.setPadding(bars.left, bars.top + ui.dp(8), bars.right, maxOf(bars.bottom, keyboard) + ui.dp(8))
            if (::bottomNavigation.isInitialized) bottomNavigation.visibility =
                if (page == R.string.configuration && insets.isVisible(WindowInsets.Type.ime())) View.GONE else View.VISIBLE
            insets
        }
        content = FrameLayout(this)
        root.addView(content, LinearLayout.LayoutParams(-1, 0, 1f))
        ride = RideDashboardView(this, onDisconnect = {
            client.disconnect(); showPage(R.string.devices)
        }, onChooseDevice = { showPage(R.string.devices) })
        picker = DevicePickerPanel(this, onScan = ::scanOrRequestPermission,
            onOpenSettings = { startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName"))) },
            onOpenConfiguration = { showPage(R.string.configuration) }, onConnect = {
                client.connect(it); showPage(R.string.ride)
            })
        settings = SettingsPanel(this) { token, id, value, refused -> client.submit(token, id, value, refused) }
        settings.restoreUiState(savedInstanceState?.getBundle("settings-ui"))
        for ((id, panel) in listOf(R.string.ride to ride, R.string.devices to picker, R.string.configuration to settings)) {
            panel.tag = id
            content.addView(panel, FrameLayout.LayoutParams(-1, -1))
        }
        val tabs = ui.row().apply {
            background = ui.rounded(44); setPadding(ui.dp(5), ui.dp(5), ui.dp(5), ui.dp(5))
        }
        bottomNavigation = tabs
        for ((id, symbol) in listOf(R.string.ride to CutoutSymbol.RIDE, R.string.devices to CutoutSymbol.DEVICES,
            R.string.configuration to CutoutSymbol.SETTINGS)) {
            val item = ui.column().apply {
                gravity = Gravity.CENTER; minimumHeight = ui.dp(58)
                addView(ui.icon(symbol, size = 25))
                addView(ui.space(3))
                addView(ui.text(getString(id), 11f, true).apply {
                    setSingleLine(true); gravity = Gravity.CENTER_HORIZONTAL
                })
                isClickable = true; isFocusable = true; contentDescription = getString(id)
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
                setOnClickListener { showPage(id) }
                accessibilityDelegate = object : View.AccessibilityDelegate() {
                    override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
                        super.onInitializeAccessibilityNodeInfo(host, info)
                        info.className = Button::class.java.name
                        info.isSelected = host.isSelected
                    }
                }
                descendantFocusability = android.view.ViewGroup.FOCUS_BLOCK_DESCENDANTS
                for (index in 0 until childCount) getChildAt(index).importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            }
            navigation[id] = item
            tabs.addView(item, LinearLayout.LayoutParams(0, -2, 1f))
        }
        root.addView(tabs, LinearLayout.LayoutParams(-1, -2).apply {
            setMargins(ui.dp(16), ui.dp(6), ui.dp(16), 0)
        })
        setContentView(root)
        showPage(page)
    }

    override fun onResume() { super.onResume(); client.observe(::render); scanIfNeeded() }
    override fun onStop() {
        client.observe(null); client.stopScan()
        if (page == R.string.devices) scanned = false
        super.onStop()
    }
    override fun onSaveInstanceState(outState: Bundle) {
        outState.putInt("page", page)
        outState.putBundle("settings-ui", settings.saveUiState())
        super.onSaveInstanceState(outState)
    }
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grants: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grants)
        if (requestCode == 1 && client.hasPermissions() && client.isEnabled()) { scanned = true; client.scan() }
        render(latest)
    }

    private fun scanOrRequestPermission() {
        when {
            !client.hasPermissions() -> requestPermissions(arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT), 1)
            !client.isEnabled() -> startActivity(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE))
            latest.scanning -> client.stopScan()
            else -> { scanned = true; client.scan() }
        }
    }

    /** A renderer also consumed by instrumentation with the existing Rust capture replay. */
    internal fun render(value: BlePresentation) {
        latest = value
        val active = value.session?.connection?.token != null
        if (firstPresentation) {
            firstPresentation = false
            if (!active && page == R.string.ride) showPage(R.string.devices)
        }
        navigation[R.string.ride]?.visibility = if (active) View.VISIBLE else View.GONE
        ride.render(value)
        picker.render(value, client.hasPermissions(), client.isEnabled(), scanned)
        settings.render(value.settings)
    }

    private fun showPage(id: Int) {
        if (page == R.string.devices && id != page) { client.stopScan(); scanned = false }
        page = id
        for (index in 0 until content.childCount) content.getChildAt(index).let {
            it.visibility = if (it.tag == id) View.VISIBLE else View.GONE
        }
        navigation.forEach { (itemId, item) ->
            val selected = itemId == id
            item.isSelected = selected
            item.background = RippleDrawable(ColorStateList.valueOf(if (ui.dark) 0x22FFFFFF else 0x11000000),
                ui.rounded(36, if (selected) ui.background else Color.TRANSPARENT, false), null)
            (item.getChildAt(0) as ImageView).setColorFilter(if (selected) ui.brand else ui.foreground)
            (item.getChildAt(2) as TextView).setTextColor(if (selected) ui.brand else ui.foreground)
        }
        scanIfNeeded()
    }

    private fun scanIfNeeded() {
        if (page == R.string.devices && !scanned && client.isEnabled()) {
            scanned = true; client.scan()
        }
    }
}
