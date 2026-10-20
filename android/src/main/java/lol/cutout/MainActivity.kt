package lol.cutout

import android.Manifest
import android.app.Activity
import android.bluetooth.BluetoothAdapter
import android.content.ClipData
import android.content.Intent
import android.content.pm.PackageManager
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
import android.window.OnBackInvokedCallback
import android.window.OnBackInvokedDispatcher
import androidx.core.content.FileProvider
import uniffi.cutout_mobile_ffi.*
import java.io.File

/** Native navigation and OS effects. All pages share the application's Rust owners. */
class MainActivity : Activity() {
    private val app get() = application as CutoutApplication
    private val client get() = app.bluetooth
    private val ui by lazy { CutoutUi(this) }
    private var page = R.string.devices
    private var restoredMapUiState: Bundle? = null
    private var setupReturnPage = R.string.devices
    private var musicReturnPage = R.string.more
    private var soundCloudReturnPage = R.string.music_settings
    private var latest = BlePresentation()
    private var scanned = false
    private var resumed = false
    private var hostStarted = false
    private lateinit var content: FrameLayout
    private lateinit var ride: RideDashboardView
    private lateinit var picker: DevicePickerPanel
    private lateinit var settings: SettingsPanel
    private lateinit var bottomNavigation: LinearLayout
    private lateinit var nestedToolbar: LinearLayout
    private lateinit var nestedTitle: TextView
    private lateinit var compactPlayer: MusicCompactPlayer
    private val panels = linkedMapOf<Int, View>()
    private val navigation = linkedMapOf<Int, LinearLayout>()
    private var map: RideMapPanel? = null
    private var lighting: LightingPanel? = null
    private var pack: PackPanel? = null
    private var soundCloud: AndroidSoundCloudPanel? = null
    private var debug: VescDebugPanel? = null
    private var captures: DiagnosticCapturePanel? = null
    private var setup: SetupPanel? = null
    private var camera: CameraPanel? = null
    private var phoneAlarms: PhoneAlarmPanel? = null
    private val observations = mutableListOf<AutoCloseable>()
    private val preferences by lazy { getSharedPreferences("cutout.presentation", MODE_PRIVATE) }
    private var musicHidden = false
    private var moreIdentity: String? = null
    private var restoreMusicRow: View? = null
    private var restoreMusicDivider: View? = null
    private val backCallback = OnBackInvokedCallback { navigateBack() }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        page = savedInstanceState?.getInt("page") ?: intent.getIntExtra("page", R.string.devices)
        restoredMapUiState = savedInstanceState?.getBundle("map-ui")
        setupReturnPage = savedInstanceState?.getInt("setup-return", R.string.devices) ?: R.string.devices
        musicReturnPage = savedInstanceState?.getInt("music-return", R.string.more) ?: R.string.more
        soundCloudReturnPage = savedInstanceState?.getInt("soundcloud-return", R.string.music_settings) ?: R.string.music_settings
        musicHidden = preferences.getBoolean("music.hidden", false)
        val root = ui.column().apply { setBackgroundColor(ui.background) }
        root.setOnApplyWindowInsetsListener { view, insets ->
            val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            val keyboard = insets.getInsets(WindowInsets.Type.ime()).bottom
            view.setPadding(bars.left, bars.top + ui.dp(8), bars.right, maxOf(bars.bottom, keyboard) + ui.dp(8))
            bottomNavigation.visibility = if (insets.isVisible(WindowInsets.Type.ime())) View.GONE else View.VISIBLE
            insets
        }
        nestedToolbar = ui.row().apply { gravity = Gravity.CENTER_VERTICAL; setPadding(ui.dp(16), 0, ui.dp(16), 0) }
        nestedToolbar.addView(ui.action("‹ ${getString(R.string.back)}") { navigateBack() }.apply {
            background = null; setTextColor(ui.brand); contentDescription = getString(R.string.back)
            minHeight = ui.dp(44); minimumHeight = ui.dp(44); setPadding(ui.dp(8), 0, ui.dp(8), 0)
        }, LinearLayout.LayoutParams(-2, ui.dp(44)))
        nestedTitle = ui.text("", 17f, true).apply { gravity = Gravity.END or Gravity.CENTER_VERTICAL }
        nestedToolbar.addView(nestedTitle, LinearLayout.LayoutParams(0, ui.dp(44), 1f))
        root.addView(nestedToolbar, LinearLayout.LayoutParams(-1, -2))
        content = FrameLayout(this)
        root.addView(content, LinearLayout.LayoutParams(-1, 0, 1f))
        ride = RideDashboardView(this, onDisconnect = { client.disconnect() }, onChooseDevice = { showPage(R.string.devices) })
        observations += app.locations.observePhoneSpeed(ride::renderPhoneSpeed)
        picker = DevicePickerPanel(this, onScan = ::scanOrRequestPermission,
            onOpenSettings = ::openAppSettings,
            onOpenConfiguration = { setupReturnPage = page; showPage(R.string.setup) },
            onConnect = { client.connect(it); showPage(R.string.ride) })
        settings = SettingsPanel(this, client.settingsDraftOwnerId,
            onOpenPhoneAlarms = { showPage(R.string.phone_alarm_title) },
            submit = { token, id, value, refused -> client.submit(token, id, value, refused) },
            submitAction = { token, id, refused -> client.submitAction(token, id, refused) })
        settings.restoreUiState(savedInstanceState?.getBundle("settings-ui"))
        addPanel(R.string.ride, ride)
        addPanel(R.string.devices, picker)
        addPanel(R.string.configuration, settings)
        compactPlayer = MusicCompactPlayer(this, app.music,
            onOpenDetails = { musicReturnPage = page; showPage(R.string.music) })
        root.addView(compactPlayer, LinearLayout.LayoutParams(-1, ui.dp(44)).apply {
            setMargins(ui.dp(16), ui.dp(6), ui.dp(16), 0)
        })
        bottomNavigation = ui.row().apply {
            background = ui.rounded(44); setPadding(ui.dp(5), ui.dp(5), ui.dp(5), ui.dp(5))
        }
        for ((id, symbol) in listOf(R.string.devices to CutoutSymbol.DEVICES,
            R.string.ride to CutoutSymbol.RIDE, R.string.map to CutoutSymbol.MAP,
            R.string.configuration to CutoutSymbol.SETTINGS, R.string.more to CutoutSymbol.MORE)) {
            val item = navigationItem(id, symbol)
            navigation[id] = item
            bottomNavigation.addView(item, LinearLayout.LayoutParams(0, -2, 1f))
        }
        root.addView(bottomNavigation, LinearLayout.LayoutParams(-1, -2).apply {
            setMargins(ui.dp(16), ui.dp(6), ui.dp(16), 0)
        })
        setContentView(root)
        onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, backCallback)
        observations += app.rides.observe { value ->
            app.latestRide = value
            if (resumed) maintainForegroundWork()
        }
        observations += app.music.observe { value ->
            compactPlayer.render(value); updateCompactPlayer(value)
            if (resumed) maintainForegroundWork()
        }
        observations += app.soundCloudMusic.observe { if (resumed) maintainForegroundWork() }
        render(client.latestPresentation)
        showPage(page)
    }

    override fun onStart() { super.onStart(); hostStarted = true; map?.onHostStart() }
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent.hasExtra("page")) showPage(intent.getIntExtra("page", page))
    }
    override fun onResume() {
        super.onResume(); resumed = true
        client.observe(::render)
        app.music.refreshSessions()
        app.refreshPhoneAlarmAuthorization()
        app.locations.setAuthorizationEvidence(if (preferences.getBoolean("location.asked", false))
            MobileRideMapLocationAuthorizationDto.DENIED else MobileRideMapLocationAuthorizationDto.NOT_DETERMINED)
        map?.onHostResume(); map?.refreshLocation(); camera?.setActive(page == R.string.camera); lighting?.setActive(page == R.string.lighting)
        setup?.takeIf { page == R.string.setup }?.reload()
        scanIfNeeded(); maintainForegroundWork()
    }
    override fun onPause() { resumed = false; map?.onHostPause(); super.onPause() }
    override fun onStop() {
        client.observe(null); client.stopScan(); lighting?.setActive(false); camera?.setActive(false)
        if (page == R.string.devices) scanned = false
        hostStarted = false; map?.onHostStop(); super.onStop()
    }
    override fun onDestroy() {
        observations.forEach { it.close() }; observations.clear()
        map?.onHostDestroy()
        if (lighting != null) app.lighting.observe(null)
        panels.values.filterIsInstance<MusicPanel>().forEach { it.dispose() }
        panels.values.filterIsInstance<MusicSettingsPanel>().forEach { it.dispose() }
        phoneAlarms?.dispose()
        captures?.dispose()
        soundCloud?.dispose()
        onBackInvokedDispatcher.unregisterOnBackInvokedCallback(backCallback)
        super.onDestroy()
    }
    override fun onLowMemory() { super.onLowMemory(); map?.onHostLowMemory() }
    override fun onSaveInstanceState(outState: Bundle) {
        outState.putInt("page", page)
        (map?.saveUiState() ?: restoredMapUiState)?.let { outState.putBundle("map-ui", it) }
        outState.putInt("setup-return", setupReturnPage); outState.putInt("music-return", musicReturnPage)
        outState.putInt("soundcloud-return", soundCloudReturnPage)
        outState.putBundle("settings-ui", settings.saveUiState())
        super.onSaveInstanceState(outState)
    }
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grants: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grants)
        if (requestCode == NOTIFICATION_PERMISSION) app.refreshPhoneAlarmAuthorization()
        if (requestCode == BLUETOOTH_PERMISSION && client.hasPermissions() && client.isEnabled()) {
            if (page == R.string.lighting) app.lighting.resume() else { scanned = true; client.scan() }
        }
        if (requestCode == LOCATION_PERMISSION) {
            app.locations.setAuthorizationEvidence(MobileRideMapLocationAuthorizationDto.DENIED)
            map?.refreshLocation()
        }
        render(latest); maintainForegroundWork()
    }

    private fun openAppSettings() = startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$packageName")))
    private fun scanOrRequestPermission() {
        when {
            !client.hasPermissions() -> requestPermissions(arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT), BLUETOOTH_PERMISSION)
            !client.isEnabled() -> startActivity(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE))
            page == R.string.lighting -> app.lighting.resume()
            latest.scanning -> client.stopScan()
            else -> { scanned = true; client.scan() }
        }
    }
    private fun requestLocation() {
        val granted = checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED ||
            checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
        if (granted) { app.locations.refreshEnvironment(); map?.refreshLocation(); maintainForegroundWork(); return }
        if (preferences.getBoolean("location.asked", false) &&
            !shouldShowRequestPermissionRationale(Manifest.permission.ACCESS_FINE_LOCATION) &&
            !shouldShowRequestPermissionRationale(Manifest.permission.ACCESS_COARSE_LOCATION)) { openAppSettings(); return }
        preferences.edit().putBoolean("location.asked", true).apply()
        requestPermissions(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION), LOCATION_PERMISSION)
    }
    private fun requestNotifications() {
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED)
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), NOTIFICATION_PERMISSION)
    }
    private fun maintainForegroundWork() {
        if (!resumed) return
        if (!app.rideServiceActive) RideForegroundService.startForCurrentWork(this)
    }

    /** Also consumed by deterministic instrumentation using the real Rust capture replay. */
    internal fun render(value: BlePresentation) {
        val hadConnection = latest.session?.connection?.token != null
        latest = value
        app.existingLighting?.selectVehicle(value.selectedVehicleIdentifier ?: value.session?.connection?.token?.platformIdentifier)
        val connected = value.session?.connection?.token != null
        if (hadConnection && !connected && (page == R.string.ride || page == R.string.configuration || page == R.string.pack)) showPage(R.string.devices)
        if (!connected && page == R.string.ride && value.phase == NativePhase.IDLE) showPage(R.string.devices)
        navigation[R.string.devices]?.visibility = if (connected) View.GONE else View.VISIBLE
        navigation[R.string.ride]?.visibility = if (connected) View.VISIBLE else View.GONE
        navigation[R.string.configuration]?.visibility = if (connected) View.VISIBLE else View.GONE
        ride.render(value, value.chargeEstimate)
        picker.render(value, client.hasPermissions(), client.isEnabled(), scanned)
        settings.render(value.settings)
        pack?.render(value)
        debug?.render(value)
        if (page == R.string.more && moreIdentity != latest.session?.identity?.toString()) renderMore()
        updateNavigation()
        if (resumed) maintainForegroundWork()
    }

    private fun addPanel(id: Int, panel: View) {
        if (panel.tag == null) panel.tag = id
        panel.visibility = View.GONE; panels[id] = panel
        content.addView(panel, FrameLayout.LayoutParams(-1, -1))
    }
    private fun showPage(requestedId: Int) {
        val id = if (requestedId == R.string.music && app.music.latestPresentation.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD) {
            soundCloudReturnPage = musicReturnPage
            R.string.music_soundcloud_brand
        } else requestedId
        if (page == R.string.devices && id != page) { client.stopScan(); scanned = false }
        if (!panels.containsKey(id)) when (id) {
            R.string.map -> {
                val panel = RideMapPanel(this, app.rides, app.charging, ::requestLocation, ::shareRide, app.music::historyPolicyForNewRide,
                    resolveVehicleName = { identifier ->
                        latest.session?.takeIf { it.connection?.token?.platformIdentifier == identifier }?.identity?.model
                            ?: latest.devices.firstOrNull { it.id == identifier }?.name
                    }, onOpenLocationSettings = ::openAppSettings, mapSurfaceFactory = app::createRideMapSurface)
                map = panel; addPanel(id, panel)
                panel.restoreUiState(restoredMapUiState)
                restoredMapUiState = null
                if (hostStarted) panel.onHostStart()
                if (resumed) panel.onHostResume()
            }
            R.string.more -> addPanel(id, ScrollView(this).apply { addView(ui.column()) })
            R.string.camera -> { val panel = CameraPanel(this, app.camera); camera = panel; addPanel(id, panel) }
            R.string.lighting -> {
                app.lighting.selectVehicle(latest.selectedVehicleIdentifier ?: latest.session?.connection?.token?.platformIdentifier)
                val panel = LightingPanel(this, app.lighting, ::scanOrRequestPermission)
                lighting = panel; addPanel(id, panel)
                observations += app.lighting.addObserver { if (resumed) maintainForegroundWork() }
            }
            R.string.pack -> { val panel = PackPanel(this); pack = panel; panel.render(latest); addPanel(id, panel) }
            R.string.vesc_debug -> { val panel = VescDebugPanel(this); debug = panel; panel.render(latest); addPanel(id, panel) }
            R.string.captures -> {
                val panel = DiagnosticCapturePanel(this, client, ::shareCapture, ::scanOrRequestPermission)
                captures = panel; addPanel(id, panel)
            }
            R.string.phone_alarm_title -> {
                val panel = PhoneAlarmPanel(this, app.phoneAlarms, ::requestNotifications)
                phoneAlarms = panel
                addPanel(id, ScrollView(this).apply {
                    setPadding(ui.dp(24), ui.dp(20), ui.dp(24), ui.dp(28))
                    addView(panel)
                })
            }
            R.string.setup -> {
                val panel = SetupPanel(this, app,
                    onMusic = { musicReturnPage = R.string.setup; showPage(R.string.music_settings) },
                    onCaptures = { showPage(R.string.captures) },
                    onLocation = ::requestLocation, onNotifications = ::requestNotifications,
                    onOpenUrl = { startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(it))) })
                setup = panel; addPanel(id, panel)
            }
            R.string.music -> addPanel(id, MusicPanel(this, app.music,
                onOpenSettings = { showPage(R.string.music_settings) },
                onHidePlayer = { setMusicHidden(true); showPage(musicReturnPage) },
                onDone = { showPage(musicReturnPage) }))
            R.string.music_settings -> addPanel(id, MusicSettingsPanel(this, app.music,
                onRequestListenerAccess = { startActivity(app.music.notificationListenerSettingsIntent()) },
                onChooseSoundCloud = { soundCloudReturnPage = R.string.music_settings; showPage(R.string.music_soundcloud_brand) }))
            R.string.music_soundcloud_brand -> {
                val panel = AndroidSoundCloudPanel(this, app.soundCloudMusic)
                soundCloud = panel
                addPanel(id, ScrollView(this).apply {
                    setPadding(0, ui.dp(12), 0, ui.dp(20))
                    addView(panel, android.widget.FrameLayout.LayoutParams(-1, -2))
                })
            }
            else -> return
        }
        page = id
        panels.forEach { (panelId, panel) -> panel.visibility = if (panelId == id) View.VISIBLE else View.GONE }
        map?.setActive(id == R.string.map)
        lighting?.setActive(resumed && id == R.string.lighting)
        camera?.setActive(resumed && id == R.string.camera)
        if (id == R.string.more) renderMore()
        if (id == R.string.setup) setup?.reload()
        if (id == R.string.captures) captures?.refresh()
        nestedToolbar.visibility = if (id in nestedPages) View.VISIBLE else View.GONE
        nestedTitle.text = getString(id)
        updateCompactPlayer(app.music.latestPresentation)
        updateNavigation(); scanIfNeeded()
    }

    private fun renderMore() {
        val container = (panels[R.string.more] as? ScrollView)?.getChildAt(0) as? LinearLayout ?: return
        moreIdentity = latest.session?.identity?.toString()
        container.removeAllViews(); container.setPadding(ui.dp(20), ui.dp(16), ui.dp(20), ui.dp(24))
        container.addView(ui.title(getString(R.string.more))); container.addView(ui.space(20))
        val accessories = ui.column().apply { background = ui.rounded(18, outlined = false) }
        container.addView(accessories, LinearLayout.LayoutParams(-1, -2))
        fun row(id: Int, symbol: CutoutSymbol) {
            if (accessories.childCount > 0) accessories.addView(ui.divider())
            accessories.addView(ui.navigationRow(getString(id), symbol) { showPage(id) })
        }
        row(R.string.camera, CutoutSymbol.CAMERA)
        row(R.string.lighting, CutoutSymbol.LIGHTING)
        if (latest.session?.identity?.vehicleKind == MobileVehicleKindDto.ELECTRIC_UNICYCLE) row(R.string.pack, CutoutSymbol.PACK)
        if (latest.session?.identity?.protocol == MobileProtocolFamilyDto.VESC) row(R.string.vesc_debug, CutoutSymbol.SETTINGS)
        container.addView(ui.space(24))
        val general = ui.column().apply { background = ui.rounded(18, outlined = false) }
        container.addView(general, LinearLayout.LayoutParams(-1, -2))
        general.addView(ui.navigationRow(getString(R.string.music), CutoutSymbol.MUSIC) {
            setMusicHidden(false); musicReturnPage = R.string.more; showPage(R.string.music)
        }.apply { tag = "more.music" })
        restoreMusicDivider = ui.divider().also { general.addView(it) }
        restoreMusicRow = ui.navigationRow(ui.label("music.restore"), CutoutSymbol.MUSIC) {
            setMusicHidden(false)
        }.apply { tag = "music.restore" }.also { general.addView(it) }
        updateMusicRestoreAction()
        general.addView(ui.divider())
        general.addView(ui.navigationRow(getString(R.string.setup), CutoutSymbol.GEAR) {
            setupReturnPage = R.string.more; showPage(R.string.setup)
        })
        general.addView(ui.divider())
        general.addView(ui.navigationRow(getString(R.string.captures), CutoutSymbol.SIGNAL) { showPage(R.string.captures) })
    }

    private fun navigateBack() {
        if (page == R.string.map && map?.handleBack() == true) return
        if (page == R.string.pack && pack?.handleBack() == true) return
        when (page) {
            R.string.phone_alarm_title -> showPage(R.string.configuration)
            R.string.setup -> showPage(setupReturnPage)
            R.string.music_soundcloud_brand -> showPage(soundCloudReturnPage)
            R.string.music_settings -> showPage(if (musicReturnPage == R.string.setup) R.string.setup else R.string.music)
            R.string.music -> showPage(musicReturnPage)
            R.string.camera, R.string.lighting, R.string.pack, R.string.vesc_debug, R.string.captures -> showPage(R.string.more)
            R.string.more, R.string.configuration, R.string.map -> showPage(if (latest.session?.connection?.token != null) R.string.ride else R.string.devices)
            else -> finish()
        }
    }
    private fun setMusicHidden(hidden: Boolean) {
        musicHidden = hidden; preferences.edit().putBoolean("music.hidden", hidden).apply()
        updateCompactPlayer(app.music.latestPresentation)
        updateMusicRestoreAction()
    }
    private fun updateMusicRestoreAction() {
        val visibility = if (musicHidden) View.VISIBLE else View.GONE
        restoreMusicRow?.visibility = visibility
        restoreMusicDivider?.visibility = visibility
    }
    private fun updateCompactPlayer(value: AndroidMusicPresentation) {
        compactPlayer.visibility = if (!musicHidden && value.showsCompactPlayer() && page != R.string.music && page != R.string.music_settings && page != R.string.music_soundcloud_brand) View.VISIBLE else View.GONE
    }
    private fun updateNavigation() {
        val selectedId = if (page == R.string.phone_alarm_title) R.string.configuration else if (page in nestedPages || page == R.string.more) R.string.more else page
        navigation.forEach { (itemId, item) ->
            val selected = itemId == selectedId; item.isSelected = selected
            item.background = RippleDrawable(ColorStateList.valueOf(if (ui.dark) 0x22FFFFFF else 0x11000000),
                ui.rounded(36, if (selected) ui.background else Color.TRANSPARENT, false), null)
            (item.getChildAt(0) as ImageView).setColorFilter(if (selected) ui.brand else ui.foreground)
            (item.getChildAt(2) as TextView).setTextColor(if (selected) ui.brand else ui.foreground)
        }
    }
    private fun navigationItem(id: Int, symbol: CutoutSymbol) = ui.column().apply {
        gravity = Gravity.CENTER; minimumHeight = ui.dp(58)
        addView(ui.icon(symbol, size = 25)); addView(ui.space(3))
        addView(ui.text(if (id == R.string.configuration) getString(R.string.tune) else getString(id), 11f, true).apply {
            setSingleLine(true); gravity = Gravity.CENTER_HORIZONTAL
        })
        isClickable = true; isFocusable = true; contentDescription = getString(id)
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
        setOnClickListener { showPage(id) }
        accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
                super.onInitializeAccessibilityNodeInfo(host, info); info.className = Button::class.java.name; info.isSelected = host.isSelected
            }
        }
        descendantFocusability = android.view.ViewGroup.FOCUS_BLOCK_DESCENDANTS
        for (index in 0 until childCount) getChildAt(index).importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
    }
    private fun scanIfNeeded() {
        if (resumed && page == R.string.devices && !scanned && client.isEnabled()) { scanned = true; client.scan() }
    }
    private fun shareRide(path: String) {
        try {
            val file = File(path).canonicalFile
            val directory = File(cacheDir, "ride-exports").canonicalFile
            require(file.parentFile == directory && file.isFile)
            val uri = FileProvider.getUriForFile(this, "$packageName.exports", file)
            val send = Intent(Intent.ACTION_SEND).setType("application/json").putExtra(Intent.EXTRA_STREAM, uri)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION).apply { clipData = ClipData.newRawUri(getString(R.string.share_ride), uri) }
            startActivity(Intent.createChooser(send, getString(R.string.share_ride)))
        } catch (_: Exception) { Toast.makeText(this, R.string.export_failed, Toast.LENGTH_LONG).show() }
    }
    private fun shareCapture(candidate: File) {
        try {
            val file = candidate.canonicalFile
            val directory = File(cacheDir, "diagnostic-exports").canonicalFile
            require(file.parentFile == directory && file.isFile)
            val uri = FileProvider.getUriForFile(this, "$packageName.exports", file)
            val send = Intent(Intent.ACTION_SEND).setType("application/octet-stream")
                .putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                .apply { clipData = ClipData.newRawUri(getString(R.string.captures), uri) }
            startActivity(Intent.createChooser(send, getString(R.string.captures)))
        } catch (_: Exception) { Toast.makeText(this, R.string.export_failed, Toast.LENGTH_LONG).show() }
    }
    companion object {
        private const val BLUETOOTH_PERMISSION = 1
        private const val LOCATION_PERMISSION = 2
        private const val NOTIFICATION_PERMISSION = 3
        private val nestedPages = setOf(R.string.phone_alarm_title, R.string.setup, R.string.camera, R.string.lighting, R.string.pack, R.string.music, R.string.music_settings, R.string.music_soundcloud_brand,
            R.string.vesc_debug, R.string.captures)
    }
}
