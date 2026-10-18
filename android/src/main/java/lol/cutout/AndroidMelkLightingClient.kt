package lol.cutout

import android.Manifest
import android.bluetooth.*
import android.bluetooth.le.*
import android.content.Context
import android.content.BroadcastReceiver
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.util.Log
import uniffi.cutout_mobile_ffi.*
import java.util.UUID
import java.util.concurrent.CopyOnWriteArrayList

data class LightingPresentation(
    val snapshot: MobileMelkLightingSessionSnapshotDto,
    val candidates: List<MobileMelkLightingSessionCandidateDto>,
    val pairedIdentifier: String?,
    val alias: String?,
    val associatedVehicle: String?,
    val restoreEnabled: Boolean,
    val requested: MobileMelkLightingRestoreStateDto?,
    val confirmed: MobileMelkLightingRestoreStateDto?,
    val presets: List<MobileRgbLightingPresetDto>,
    val error: String? = null,
    val permissionMissing: Boolean = false,
)

/** Native BLE effects for the Rust-owned MELK session reducer. No MELK frames or retry policy live here. */
class AndroidMelkLightingClient(private val context: Context) {
    private val thread = HandlerThread("CutoutMelkLighting").apply { start() }
    private val owner = Handler(thread.looper)
    private val main = Handler(Looper.getMainLooper())
    private val manager = context.getSystemService(BluetoothManager::class.java)
    private val core = MobileMelkLightingSessionCore()
    private val preferences = context.getSharedPreferences("lighting.accessories", Context.MODE_PRIVATE)
    private var store = loadStore()
    private var record: MobileRgbLightingAccessoryRecord? = runCatching { store.selectVehicle(null) }.getOrNull()
    private var selectedVehicle: String? = null
    private val candidates = linkedMapOf<String, Pair<BluetoothDevice, MobileMelkLightingSessionCandidateDto>>()
    private var activeGatt: BluetoothGatt? = null
    private var activeId: String? = null
    private var scanCallback: ScanCallback? = null
    private val timers = mutableMapOf<MobileMelkLightingTimerDto, Runnable>()
    @Volatile private var listener: ((LightingPresentation) -> Unit)? = null
    private val observers = CopyOnWriteArrayList<(LightingPresentation) -> Unit>()
    @Volatile private var presentation = makePresentation()
    private var lastColorPreview = 0L
    private var restoreAttempted = false
    private var pendingScope: String? = null
    private var pendingState: MobileMelkLightingRestoreStateDto? = null

