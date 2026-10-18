package lol.cutout

import android.Manifest
import android.bluetooth.*
import android.bluetooth.le.*
import android.content.Context
import android.content.pm.PackageManager
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import java.util.UUID
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicBoolean
import uniffi.cutout_mobile_ffi.*

data class NearbyPev(val id: String, val name: String, val rssi: Int, val suggested: Boolean,
    val candidate: DiscoveryCandidate? = null, val advertisedName: String? = null)
enum class NativePhase { IDLE, CONNECTING, DISCOVERING, IDENTIFYING }
data class BlePresentation(
    val scanning: Boolean = false,
    val devices: List<NearbyPev> = emptyList(),
    val phase: NativePhase = NativePhase.IDLE,
    val session: MobileDeviceSessionSnapshotDto? = null,
    val telemetry: MobileTelemetrySnapshotDto? = null,
    val settings: MobileSettingsDto? = null,
    val bms: MobileBmsSnapshotDto? = null,
    val problem: Boolean = false,
)

/** Application-owned Android effects. Every Rust call and GATT operation uses one owner. */
class AndroidBleClient(
    private val context: Context,
    private val onSessionObservation: (BlePresentation, CutoutSessionStateHandle) -> Unit = { _, _ -> },
) {
    private val owner = Handler(HandlerThread("CutoutBluetooth").apply { start() }.looper)
    private val main = Handler(Looper.getMainLooper())
    private val manager = context.getSystemService(BluetoothManager::class.java)
    private val stateOwner = lazy {
        (context.applicationContext as CutoutApplication).sessionStateForNativeOwner()
    }
    private val state by stateOwner
    // Saved editor drafts belong to this native owner, not merely a reusable attempt number.
    val settingsDraftOwnerId: String = UUID.randomUUID().toString()
    private val found = linkedMapOf<String, Pair<BluetoothDevice, NearbyPev>>()
    private var connection: Connection? = null
    private var pendingConnectId: String? = null
    private var scanCallback: ScanCallback? = null
    private var stopScanTimer: (() -> Unit)? = null
    private var publishTimer: (() -> Unit)? = null
    private var view = BlePresentation()
    @Volatile private var listener: ((BlePresentation) -> Unit)? = null
    private val additionalObservers = java.util.concurrent.CopyOnWriteArrayList<(BlePresentation) -> Unit>()
    @Volatile var latestPresentation: BlePresentation = view
        private set
    private val pendingCallbacks = AtomicInteger()
    private val timers = NativeTimer { delay, action ->
        val runnable = Runnable(action)
        owner.postDelayed(runnable, delay.coerceAtLeast(1))
        val cancel: () -> Unit = { owner.removeCallbacks(runnable) }
        cancel
    }

    private fun postOnOwner(action: () -> Unit): Boolean = owner.post {
        try { action() } catch (error: Exception) {
            Log.w("CutoutBle", "Bluetooth operation could not complete", error)
            val active = connection
            if (active != null) active.finishTransport(failed = true)
            else view = view.copy(scanning = false, phase = NativePhase.IDLE, session = null,
                telemetry = null, settings = null, bms = null, problem = true)
            stopScanOnOwner()
            publish()
        }
    }

    fun hasPermissions(): Boolean = arrayOf(Manifest.permission.BLUETOOTH_SCAN,
        Manifest.permission.BLUETOOTH_CONNECT).all {
        context.checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED
    }

    fun isEnabled(): Boolean = hasPermissions() && try { manager.adapter?.isEnabled == true }
        catch (_: SecurityException) { false }

    fun observe(callback: ((BlePresentation) -> Unit)?) {
        listener = callback
        postOnOwner {
            if (!hasPermissions()) connection?.fail("Bluetooth permission was revoked")
            publish()
        }
    }

    fun addObserver(callback: (BlePresentation) -> Unit): AutoCloseable {
        additionalObservers += callback
        main.post { if (callback in additionalObservers) callback(latestPresentation) }
        return AutoCloseable { additionalObservers -= callback }
    }

    fun diagnosticRecordingEnabled(completed: (Result<Boolean>) -> Unit) = postOnOwner {
        val result = runCatching { state.automaticDiagnosticRecordingEnabled() }
        main.post { completed(result) }
    }

    fun setDiagnosticRecordingEnabled(enabled: Boolean, completed: (Result<Unit>) -> Unit) = postOnOwner {
        val result = runCatching { state.setAutomaticDiagnosticRecordingEnabled(enabled) }
        main.post { completed(result.map { Unit }) }
    }

    fun scan() = postOnOwner {
        if (!isEnabled()) { view = view.copy(problem = true); publish(); return@postOnOwner }
        stopScanOnOwner()
        found.clear()
        val callback = object : ScanCallback() {
            override fun onScanResult(type: Int, result: ScanResult) = accept(listOf(result))
            override fun onBatchScanResults(results: MutableList<ScanResult>) = accept(results.toList())
            private fun accept(results: List<ScanResult>) {
                // ScanResult payloads are native observations. Do not retain mutable callback buffers.
                val observations = results.take(128).map { result -> Triple(result.device,
                    result.scanRecord?.deviceName, Pair(result.rssi,
                        result.scanRecord?.serviceUuids?.map { DiscoveryServiceUuid(it.uuid.wireBytes()) } ?: emptyList())) }
                postOnOwner {
                    if (scanCallback !== this || !hasPermissions()) return@postOnOwner
                    observations.forEach { (device, name, signal) ->
                        val id = device.address
                        if (found.size >= 128 && id !in found) return@forEach
                        state.observeDiscovery(DiscoveryObservation(id, name?.encodeToByteArray(),
                            signal.second, emptyList(), signal.first.toShort()))
                        found[id] = device to NearbyPev(id, name ?: context.getString(R.string.device_fallback),
                            signal.first, false, advertisedName = name)
                    }
                    // Rust owns candidate admission and ordering; signal changes do not
                    // independently reorder the native rows or promote Bluetooth noise.
                    view = view.copy(devices = state.discoverySnapshot().pickerCandidates.mapNotNull { candidate ->
                        found[candidate.platformIdentifier]?.second?.copy(name = candidate.displayName,
                            suggested = candidate.isPickerCandidate, candidate = candidate)
                    })
                    publish()
                }
            }
            override fun onScanFailed(code: Int) { postOnOwner {
                if (scanCallback === this) {
                    stopScanOnOwner()
                    view = view.copy(problem = true)
                    Log.w("CutoutBle", "scan failed: $code")
                    publish()
                }
            } }
        }
        try {
            val scanner = manager.adapter?.bluetoothLeScanner ?: error("No Bluetooth scanner")
            scanCallback = callback
            scanner.startScan(null, ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
                .setReportDelay(500).build(), callback)
            view = view.copy(scanning = true, devices = emptyList(), problem = false)
            stopScanTimer = timers.after(20_000) { stopScanOnOwner(); publish() }
        } catch (error: Exception) {
            stopScanOnOwner()
            view = view.copy(problem = true)
            Log.w("CutoutBle", "scan unavailable", error)
        }
        publish()
    }

    fun stopScan() = postOnOwner { stopScanOnOwner(); publish() }

    fun connect(id: String) = postOnOwner {
        if (found[id] == null) return@postOnOwner
        pendingConnectId = id
        val existing = connection
        if (existing == null) connectPendingOnOwner() else existing.finishTransport(failed = false)
    }

    private fun connectPendingOnOwner() {
        val id = pendingConnectId ?: return
        pendingConnectId = null
        val selected = found[id] ?: return
        if (!isEnabled()) { view = view.copy(problem = true); publish(); return }
        stopScanOnOwner()
        state.selectDiscoveredPlatform(id)
        val token = checkNotNull(state.beginConnectionAttempt(id, now()).token)
        state.observeConnectionAdvertisement(token, selected.second.advertisedName?.encodeToByteArray())
        view = view.copy(phase = NativePhase.CONNECTING, telemetry = null, settings = null, bms = null,
            session = state.deviceSessionSnapshot(), problem = false)
        val next = Connection(token)
        connection = next
        try {
            next.gatt = selected.first.connectGatt(context, false, next.callback, BluetoothDevice.TRANSPORT_LE)
            if (next.gatt == null) next.fail("Android refused the connection")
            else next.armDeadline()
        } catch (error: Exception) { next.fail("Bluetooth connect failed", error) }
        publish()
    }

    fun disconnect() = postOnOwner {
        pendingConnectId = null
        val existing = connection
        if (existing != null) existing.finishTransport(failed = false)
    }

    fun submit(token: MobileConnectionAttemptTokenDto, id: MobileSettingIdDto,
        value: MobileSettingValueDto, refused: () -> Unit) = postOnOwner {
        val active = connection
        if (active == null || active.token != token) { main.post(refused); return@postOnOwner }
        try { active.session.submit(id, value); active.reschedule() }
        catch (error: Exception) {
            Log.w("CutoutBle", "setting refused", error)
            active.session.publish()
            main.post(refused)
        }
    }

    private fun now(): ULong = SystemClock.elapsedRealtime().toULong()

    private fun stopScanOnOwner() {
        stopScanTimer?.invoke(); stopScanTimer = null
        val callback = scanCallback
        scanCallback = null
        if (callback != null) try { manager.adapter?.bluetoothLeScanner?.stopScan(callback) }
            catch (_: SecurityException) { }
        view = view.copy(scanning = false)
    }

    private fun publish() {
        if (publishTimer != null) return
        publishTimer = timers.after(100) {
            publishTimer = null
            val owned = view
            latestPresentation = owned
            if (stateOwner.isInitialized()) onSessionObservation(owned, state)
            main.post {
                listener?.invoke(owned)
                additionalObservers.forEach { it(owned) }
            }
        }
    }

    private inner class Connection(val token: MobileConnectionAttemptTokenDto) {
        var gatt: BluetoothGatt? = null
        private var retired = false
        private var ending = false
        private var draining = false
        private val callbackOverflow = AtomicBoolean()
        private var cancelWakeup: (() -> Unit)? = null
        private var cancelDeadline: (() -> Unit)? = null
        private val characteristics = mutableListOf<BluetoothGattCharacteristic>()
        private val subscribed = mutableSetOf<UUID>()
        val session = PevSession(state, token, ::now, ::effect,
            onRideTelemetry = { attempt, receivedAtMs, speed ->
                (context.applicationContext as CutoutApplication).rides.observeRideTelemetry(
                    state, attempt, receivedAtMs, speed,
                )
            }) { snapshot, telemetry, settings ->
            if (current()) {
                if (snapshot.connection.deadlineMs == null) {
                    cancelDeadline?.invoke(); cancelDeadline = null
                }
                view = view.copy(session = snapshot, telemetry = telemetry, settings = settings)
                publish()
            }
        }
        private val queue = GattOperationQueue(::current, timers, { fail(it) }, capacity = 128)
        private fun current(): Boolean = !retired && !ending && connection === this && session.isCurrent

        private fun callback(gatt: BluetoothGatt, action: () -> Unit) {
            if (callbackOverflow.get()) return
            if (pendingCallbacks.incrementAndGet() > 256) {
                pendingCallbacks.decrementAndGet()
                if (callbackOverflow.compareAndSet(false, true))
                    postOnOwner { if (current()) fail("Bluetooth callback queue is full") }
                return
            }
            postOnOwner {
                pendingCallbacks.decrementAndGet()
                if (this.gatt !== gatt || !current()) return@postOnOwner
                try { action() } catch (error: Exception) { fail("Bluetooth callback failed", error) }
            }
        }

        val callback = object : BluetoothGattCallback() {
            override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) = callback(gatt) {
                if (status != BluetoothGatt.GATT_SUCCESS || newState == BluetoothProfile.STATE_DISCONNECTED) {
                    fail("Bluetooth link disconnected: $status")
                } else if (newState == BluetoothProfile.STATE_CONNECTED) {
                    state.connectionLinkEstablished(token)
                    view = view.copy(phase = NativePhase.DISCOVERING)
                    queue.enqueue(GattOperationQueue.Operation("mtu", {
                        if (gatt.requestMtu(517)) NativeStart.PENDING else NativeStart.COMPLETE
                    }))
                    queue.enqueue(GattOperationQueue.Operation("services", {
                        if (gatt.discoverServices()) NativeStart.PENDING else NativeStart.REJECTED
                    }))
                    publish()
                }
            }
            override fun onMtuChanged(gatt: BluetoothGatt, mtu: Int, status: Int) = callback(gatt) {
                if (status == BluetoothGatt.GATT_SUCCESS && mtu in 23..517) session.negotiatedMtu(mtu)
                queue.complete("mtu", true) // A rejected negotiation retains ATT's default 20-byte payload.
            }
            override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) = callback(gatt) {
                if (status != BluetoothGatt.GATT_SUCCESS) { queue.complete("services", false); return@callback }
                characteristics.clear()
                characteristics.addAll(gatt.services.flatMap { it.characteristics })
                if (characteristics.size > 64) { fail("Device has too many characteristics"); return@callback }
                val inventory = characteristics.map { characteristic ->
                    val properties = characteristic.properties
                    val roles = mutableListOf<MobileGattRoleDto>()
                    if (properties and BluetoothGattCharacteristic.PROPERTY_READ != 0) roles += MobileGattRoleDto.READ
                    if (properties and BluetoothGattCharacteristic.PROPERTY_WRITE != 0) roles += MobileGattRoleDto.WRITE
                    if (properties and BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE != 0) roles += MobileGattRoleDto.WRITE_WITHOUT_RESPONSE
                    if (properties and BluetoothGattCharacteristic.PROPERTY_NOTIFY != 0) roles += MobileGattRoleDto.NOTIFY
                    if (properties and BluetoothGattCharacteristic.PROPERTY_INDICATE != 0) roles += MobileGattRoleDto.INDICATE
                    MobileGattFingerprintDto(characteristic.service.uuid.wireBytes(), characteristic.uuid.wireBytes(),
                        roles, MobileVerificationStatusDto.HARDWARE_VERIFIED)
                }
                state.observeConnectionGatt(token, inventory)
                view = view.copy(phase = NativePhase.IDENTIFYING)
                val detectionServices = mobileDiscoveryScanServiceUuids().map { wireUuid(it.bytes) }.toSet()
                characteristics.filter { it.service.uuid in detectionServices }
                    .filter { it.properties and (BluetoothGattCharacteristic.PROPERTY_NOTIFY or
                    BluetoothGattCharacteristic.PROPERTY_INDICATE) != 0 }.forEach { subscribe(it.uuid.wireBytes()) }
                characteristics.filter { it.properties and BluetoothGattCharacteristic.PROPERTY_READ != 0 }.forEach { characteristic ->
                    queue.enqueue(GattOperationQueue.Operation(key("read", characteristic), {
                        if (gatt.readCharacteristic(characteristic)) NativeStart.PENDING else NativeStart.COMPLETE
                    }))
                }
                queue.complete("services", true)
                session.identify()
                reschedule()
                publish()
            }
            override fun onDescriptorWrite(gatt: BluetoothGatt, descriptor: BluetoothGattDescriptor, status: Int) = callback(gatt) {
                if (status == BluetoothGatt.GATT_SUCCESS) subscribed += descriptor.characteristic.uuid
                queue.complete(key("subscribe", descriptor.characteristic), status == BluetoothGatt.GATT_SUCCESS)
                probe()
                reschedule()
            }
            override fun onCharacteristicWrite(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic, status: Int) = callback(gatt) {
                queue.complete(key("write", characteristic), status == BluetoothGatt.GATT_SUCCESS)
                reschedule()
            }
            override fun onCharacteristicRead(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic,
                value: ByteArray, status: Int) {
                val owned = value.copyOf()
                callback(gatt) {
                    if (status == BluetoothGatt.GATT_SUCCESS) session.notification(characteristic.uuid.wireBytes(), owned)
                    // Protected/unreadable metadata is an observation gap, not a failed BLE link.
                    queue.complete(key("read", characteristic), true)
                    probe(); reschedule()
                }
            }
            override fun onCharacteristicChanged(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic, value: ByteArray) {
                val owned = value.copyOf()
                callback(gatt) { session.notification(characteristic.uuid.wireBytes(), owned); probe(); reschedule() }
            }
        }

        private fun characteristic(channel: ByteArray): BluetoothGattCharacteristic? =
            characteristics.filter { it.uuid == wireUuid(channel) }.singleOrNull()

        private fun key(kind: String, characteristic: BluetoothGattCharacteristic): String =
            "$kind:${characteristic.service.uuid}:${characteristic.uuid}"

        private fun subscribe(channel: ByteArray) {
            val characteristic = characteristic(channel) ?: run { fail("Missing or ambiguous Bluetooth channel"); return }
            queue.enqueue(GattOperationQueue.Operation(key("subscribe", characteristic), {
                if (characteristic.uuid in subscribed) NativeStart.COMPLETE else {
                    val native = checkNotNull(gatt)
                    val descriptor = characteristic.getDescriptor(UUID.fromString("00002902-0000-1000-8000-00805f9b34fb"))
                    if (descriptor == null || !native.setCharacteristicNotification(characteristic, true)) NativeStart.REJECTED
                    else {
                        val value = if (characteristic.properties and BluetoothGattCharacteristic.PROPERTY_NOTIFY != 0)
                            BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE else BluetoothGattDescriptor.ENABLE_INDICATION_VALUE
                        if (native.writeDescriptor(descriptor, value) == BluetoothStatusCodes.SUCCESS)
                            NativeStart.PENDING else NativeStart.REJECTED
                    }
                }
            }))
        }

        private fun effect(output: MobileSessionOutputDto) {
            if (!current()) return
            output.bmsSnapshot?.let { view = view.copy(bms = it) }
            when (output.kind) {
                MobileSessionOutputKindDto.SUBSCRIBE -> subscribe(output.channel)
                MobileSessionOutputKindDto.WRITE -> write(output.channel, output.bytes, output.operationId)
                MobileSessionOutputKindDto.DISCONNECT -> fail("Device requested disconnect")
                else -> Unit // Semantic outputs are already projected by Rust into the paired snapshot.
            }
        }

        private fun write(channel: ByteArray, bytes: ByteArray, operationId: ULong?, mode: Int? = null) {
            val characteristic = characteristic(channel)
            if (characteristic == null || bytes.isEmpty()) {
                session.writeReceipt(operationId, NativeReceipt.REJECTED)
                fail("Missing or ambiguous Bluetooth write channel"); return
            }
            val chunks = attChunks(bytes, session.writeLimit.toInt())
            val receipt = NativeWriteReceipt(chunks.size) { session.writeReceipt(operationId, it) }
            chunks.forEachIndexed { index, chunk ->
                queue.enqueue(GattOperationQueue.Operation(key("write", characteristic), {
                    if (!session.writeIsCurrent(operationId)) NativeStart.REJECTED else {
                        val type = mode ?: if (characteristic.properties and BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE != 0)
                            BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE else BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
                        val required = if (type == BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE)
                            BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE else BluetoothGattCharacteristic.PROPERTY_WRITE
                        if (characteristic.properties and required == 0) NativeStart.REJECTED
                        else if (checkNotNull(gatt).writeCharacteristic(characteristic, chunk, type) == BluetoothStatusCodes.SUCCESS)
                            NativeStart.PENDING else NativeStart.REJECTED
                    }
                }, { receipt.observe(index, it) }))
            }
        }

        private fun probe() {
            if (!current() || subscribed.isEmpty()) return
            when (val plan = state.beginIdentificationProbeForAttemptAt(token, now())) {
                is MobileIdentificationProbeOutcomeDto.Writes -> plan.writes.forEach {
                    val mode = if (it.mode == MobileIdentificationProbeWriteModeDto.WITHOUT_RESPONSE)
                        BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE else BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
                    write(it.characteristic, it.payload, null, mode)
                }
                else -> Unit
            }
        }

        fun armDeadline() {
            cancelDeadline?.invoke()
            state.connectionAttemptSnapshot().deadlineMs?.let { deadline ->
                cancelDeadline = timers.after((deadline - minOf(deadline, now())).toLong()) {
                    if (current()) {
                        if (view.phase == NativePhase.CONNECTING) fail("Bluetooth connection timed out")
                        else {
                            state.expireConnectionAttempt(token, now())
                            session.identify(true)
                            session.publish()
                        }
                    }
                }
            }
        }

        fun reschedule() {
            cancelWakeup?.invoke(); cancelWakeup = null
            if (!current()) return
            session.nextWakeup(queue.isIdle)?.let { deadline ->
                cancelWakeup = timers.after((deadline - minOf(deadline, now())).toLong()) {
                    if (current()) { session.tick(); reschedule() }
                }
            }
        }

        fun fail(message: String, error: Exception? = null) {
            if (!current()) return
            Log.w("CutoutBle", message, error)
            finishTransport(failed = true)
        }

        /** Preserve admitted callback order until Rust has settled the recording prefix. */
        fun finishTransport(failed: Boolean) {
            if (retired || draining) return
            ending = true
            draining = true
            queue.close()
            cancelWakeup?.invoke(); cancelDeadline?.invoke()
            val endedAt = now()
            (context.applicationContext as CutoutApplication).rides.prepareForTransportEnd(state, token, endedAt) { result ->
                postOnOwner {
                    if (retired || connection !== this) return@postOnOwner
                    draining = false
                    if (result.isFailure) {
                        view = view.copy(problem = true)
                        publish()
                        return@postOnOwner
                    }
                    if (failed) state.connectionTransportFailed(token) else state.disconnectConnectionAttempt()
                    retire(invalidate = false)
                    connection = null
                    view = view.copy(phase = NativePhase.IDLE, session = state.deviceSessionSnapshot(),
                        telemetry = null, settings = null, bms = null, problem = failed || result.isFailure)
                    publish()
                    connectPendingOnOwner()
                }
            }
        }

        fun retire(invalidate: Boolean = true) {
            if (retired) return
            queue.close()
            if (invalidate) state.disconnectConnectionAttempt()
            retired = true
            cancelWakeup?.invoke(); cancelDeadline?.invoke()
            try { gatt?.disconnect() } catch (_: SecurityException) { }
            gatt?.close(); gatt = null
        }
    }
}