    private val adapterReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action != BluetoothAdapter.ACTION_STATE_CHANGED) return
            val state = intent.getIntExtra(BluetoothAdapter.EXTRA_STATE, BluetoothAdapter.ERROR)
            owner.post {
                val poweredOn = state == BluetoothAdapter.STATE_ON
                if (!poweredOn) { stopScan(); closeGatt() }
                handle(MobileMelkLightingSessionEventDto.BluetoothState(poweredOn, state))
                if (!hasPermissions()) publish(permissionMissing = true)
            }
        }
    }

    init {
        context.applicationContext.registerReceiver(adapterReceiver, IntentFilter(BluetoothAdapter.ACTION_STATE_CHANGED), Context.RECEIVER_NOT_EXPORTED)
    }

    private fun loadStore(): MobileRgbLightingAccessoryStore = preferences.getString("store", null)
        ?.let { encoded -> runCatching { MobileRgbLightingAccessoryStore.decode(android.util.Base64.decode(encoded, android.util.Base64.NO_WRAP)) }.getOrNull() }
        ?: MobileRgbLightingAccessoryStore()

    fun observe(callback: ((LightingPresentation) -> Unit)?) {
        listener = callback
        main.post { callback?.invoke(presentation) }
    }

    fun addObserver(callback: (LightingPresentation) -> Unit): AutoCloseable {
        observers += callback
        main.post { if (callback in observers) callback(presentation) }
        return AutoCloseable { observers.remove(callback) }
    }

    fun currentPresentation(): LightingPresentation = presentation

    fun resume(preferredPlatformIdentifier: String? = null) = owner.post {
        if (!hasPermissions()) {
            publish(error = null, permissionMissing = true)
            return@post
        }
        try {
            core.start(preferredPlatformIdentifier ?: record?.platformIdentifier())
            val adapter = manager.adapter
            core.handle(MobileMelkLightingSessionEventDto.BluetoothState(adapter?.isEnabled == true, adapter?.state ?: BluetoothAdapter.ERROR))
            execute(core.drainActions())
            publish()
        } catch (error: Exception) { fail("Bluetooth lighting session could not start", error) }
    }

    /** First-pairing scans belong to the route; a paired accessory reconnects across navigation. */
    fun stopIfUnpaired() = owner.post {
        if (record?.platformIdentifier().isNullOrBlank()) stopOnOwner()
    }

    fun stop() = owner.post { stopOnOwner() }

    private fun stopOnOwner() {
        core.stop()
        execute(core.drainActions())
        stopScan()
        closeGatt()
        timers.values.forEach(owner::removeCallbacks); timers.clear()
        runCatching { record?.setConnection(MobileRgbLightingConnectionStateDto.DISCONNECTED) }
        persist()
        publish()
    }

    fun selectVehicle(identifier: String?) = owner.post {
        if (selectedVehicle == identifier) return@post
        val state = core.snapshot().state
        val wasRunning = activeGatt != null || scanCallback != null || state == MobileMelkLightingSessionStateDto.Connecting ||
            state == MobileMelkLightingSessionStateDto.Discovering || state == MobileMelkLightingSessionStateDto.Ready ||
            state == MobileMelkLightingSessionStateDto.Scanning || state is MobileMelkLightingSessionStateDto.Retrying
        if (wasRunning) {
            core.stop(); execute(core.drainActions()); closeGatt(); stopScan()
            timers.values.forEach(owner::removeCallbacks); timers.clear()
        }
        selectedVehicle = identifier
        record = runCatching { store.selectVehicle(identifier) }.getOrNull()
        persist()
        restoreAttempted = false
        if (wasRunning || record?.platformIdentifier() != null) {
            core.start(record?.platformIdentifier())
            val adapter = manager.adapter
            core.handle(MobileMelkLightingSessionEventDto.BluetoothState(adapter?.isEnabled == true, adapter?.state ?: BluetoothAdapter.ERROR))
            execute(core.drainActions())
        }
        publish()
    }

    fun selectCandidate(identifier: String) = owner.post { core.selectCandidate(identifier); execute(core.drainActions()); publish() }

    fun setPower(value: Boolean) = owner.post {
        if (core.setPower(value)) { pendingScope = "power"; pendingState = currentRequested().copy(powerOn = value); saveRequested(pendingState!!); core.flushWrites(activeGatt != null); execute(core.drainActions()); publish() }
        else publish(error = "Power command was not accepted")
    }
    fun setBrightness(value: Int) = owner.post {
        val next = currentRequested().copy(brightness = value.coerceIn(0, 100).toUByte())
        if (core.setBrightness(next.brightness)) { pendingScope = "brightness"; pendingState = next; saveRequested(next); core.flushWrites(activeGatt != null); execute(core.drainActions()); publish() }
        else publish(error = "Brightness command was not accepted")
    }
    fun setSolidColor(red: Int, green: Int, blue: Int, preview: Boolean = false) = owner.post {
        if (preview && android.os.SystemClock.elapsedRealtime() - lastColorPreview < 34) return@post
        val requested = currentRequested()
        val state = requested.copy(red = red.coerceIn(0,255).toUByte(), green = green.coerceIn(0,255).toUByte(), blue = blue.coerceIn(0,255).toUByte(), playback = null)
        val wasSolid = requested.playback == null || requested.playback == MobileLightingPlaybackDto.Solid
        val accepted = if (wasSolid) core.setSolidColor(state.red, state.green, state.blue) else core.applyState(state)
        if (!accepted) { publish(error = "Color command was not accepted"); return@post }
        if (!preview) { pendingScope = if (wasSolid) "color" else "state"; pendingState = state; saveRequested(state) }
        if (preview) lastColorPreview = android.os.SystemClock.elapsedRealtime()
        core.flushWrites(activeGatt != null); execute(core.drainActions()); if (!preview) publish()
    }

    fun setEffectSpeed(value: Int) = mutate("Effect speed command was not accepted") {
        val old = currentRequested(); val playback = old.playback as? MobileLightingPlaybackDto.Effect ?: return@mutate false
        val accepted = core.setEffectSpeed(value.coerceIn(0,255).toUByte())
        if (accepted) { pendingScope = "speed"; pendingState = old.copy(playback = MobileLightingPlaybackDto.Effect(playback.pattern, value.coerceIn(0,255).toUByte())); saveRequested(pendingState!!) }
        accepted
    }

    fun applyPlayback(playback: MobileLightingPlaybackDto) = owner.post {
        val old = currentRequested()
        val next = old.copy(playback = playback, powerOn = if (playback == MobileLightingPlaybackDto.Solid) old.powerOn else true)
        if (core.applyState(next)) { pendingScope = "state"; pendingState = next; saveRequested(next); core.flushWrites(activeGatt != null); execute(core.drainActions()); publish() }
        else publish(error = "Playback command was not accepted")
    }

    fun stopMusic() = applyPlayback(MobileLightingPlaybackDto.Solid)
    fun savePreset(name: String) = owner.post {
        val selected = record ?: return@post publish(error = "Pair an accessory before saving scenes")
        try { selected.addPreset(name.trim(), currentRequested()); persist(); publish() }
        catch (error: Exception) { publish(error = error.message ?: "Scene could not be saved") }
    }
    fun replacePreset(name: String) = owner.post {
        val selected = record ?: return@post
        try { selected.replacePreset(name, currentRequested()); persist(); publish() }
        catch (error: Exception) { publish(error = error.message ?: "Scene could not be updated") }
    }
    fun removePreset(name: String) = owner.post { record?.removePreset(name); persist(); publish() }
    fun applyPreset(preset: MobileRgbLightingPresetDto) = owner.post {
        if (core.applyState(preset.requested)) { pendingScope = "state"; pendingState = preset.requested; saveRequested(preset.requested); core.flushWrites(activeGatt != null); execute(core.drainActions()); publish() }
        else publish(error = "Scene command was not accepted")
    }
    fun setRestoreEnabled(enabled: Boolean) = owner.post { record?.setRestoreEnabled(enabled); persist(); publish() }
    fun setAlias(alias: String?) = owner.post {
        try { record?.setAlias(alias?.trim()?.takeIf { it.isNotEmpty() }); persist(); publish() }
        catch (error: Exception) { publish(error = error.message ?: "Accessory name could not be saved") }
    }
    fun setAssociatedVehicle(identifier: String?) = owner.post {
        try { record?.setVehicleIdentifier(identifier); persist(); publish() }
        catch (error: Exception) { publish(error = error.message ?: "Vehicle association could not be saved") }
    }
    fun forgetAccessory() = owner.post {
        stopOnOwner()
        record?.let { store.forget(it.platformIdentifier()) }
        record = null; persist(); publish()
    }

    private fun mutate(failure: String, block: () -> Boolean) = owner.post {
        if (!block()) { publish(error = failure); return@post }
        core.flushWrites(activeGatt != null); execute(core.drainActions()); publish()
    }

    private fun currentRequested() = record?.requestedState() ?: MobileMelkLightingRestoreStateDto(false, 255u, 0u, 0u, 100u, null)
    private fun ensureRecord(identifier: String) {
        if (record?.platformIdentifier() != identifier) {
            record = try { store.pair(identifier, selectedVehicle, mobileMelkLightingCapabilitiesFingerprint()) }
            catch (error: Exception) { Log.w(TAG, "Could not save verified accessory identity", error); null }
            persist()
        }
        val selected = record ?: return
        runCatching { selected.setConnection(MobileRgbLightingConnectionStateDto.READY) }
        if (!restoreAttempted && selected.restoreEnabled() && store.isCompatibleWithCurrentProfile(identifier)) {
            val candidate = runCatching { store.restoreCandidate(identifier) }.getOrNull()
            if (candidate != null && candidate.platformIdentifier == identifier && core.applyState(candidate.requestedState)) {
                pendingScope = "state"; pendingState = candidate.requestedState; restoreAttempted = true
                selected.setRequestedState(candidate.requestedState); selected.setConfirmation(MobileRgbLightingConfirmationStateDto.UNKNOWN)
                core.flushWrites(activeGatt != null)
            }
        }
        persist()
    }
    private fun confirmPending() {
        val selected = record ?: return
        val state = pendingState ?: return
        when (pendingScope) {
            "power" -> selected.confirmPartialState(state, MobileRgbLightingPartialStateDto.POWER)
            "color" -> selected.confirmPartialState(state, MobileRgbLightingPartialStateDto.COLOR)
            "brightness" -> selected.confirmPartialState(state, MobileRgbLightingPartialStateDto.BRIGHTNESS)
            "speed" -> selected.confirmPartialState(state, MobileRgbLightingPartialStateDto.EFFECT_SPEED)
            "state" -> selected.setConfirmedState(state)
            else -> return
        }
        selected.setConfirmation(MobileRgbLightingConfirmationStateDto.CONFIRMED)
        pendingState = null; pendingScope = null; persist()
    }
    private fun markUnconfirmed() {
        val selected = record
        if (pendingState != null && selected != null) {
            selected.setConfirmation(MobileRgbLightingConfirmationStateDto.UNCONFIRMED)
            pendingState = null; pendingScope = null; persist()
        }
    }
    private fun saveRequested(state: MobileMelkLightingRestoreStateDto) {
        try { record?.setRequestedState(state); record?.setConfirmation(MobileRgbLightingConfirmationStateDto.UNKNOWN); persist() }
        catch (error: Exception) { Log.w(TAG, "Unable to persist requested light state", error) }
    }
    private fun persist() {
        runCatching { preferences.edit().putString("store", android.util.Base64.encodeToString(store.encode(), android.util.Base64.NO_WRAP)).apply() }
            .onFailure { Log.w(TAG, "Unable to persist lighting accessory store", it) }
    }

    private fun makePresentation(error: String? = null, permissionMissing: Boolean = !hasPermissions()) = LightingPresentation(
        core.snapshot(), candidates.values.map { it.second }.sortedByDescending { it.rssi }, record?.platformIdentifier(),
        record?.alias(), record?.vehicleIdentifier(), record?.restoreEnabled() ?: false, record?.requestedState(), record?.confirmedState(),
        record?.presets() ?: emptyList(), error, permissionMissing,
    )
    private fun publish(error: String? = null, permissionMissing: Boolean = !hasPermissions()) {
        presentation = makePresentation(error, permissionMissing)
        val value = presentation
        main.post { listener?.invoke(value); observers.forEach { it(value) } }
    }
    private fun hasPermissions() = context.checkSelfPermission(Manifest.permission.BLUETOOTH_SCAN) == PackageManager.PERMISSION_GRANTED &&
        context.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED
    private fun adapterEnabled() = hasPermissions() && runCatching { manager.adapter?.isEnabled == true }.getOrDefault(false)

    private fun handle(event: MobileMelkLightingSessionEventDto) {
        try {
            core.handle(event)
            core.drainCandidates().forEach { candidate ->
                val device = candidates[candidate.platformIdentifier]?.first ?: resolveDevice(candidate.platformIdentifier)
                if (device != null) candidates[candidate.platformIdentifier] = device to candidate
            }
            core.drainCandidateRemovals().forEach(candidates::remove)
            val snapshot = core.snapshot()
            if (snapshot.state == MobileMelkLightingSessionStateDto.Ready) snapshot.platformIdentifier?.let(::ensureRecord)
            if (snapshot.state == MobileMelkLightingSessionStateDto.Disconnected || snapshot.state is MobileMelkLightingSessionStateDto.Failed || snapshot.state is MobileMelkLightingSessionStateDto.Retrying) {
                restoreAttempted = false
                if (snapshot.commandStatus.toInt() == 3) markUnconfirmed()
                record?.setConnection(MobileRgbLightingConnectionStateDto.DISCONNECTED)
            }
            if (event is MobileMelkLightingSessionEventDto.Notification) {
                core.drainNotifications().forEach { bytes -> Log.d(TAG, "FFF4 notification received (${bytes.size} bytes)") }
                when (core.snapshot().commandStatus.toInt()) { 2 -> confirmPending(); 3 -> markUnconfirmed() }
            }
            execute(core.drainActions()); publish()
        } catch (error: Exception) { fail("Rust lighting session rejected an event", error) }
    }
    private fun execute(actions: List<MobileMelkLightingSessionActionDto>) {
        for (action in actions) try {
            when (action) {
                MobileMelkLightingSessionActionDto.RestartTransport -> {
                    stopScan(); closeGatt(); candidates.clear()
                    timers.values.forEach(owner::removeCallbacks); timers.clear()
                }
                MobileMelkLightingSessionActionDto.Scan -> startScan()
                MobileMelkLightingSessionActionDto.StopScan -> stopScan()
                is MobileMelkLightingSessionActionDto.RestorePeripheral -> restore(action.platformIdentifier)
                is MobileMelkLightingSessionActionDto.Connect -> connect(action.platformIdentifier)
                is MobileMelkLightingSessionActionDto.CancelConnect -> if (activeId == action.platformIdentifier) closeGatt()
                is MobileMelkLightingSessionActionDto.DiscoverServices -> if (activeId == action.platformIdentifier && activeGatt?.discoverServices() != true) {
                    handle(MobileMelkLightingSessionEventDto.ServicesDiscovered(emptyList(), "Android refused service discovery"))
                }
                is MobileMelkLightingSessionActionDto.DiscoverCharacteristics -> if (activeId == action.platformIdentifier) {
                    val service = activeGatt?.getService(action.service.asUuid())
                    if (service == null) handle(MobileMelkLightingSessionEventDto.CharacteristicsDiscovered(null, action.service, emptyList(), "Verified service is unavailable"))
                    else {
                        val evidence = service.characteristics.map { characteristic ->
                            MobileMelkLightingCharacteristicEvidenceDto(characteristic.uuid.toDto(),
                                characteristic.properties and BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE != 0,
                                characteristic.properties and (BluetoothGattCharacteristic.PROPERTY_NOTIFY or BluetoothGattCharacteristic.PROPERTY_INDICATE) != 0)
                        }
                        handle(MobileMelkLightingSessionEventDto.CharacteristicsDiscovered(activeGatt?.device?.name, action.service, evidence, null))
                    }
                }
                is MobileMelkLightingSessionActionDto.Subscribe -> subscribe(action.platformIdentifier, action.characteristic)
                is MobileMelkLightingSessionActionDto.Write -> write(action.platformIdentifier, action.write)
                is MobileMelkLightingSessionActionDto.ArmTimer -> arm(action.timer, action.delayMilliseconds.toLong())
            }
        } catch (error: Exception) { fail("Bluetooth lighting operation failed", error) }
    }

    private fun startScan() {
        if (!adapterEnabled()) { handle(MobileMelkLightingSessionEventDto.BluetoothState(false, 0)); return }
        stopScan()
        val callback = object : ScanCallback() {
            override fun onScanResult(type: Int, result: ScanResult) = accept(result)
            override fun onBatchScanResults(results: MutableList<ScanResult>) = results.forEach(::accept)
            private fun accept(result: ScanResult) {
                val dev = result.device; val id = dev.address; val name = result.scanRecord?.deviceName ?: dev.name
                owner.post {
                    if (scanCallback !== this || !hasPermissions()) return@post
                    candidates[id] = dev to MobileMelkLightingSessionCandidateDto(id, name, result.rssi)
                    handle(MobileMelkLightingSessionEventDto.Discovered(name, id, result.rssi))
                }
            }
            override fun onScanFailed(errorCode: Int) { owner.post { if (scanCallback === this) fail("Bluetooth scan failed: $errorCode") } }
        }
        scanCallback = callback
        try { manager.adapter?.bluetoothLeScanner?.startScan(null, ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY).setReportDelay(300).build(), callback) }
        catch (error: Exception) { fail("Bluetooth scan is unavailable", error) }
    }
    private fun stopScan() { val current = scanCallback; scanCallback = null; if (current != null) runCatching { manager.adapter?.bluetoothLeScanner?.stopScan(current) } }
    private fun restore(identifier: String) {
        if (!hasPermissions()) { handle(MobileMelkLightingSessionEventDto.RestoreUnavailable); return }
        val device = resolveDevice(identifier)
        if (device == null) { handle(MobileMelkLightingSessionEventDto.RestoreUnavailable); return }
        candidates[identifier] = device to MobileMelkLightingSessionCandidateDto(identifier, device.name, Int.MIN_VALUE)
        handle(MobileMelkLightingSessionEventDto.Restored(device.name, identifier, false, false))
    }
    private fun connect(identifier: String) {
        if (!hasPermissions()) { handle(MobileMelkLightingSessionEventDto.ConnectFailed("Bluetooth permission is unavailable")); return }
        val device = candidates[identifier]?.first ?: resolveDevice(identifier)
        if (device == null) { handle(MobileMelkLightingSessionEventDto.ConnectFailed("Lighting accessory is unavailable")); return }
        closeGatt(); activeId = identifier
        val callback = callbackFor(identifier)
        activeGatt = device.connectGatt(context, false, callback, BluetoothDevice.TRANSPORT_LE)
        if (activeGatt == null) handle(MobileMelkLightingSessionEventDto.ConnectFailed("Android refused the connection"))
    }

    private fun callbackFor(identifier: String) = object : BluetoothGattCallback() {
        private fun valid(gatt: BluetoothGatt) = activeGatt === gatt && activeId == identifier
        private fun deliver(gatt: BluetoothGatt, event: MobileMelkLightingSessionEventDto) = owner.post { if (valid(gatt)) handle(event) }
        override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
            if (status != BluetoothGatt.GATT_SUCCESS || newState == BluetoothProfile.STATE_DISCONNECTED) {
                owner.post {
                    if (valid(gatt)) {
                        val connecting = core.snapshot().state == MobileMelkLightingSessionStateDto.Connecting
                        activeGatt = null; activeId = null; runCatching { gatt.close() }
                        handle(if (connecting) MobileMelkLightingSessionEventDto.ConnectFailed("Bluetooth link disconnected: $status")
                            else MobileMelkLightingSessionEventDto.Disconnected("Bluetooth link disconnected: $status", adapterEnabled()))
                    }
                }
            } else if (newState == BluetoothProfile.STATE_CONNECTED) deliver(gatt,
                MobileMelkLightingSessionEventDto.Connected(gatt.device.name, identifier))
        }
        override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
            val services = if (status == BluetoothGatt.GATT_SUCCESS) gatt.services.map { it.uuid.toDto() } else emptyList()
            deliver(gatt, MobileMelkLightingSessionEventDto.ServicesDiscovered(services, if (status == BluetoothGatt.GATT_SUCCESS) null else "GATT service discovery failed: $status"))
        }
        override fun onCharacteristicRead(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic, value: ByteArray, status: Int) {
            if (status == BluetoothGatt.GATT_SUCCESS) deliver(gatt, MobileMelkLightingSessionEventDto.Notification(characteristic.uuid.toDto(), value.copyOf()))
        }
        override fun onCharacteristicChanged(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic, value: ByteArray) {
            deliver(gatt, MobileMelkLightingSessionEventDto.Notification(characteristic.uuid.toDto(), value.copyOf()))
        }
        override fun onDescriptorWrite(gatt: BluetoothGatt, descriptor: BluetoothGattDescriptor, status: Int) {
            val c = descriptor.characteristic
            deliver(gatt, MobileMelkLightingSessionEventDto.NotificationState(c.uuid.toDto(), status == BluetoothGatt.GATT_SUCCESS, true, if (status == BluetoothGatt.GATT_SUCCESS) null else "Notification subscription failed: $status"))
        }
        override fun onCharacteristicWrite(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic, status: Int) {
            deliver(gatt, if (status == BluetoothGatt.GATT_SUCCESS) MobileMelkLightingSessionEventDto.WriteReady(true)
                else MobileMelkLightingSessionEventDto.Disconnected("Lighting write failed: $status", adapterEnabled()))
        }
    }

    private fun subscribe(identifier: String, uuid: MobileBluetoothUuid) {
        if (activeId != identifier) return
        val characteristic = activeGatt?.services?.flatMap { it.characteristics }?.singleOrNull { it.uuid == uuid.asUuid() }
        val gatt = activeGatt
        if (characteristic == null || gatt == null) {
            handle(MobileMelkLightingSessionEventDto.NotificationState(uuid, false, false, "Verified notification channel is unavailable")); return
        }
        if (!gatt.setCharacteristicNotification(characteristic, true)) {
            handle(MobileMelkLightingSessionEventDto.NotificationState(uuid, false, false, "Android refused notification subscription")); return
        }
        val descriptor = characteristic.getDescriptor(UUID.fromString("00002902-0000-1000-8000-00805f9b34fb"))
        val enabled = if (characteristic.properties and BluetoothGattCharacteristic.PROPERTY_NOTIFY != 0)
            BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE else BluetoothGattDescriptor.ENABLE_INDICATION_VALUE
        if (descriptor == null || gatt.writeDescriptor(descriptor, enabled) != BluetoothStatusCodes.SUCCESS)
            handle(MobileMelkLightingSessionEventDto.NotificationState(uuid, false, false, "Android refused notification descriptor"))
    }
    private fun write(identifier: String, write: MobileMelkLightingWriteDto) {
        if (activeId != identifier) return
        val characteristic = activeGatt?.services?.flatMap { it.characteristics }?.singleOrNull { it.uuid == write.characteristic.asUuid() }
        val gatt = activeGatt
        if (characteristic == null || gatt == null || write.mode != MobileMelkLightingWriteModeDto.WITHOUT_RESPONSE ||
            characteristic.properties and BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE == 0) {
            fail("Rust-requested lighting write is unavailable or unsupported"); return
        }
        if (gatt.writeCharacteristic(characteristic, write.payload, BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE) != BluetoothStatusCodes.SUCCESS)
            fail("Android refused a Rust-planned lighting write")
    }

    private fun arm(timer: MobileMelkLightingTimerDto, delay: Long) {
        timers.remove(timer)?.let(owner::removeCallbacks)
        val task = Runnable { handle(MobileMelkLightingSessionEventDto.TimerFired(timer, activeGatt != null)) }
        timers[timer] = task
        owner.postDelayed(task, delay.coerceAtLeast(1))
    }
    private fun closeGatt() { val gatt = activeGatt; activeGatt = null; activeId = null; if (gatt != null) { runCatching { gatt.disconnect() }; runCatching { gatt.close() } } }
    private fun resolveDevice(identifier: String): BluetoothDevice? {
        return runCatching { manager.adapter?.getRemoteDevice(identifier) }.getOrNull()
    }
    private fun fail(message: String, error: Exception? = null) {
        Log.w(TAG, message, error)
        val hadConnection = core.snapshot().platformIdentifier != null
        closeGatt(); stopScan()
        if (hadConnection) {
            core.handle(MobileMelkLightingSessionEventDto.Disconnected(message, adapterEnabled()))
            execute(core.drainActions())
        }
        publish(error = error?.message?.takeIf { it.isNotBlank() } ?: message)
    }

    companion object { private const val TAG = "CutoutLighting" }
}

private fun UUID.toDto() = MobileBluetoothUuid(mostSignificantBits.toULong(), leastSignificantBits.toULong())
private fun MobileBluetoothUuid.asUuid() = UUID(mostSignificantBits.toLong(), leastSignificantBits.toLong())
