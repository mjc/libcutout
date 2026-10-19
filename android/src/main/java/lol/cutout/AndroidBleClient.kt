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
import java.security.SecureRandom
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicBoolean
import uniffi.cutout_mobile_ffi.*

data class NearbyPev(val id: String, val name: String, val rssi: Int, val suggested: Boolean,
    val candidate: DiscoveryCandidate? = null, val advertisedName: String? = null,
    val advertisedServices: List<ByteArray> = emptyList())
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
    val selectedVehicleIdentifier: String? = null,
    val notificationCount: ULong = 0UL,
    val chargeEstimate: MobileChargeEstimateStateDto? = null,
    /** Rust-owned logical ride freshness state used to fence safety readings. */
    val rideSession: MobileRideSessionSnapshotDto? = null,
    /** Rust-derived admission route for the active attempt, independent of scan rows. */
    val connectionCandidate: DiscoveryCandidate? = null,
)

/** Android effect boundary for the shared Rust-owned ride lifecycle. */
internal object AndroidRideSessionEffects {
    fun reduceAndSettlePlatformEffects(
        state: CutoutSessionStateHandle,
        input: MobileRideSessionInputDto,
    ): MobileRideSessionSnapshotDto {
        var decision = state.reduceRideSession(input)
        repeat(3) {
            val acknowledgement = when (val effect = decision.effect) {
                is MobileRideSessionEffectDto.StartActivity ->
                    MobileRideSessionInputDto.ActivityUnavailable(effect.identity)
                is MobileRideSessionEffectDto.EndActivity ->
                    MobileRideSessionInputDto.ActivityEnded(effect.identity)
                else -> return decision.snapshot
            }
            decision = state.reduceRideSession(acknowledgement)
        }
        return decision.snapshot
    }

    fun freshnessChecked(state: CutoutSessionStateHandle, nowMs: ULong): MobileRideSessionSnapshotDto =
        state.reduceRideSession(MobileRideSessionInputDto.FreshnessChecked(nowMs)).snapshot

    fun acceptsFreshnessUpdates(snapshot: MobileRideSessionSnapshotDto): Boolean =
        snapshot.phase == MobileRideSessionPhaseDto.Starting ||
            snapshot.phase == MobileRideSessionPhaseDto.Active ||
            snapshot.phase == MobileRideSessionPhaseDto.Reconnecting

    fun reconnectExhausted(state: CutoutSessionStateHandle): MobileRideSessionSnapshotDto =
        reduceAndSettlePlatformEffects(state, MobileRideSessionInputDto.ReconnectExhausted)
}

/** Application-owned Android effects. Every Rust call and GATT operation uses one owner. */
class AndroidBleClient(
    private val context: Context,
    private val onSessionObservation: (BlePresentation, CutoutSessionStateHandle) -> Unit = { _, _ -> },
) {
    private val owner = Handler(HandlerThread("CutoutBluetooth").apply { start() }.looper)
    private val chargeEstimates by lazy(LazyThreadSafetyMode.NONE) { ChargeEstimateRuntime() }
    private val main = Handler(Looper.getMainLooper())
    private val manager = context.getSystemService(BluetoothManager::class.java)
    private val stateOwner = lazy {
        (context.applicationContext as CutoutApplication).sessionStateForNativeOwner()
    }
    private val state by stateOwner
    val diagnostics by lazy {
        AndroidDiagnosticCaptureClient(
            context = context,
            state = { state },
            database = { (context.applicationContext as CutoutApplication).rides.databaseForNativeOwner() },
            postOwner = ::postOnOwner,
            onFinishManual = { connection?.finishTransport(failed = false) },
        )
    }
    // Saved editor drafts belong to this native owner, not merely a reusable attempt number.
    val settingsDraftOwnerId: String = UUID.randomUUID().toString()
    private val found = linkedMapOf<String, Pair<BluetoothDevice, NearbyPev>>()
    private var connection: Connection? = null
    private var pendingConnectId: String? = null
    private var pendingRetry: MobileConnectionRetryDto? = null
    private var pendingRetryDevice: BluetoothDevice? = null
    private var cancelRetryTimer: (() -> Unit)? = null
    private var restoreAttemptedId: String? = null
    private var restoreSuppressed = false
    private var selectedDeviceId: String? = null
    private var selectionLoaded = false
    private var selectionLoadInFlight = false
    private var selectionRevision = 0L
    private var forgettingSavedDevice = false
    private var scanCallback: ScanCallback? = null
    private var stopScanTimer: (() -> Unit)? = null
    // Callback counters are per-scan and atomic; forwarding/candidate counts belong to the owner.
    private var scanCallbackCount = java.util.concurrent.atomic.AtomicInteger()
    private var scanResultCount = java.util.concurrent.atomic.AtomicInteger()
    private var scanForwardedCount = 0
    private var scanCandidateCount = 0
    private var publishTimer: (() -> Unit)? = null
    private var rideFreshnessTimer: (() -> Unit)? = null
    private var rideFreshnessScheduledAt: ULong? = null
    private var rideReconnectPlatformIdentifier: String? = null
    private var rideStartedConnectionToken: MobileConnectionAttemptTokenDto? = null
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
                telemetry = null, settings = null, bms = null, problem = true, connectionCandidate = null)
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
            else if (callback != null && pendingRetry != null && isEnabled()) {
                pendingRetryDevice?.let { armRetryTimer(it, checkNotNull(pendingRetry)) }
            } else if (callback != null) {
                loadSavedSelectionOnOwner()
                tryRestoreSelectedDeviceOnOwner()
            }
            publish()
        }
    }

    fun addObserver(callback: (BlePresentation) -> Unit): AutoCloseable {
        additionalObservers += callback
        main.post { if (callback in additionalObservers) callback(latestPresentation) }
        return AutoCloseable { additionalObservers -= callback }
    }

    /** Reads the exact user-selected platform identifier from this BLE owner's Rust database. */
    fun savedDevice(completed: (Result<String?>) -> Unit): Boolean = postOnOwner {
        if (selectionLoaded) {
            val saved = selectedDeviceId
            main.post { completed(Result.success(saved)) }
            return@postOnOwner
        }
        val revision = selectionRevision
        (context.applicationContext as CutoutApplication).rides.withDatabase(
            work = { database -> database.selectedDevice() },
        ) { result ->
            owner.post {
                if (revision == selectionRevision) {
                    result.onSuccess {
                        selectedDeviceId = it
                        selectionLoaded = true
                    }.onFailure { error ->
                        Log.w("CutoutBle", "Saved Bluetooth selection could not be loaded", error)
                    }
                }
                if (revision == selectionRevision) {
                    main.post { completed(result) }
                } else {
                    // A selection mutation won the race; read back that newer authoritative state.
                    savedDevice(completed)
                }
            }
        }
    }

    /**
     * Forgets the saved device only after the existing transport owner has drained admitted
     * telemetry and finalized any diagnostic capture. On failure the saved ID remains available
     * for an explicit retry, while automatic restore and retry stay suppressed.
     */
    fun forgetSavedDevice(completed: (Result<Unit>) -> Unit) = postOnOwner {
        if (forgettingSavedDevice) {
            main.post { completed(Result.failure(IllegalStateException("Forget is already in progress"))) }
            return@postOnOwner
        }
        var completedOnce = false
        fun complete(result: Result<Unit>) {
            if (completedOnce) return
            completedOnce = true
            main.post { completed(result) }
        }
        val revision = ++selectionRevision
        forgettingSavedDevice = true
        restoreSuppressed = true
        pendingConnectId = null
        cancelPendingRetryOnOwner()
        stopScanOnOwner()

        endRideSessionOnOwner()

        fun clearSelectionAfterTeardown(disconnectAttempt: Boolean) {
            if (revision != selectionRevision) {
                complete(Result.failure(IllegalStateException("Saved device changed during forget")))
                return
            }
            val result = if (disconnectAttempt) runCatching { state.disconnectConnectionAttempt() }
                else Result.success(Unit)
            result.onFailure { error ->
                forgettingSavedDevice = false
                selectionRevision++
                complete(Result.failure(error))
                return
            }
            (context.applicationContext as CutoutApplication).rides.withDatabase(
                work = { database -> database.clearSelectedDevice() },
            ) { cleared ->
                owner.post {
                    if (revision != selectionRevision) {
                        complete(Result.failure(IllegalStateException("Saved device changed during forget")))
                        return@post
                    }
                    forgettingSavedDevice = false
                    cleared.onSuccess {
                        selectedDeviceId = null
                        selectionLoaded = true
                        restoreAttemptedId = null
                        view = view.copy(phase = NativePhase.IDLE, session = state.deviceSessionSnapshot(),
                            telemetry = null, settings = null, bms = null, connectionCandidate = null)
                    }.onFailure { error ->
                        Log.w("CutoutBle", "Saved Bluetooth selection could not be cleared", error)
                        view = view.copy(problem = true)
                    }
                    publish()
                    complete(cleared.map { Unit })
                }
            }
        }

        val active = connection
        if (active == null) {
            clearSelectionAfterTeardown(disconnectAttempt = true)
        } else {
            val drained = { clearSelectionAfterTeardown(disconnectAttempt = false) }
            active.finishTransport(
                failed = false,
                afterDrain = drained,
                afterDrainFailure = { error ->
                    if (revision == selectionRevision) {
                        active.removeAfterDrainCallback(drained)
                        forgettingSavedDevice = false
                        selectionRevision++ // Retire this completion so a later teardown cannot clear implicitly.
                        view = view.copy(problem = true)
                        publish()
                        complete(Result.failure(error))
                    }
                },
            )
        }
    }

    fun diagnosticRecordingEnabled(completed: (Result<Boolean>) -> Unit) = postOnOwner {
        val result = runCatching { state.automaticDiagnosticRecordingEnabled() }
        main.post { completed(result) }
    }

    fun setDiagnosticRecordingEnabled(enabled: Boolean, completed: (Result<Unit>) -> Unit) = postOnOwner {
        val result = runCatching { state.setAutomaticDiagnosticRecordingEnabled(enabled) }
        result.getOrNull()?.let(diagnostics::finishWithdrawn)
        main.post { completed(result.map { Unit }) }
    }

    fun scan() = postOnOwner {
        if (!hasPermissions()) {
            Log.i("CutoutBle", "scan request blocked: Bluetooth permission unavailable")
            view = view.copy(problem = true); publish(); return@postOnOwner
        }
        if (!isEnabled()) {
            Log.i("CutoutBle", "scan request blocked: Bluetooth adapter unavailable")
            view = view.copy(problem = true); publish(); return@postOnOwner
        }
        if (tryRestoreSelectedDeviceOnOwner()) {
            Log.i("CutoutBle", "scan bypassed: restoring saved device")
            publish(); return@postOnOwner
        }
        stopScanOnOwner()
        val callbackCount = java.util.concurrent.atomic.AtomicInteger()
        val resultCount = java.util.concurrent.atomic.AtomicInteger()
        scanCallbackCount = callbackCount
        scanResultCount = resultCount
        scanForwardedCount = 0
        scanCandidateCount = 0
        found.clear()
        val callback = object : ScanCallback() {
            override fun onScanResult(type: Int, result: ScanResult) = accept(listOf(result))
            override fun onBatchScanResults(results: MutableList<ScanResult>) = accept(results.toList())
            private fun accept(results: List<ScanResult>) {
                // Count delivery before queueing; a later stop may reject owner work.
                callbackCount.incrementAndGet()
                resultCount.addAndGet(results.size)
                // ScanResult payloads are native observations. Do not retain mutable callback buffers.
                val observations = results.take(128).map(::androidBleScanObservation)
                postOnOwner {
                    if (scanCallback !== this) return@postOnOwner
                    if (!hasPermissions()) return@postOnOwner
                    var forwardedNow = 0
                    observations.forEach { observation ->
                        val id = observation.platformIdentifier
                        if (found.size >= 128 && id !in found) return@forEach
                        state.observeDiscovery(observation.toRustObservation())
                        found[id] = observation.device to observation.toNearbyPev(
                            context.getString(R.string.device_fallback),
                        )
                        forwardedNow++
                    }
                    scanForwardedCount += forwardedNow
                    // Rust owns candidate admission and ordering; signal changes do not
                    // independently reorder the native rows or promote Bluetooth noise.
                    val candidates = state.discoverySnapshot().pickerCandidates.mapNotNull { candidate ->
                        found[candidate.platformIdentifier]?.second?.copy(name = candidate.displayName,
                            suggested = candidate.isPickerCandidate, candidate = candidate)
                    }
                    scanCandidateCount = candidates.size
                    view = view.copy(devices = candidates)
                    publish()
                }
            }
            override fun onScanFailed(code: Int) { postOnOwner {
                if (scanCallback === this) {
                    stopScanOnOwner(outcome = "failed code=$code")
                    view = view.copy(problem = true)
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
            Log.i("CutoutBle", "scan started")
            stopScanTimer = timers.after(20_000) { stopScanOnOwner(outcome = "timed out"); publish() }
        } catch (error: Exception) {
            stopScanOnOwner(outcome = "start failed (${error.javaClass.simpleName})")
            view = view.copy(problem = true)
            Log.w("CutoutBle", "scan unavailable (${error.javaClass.simpleName})")
        }
        publish()
    }

    fun stopScan() = postOnOwner { stopScanOnOwner(); publish() }

    fun connect(id: String) = postOnOwner {
        if (found[id] == null) return@postOnOwner
        forgettingSavedDevice = false
        cancelPendingRetryOnOwner()
        restoreSuppressed = false
        selectedDeviceId = id
        selectionRevision += 1
        val revision = selectionRevision
        (context.applicationContext as CutoutApplication).rides.withDatabase({ database ->
            database.saveSelectedDevice(id, now())
        }) { result ->
            owner.post {
                if (revision == selectionRevision && result.isFailure) {
                    Log.w("CutoutBle", "Saved Bluetooth selection could not be persisted", result.exceptionOrNull())
                    view = view.copy(problem = true)
                    publish()
                }
            }
        }
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
        startConnection(selected.first, token)
    }

    private fun startConnection(device: BluetoothDevice, token: MobileConnectionAttemptTokenDto, recordOnly: Boolean = false) {
        view = view.copy(phase = NativePhase.CONNECTING, telemetry = null, settings = null, bms = null,
            session = state.deviceSessionSnapshot(), problem = false, notificationCount = 0UL,
            connectionCandidate = null)
        if (!recordOnly) {
            val advertisement = found[device.address]?.second
            diagnostics.start(
                MobileCaptureOriginDto.AUTOMATIC, device.address,
                advertisement?.advertisedServices ?: emptyList(),
                advertisement?.advertisedName ?: runCatching { device.name }.getOrNull(),
                null, token,
            )
        }
        val next = Connection(token, device, recordOnly)
        connection = next
        try {
            next.gatt = device.connectGatt(context, false, next.callback, BluetoothDevice.TRANSPORT_LE)
            if (next.gatt == null) next.fail("Android refused the connection")
            else next.armDeadline()
        } catch (error: Exception) { next.fail("Bluetooth connect failed", error) }
        publish()
    }

    fun disconnect() = postOnOwner {
        pendingConnectId = null
        restoreSuppressed = true
        cancelPendingRetryOnOwner()
        endRideSessionOnOwner()
        val existing = connection
        if (existing != null) existing.finishTransport(failed = false)
        else {
            state.disconnectConnectionAttempt()
            view = view.copy(phase = NativePhase.IDLE, session = state.deviceSessionSnapshot(),
                telemetry = null, settings = null, bms = null, connectionCandidate = null)
            publish()
        }
    }

    private fun endRideSessionOnOwner() {
        rideReconnectPlatformIdentifier = null
        rideStartedConnectionToken = null
        AndroidRideSessionEffects.reduceAndSettlePlatformEffects(state, MobileRideSessionInputDto.UserDisconnected)
    }

    /** Starts the Swift-style diagnostic-only route; it never promotes this link to protocol control. */
    fun startManualCapture(
        platformId: String,
        note: String?,
        description: String?,
        completed: (Boolean) -> Unit,
    ) = postOnOwner {
        val selected = found[platformId]
        if (forgettingSavedDevice || selected == null || connection != null || !state.captureLifecycleSnapshot().canStart) {
            main.post { completed(false) }
            return@postOnOwner
        }
        cancelPendingRetryOnOwner()
        restoreSuppressed = true
        pendingConnectId = null
        stopScanOnOwner()
        state.setDeviceConnectionIntent(DeviceConnectionIntentDto.RECORD_ONLY)
        state.selectDiscoveredPlatform(platformId)
        val token = state.beginConnectionAttempt(platformId, now()).token
        if (token == null) {
            main.post { completed(false) }
            return@postOnOwner
        }
        state.observeConnectionAdvertisement(token, selected.second.advertisedName?.encodeToByteArray())
        val annotation = listOfNotNull(note?.takeIf(String::isNotBlank), description?.takeIf(String::isNotBlank))
            .joinToString(" — ").ifBlank { null }
        val generation = diagnostics.start(
            origin = MobileCaptureOriginDto.MANUAL,
            platformId = platformId,
            advertisedServices = selected.second.advertisedServices,
            advertisedName = selected.second.advertisedName,
            description = annotation,
            connectionToken = token,
        )
        if (generation == null) {
            state.disconnectConnectionAttempt()
            main.post { completed(false) }
            return@postOnOwner
        }
        selectedDeviceId = platformId
        restoreSuppressed = true
        startConnection(selected.first, token, recordOnly = true)
        main.post { completed(true) }
    }

    fun finishManualCapture(completed: (Boolean) -> Unit) = diagnostics.beginManualFinish(completed)

    /** Drain the current transport and finalize any capture before closing the capture worker. */
    fun close(completed: () -> Unit = {}) {
        postOnOwner {
            restoreSuppressed = true
            pendingConnectId = null
            cancelPendingRetryOnOwner()
            stopScanOnOwner()
            rideFreshnessTimer?.invoke(); rideFreshnessTimer = null; rideFreshnessScheduledAt = null
            val active = connection
            if (active == null) {
                publishTimer?.invoke(); publishTimer = null
                chargeEstimates.close()
                diagnostics.close(completed)
            } else {
                active.finishTransport(failed = false) {
                    publishTimer?.invoke(); publishTimer = null
                    chargeEstimates.close()
                    diagnostics.close(completed)
                }
            }
        }
    }

    /** Restore only the exact platform identifier explicitly selected by the user. The new
     * transport still passes through fresh service discovery, GATT validation, and Rust detection. */
    private fun tryRestoreSelectedDeviceOnOwner(): Boolean {
        val id = selectedDeviceId ?: return false
        if (restoreSuppressed || restoreAttemptedId == id || connection != null || pendingRetry != null || !isEnabled()) return false
        val adapter = manager.adapter ?: return false
        if (!BluetoothAdapter.checkBluetoothAddress(id)) {
            restoreAttemptedId = id
            view = view.copy(problem = true)
            return false
        }
        return try {
            val device = adapter.getRemoteDevice(id)
            restoreAttemptedId = id
            stopScanOnOwner()
            state.selectDiscoveredPlatform(id)
            val token = checkNotNull(state.beginConnectionAttempt(id, now()).token)
            startConnection(device, token)
            true
        } catch (error: Exception) {
            restoreAttemptedId = id
            view = view.copy(problem = true)
            Log.w("CutoutBle", "Saved Bluetooth device could not be restored", error)
            false
        }
    }

    /** Load the exact persisted platform identifier from the shared Rust database owner. */
    private fun loadSavedSelectionOnOwner() {
        if (selectionLoaded || selectionLoadInFlight) return
        selectionLoadInFlight = true
        val revision = selectionRevision
        (context.applicationContext as CutoutApplication).rides.withDatabase(
            work = { database -> database.selectedDevice() },
        ) { result ->
            owner.post {
                selectionLoadInFlight = false
                if (revision != selectionRevision) return@post
                selectionLoaded = result.isSuccess
                result.fold(
                    onSuccess = { selectedDeviceId = it },
                    onFailure = {
                        Log.w("CutoutBle", "Saved Bluetooth selection could not be loaded", it)
                        view = view.copy(problem = true)
                    },
                )
                tryRestoreSelectedDeviceOnOwner()
                publish()
            }
        }
    }

    private fun scheduleRustRetry(device: BluetoothDevice, token: MobileConnectionAttemptTokenDto) {
        val decision = state.requestConnectionRetry(token, now(), secureRandom.nextInt(1001).toUShort())
        when (decision) {
            is MobileConnectionRetryDecisionDto.Scheduled -> {
                pendingRetry = decision.retry
                pendingRetryDevice = device
                armRetryTimer(device, decision.retry)
            }
            is MobileConnectionRetryDecisionDto.Exhausted -> {
                pendingRetry = null
                pendingRetryDevice = null
                cancelRetryTimer?.invoke(); cancelRetryTimer = null
                rideReconnectPlatformIdentifier = null
                view = view.copy(rideSession = AndroidRideSessionEffects.reconnectExhausted(state))
                view = view.copy(phase = NativePhase.IDLE, session = state.deviceSessionSnapshot(), problem = true,
                    connectionCandidate = null)
            }
            MobileConnectionRetryDecisionDto.Rejected -> {
                pendingRetry = null
                pendingRetryDevice = null
                cancelRetryTimer?.invoke(); cancelRetryTimer = null
                view = view.copy(phase = NativePhase.IDLE, session = state.deviceSessionSnapshot(), problem = true,
                    connectionCandidate = null)
            }
        }
    }

    private fun armRetryTimer(device: BluetoothDevice, retry: MobileConnectionRetryDto) {
        cancelRetryTimer?.invoke()
        val remaining = retry.deadlineMs - minOf(retry.deadlineMs, now())
        cancelRetryTimer = timers.after(remaining.toLong()) {
            if (pendingRetry?.token != retry.token || !hasPermissions() || !isEnabled() || restoreSuppressed) return@after
            val admitted = state.admitConnectionRetry(retry.token, now())
            if (admitted == null) {
                if (state.connectionRetryDeadline(retry.token) != null) armRetryTimer(device, retry)
                return@after
            }
            pendingRetry = null
            pendingRetryDevice = null
            cancelRetryTimer = null
            val nextToken = admitted.token ?: return@after
            stopScanOnOwner()
            startConnection(device, nextToken)
        }
    }

    private fun cancelPendingRetryOnOwner() {
        cancelRetryTimer?.invoke(); cancelRetryTimer = null
        pendingRetry?.let { state.cancelConnectionRetry(it.token) }
        pendingRetry = null
        pendingRetryDevice = null
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

    /** Invoke the next Rust-selected action step for this exact connection token. */
    fun submitAction(token: MobileConnectionAttemptTokenDto, id: MobileDeviceActionIdDto,
        refused: () -> Unit) = postOnOwner {
        val active = connection
        if (active == null || active.token != token) { main.post(refused); return@postOnOwner }
        try { active.session.submitAction(id); active.reschedule() }
        catch (error: Exception) {
            Log.w("CutoutBle", "device action refused", error)
            active.session.publish()
            main.post(refused)
        }
    }

    private fun now(): ULong = SystemClock.elapsedRealtime().toULong()

    private fun stopScanOnOwner(outcome: String = "stopped") {
        stopScanTimer?.invoke(); stopScanTimer = null
        val callback = scanCallback
        scanCallback = null
        if (callback != null) try { manager.adapter?.bluetoothLeScanner?.stopScan(callback) }
            catch (_: SecurityException) { }
        if (callback != null) Log.i(
            "CutoutBle",
            "scan $outcome: callbacks=${scanCallbackCount.get()} results=${scanResultCount.get()} forwarded=$scanForwardedCount rust_candidates=$scanCandidateCount",
        )
        view = view.copy(scanning = false)
    }

    private fun publish() {
        if (publishTimer != null) return
        publishTimer = timers.after(100) {
            publishTimer = null
            val rideSession = updateRideSessionOnOwner()
            val owned = view.copy(
                selectedVehicleIdentifier = if (stateOwner.isInitialized())
                    state.discoverySnapshot().selectedPlatformIdentifier else null,
                chargeEstimate = chargeEstimates.observe(view.session, view.settings, view.telemetry),
                rideSession = rideSession,
            )
            latestPresentation = owned
            if (stateOwner.isInitialized()) onSessionObservation(owned, state)
            main.post {
                listener?.invoke(owned)
                additionalObservers.forEach { it(owned) }
            }
        }
    }

    /** Feed only timestamped, verified transport observations into Rust's ride lifecycle. */
    private fun updateRideSessionOnOwner(): MobileRideSessionSnapshotDto {
        val snapshot = state.rideSessionSnapshot()
        val connection = view.session?.connection
        val token = connection?.token
        if (token != null && connection.readiness == MobileConnectionReadinessDto.VERIFIED) {
            if (snapshot.identity?.platformIdentifier != token.platformIdentifier) {
                startRideSessionOnOwner(token.platformIdentifier)
                rideStartedConnectionToken = token
                rideReconnectPlatformIdentifier = null
            } else when (snapshot.phase) {
                MobileRideSessionPhaseDto.Idle -> {
                    if (rideStartedConnectionToken != token) startRideSessionOnOwner(token.platformIdentifier)
                    rideStartedConnectionToken = token
                }
                is MobileRideSessionPhaseDto.Ended -> {
                    // A new connection attempt may start a fresh logical ride after the
                    // prior one ended. Repeated publications from this same attempt must
                    // not silently reopen a rider-stopped session.
                    if (rideStartedConnectionToken != token) {
                        startRideSessionOnOwner(token.platformIdentifier)
                        rideStartedConnectionToken = token
                    }
                }
                is MobileRideSessionPhaseDto.Ending -> Unit
                else -> rideStartedConnectionToken = token
            }
            var current = state.rideSessionSnapshot()
            if (rideReconnectPlatformIdentifier == token.platformIdentifier &&
                current.phase == MobileRideSessionPhaseDto.Reconnecting
            ) {
                state.reduceRideSession(MobileRideSessionInputDto.BluetoothConnected)
                rideReconnectPlatformIdentifier = null
                current = state.rideSessionSnapshot()
            }
            val observedAt: ULong? = view.telemetry?.atMs?.milliseconds
            if (observedAt != null && current.lastTelemetryAtMs != observedAt) {
                state.reduceRideSession(MobileRideSessionInputDto.TelemetryObserved(observedAt))
            }
        }
        return state.rideSessionSnapshot().also(::scheduleRideFreshnessCheck)
    }

    private fun startRideSessionOnOwner(platformIdentifier: String) {
        AndroidRideSessionEffects.reduceAndSettlePlatformEffects(
            state,
            MobileRideSessionInputDto.Start(platformIdentifier),
        )
    }

    /** One cancellable check at the deadline returned by Rust; Rust decides whether it is stale. */
    private fun scheduleRideFreshnessCheck(snapshot: MobileRideSessionSnapshotDto) {
        val lastTelemetryAt = snapshot.lastTelemetryAtMs
        if (lastTelemetryAt == null || !AndroidRideSessionEffects.acceptsFreshnessUpdates(snapshot)) {
            rideFreshnessTimer?.invoke()
            rideFreshnessTimer = null
            rideFreshnessScheduledAt = null
            return
        }
        if (rideFreshnessScheduledAt == lastTelemetryAt && rideFreshnessTimer != null) return
        rideFreshnessTimer?.invoke()
        val nowMs = now()
        val deadline = if (ULong.MAX_VALUE - lastTelemetryAt < snapshot.staleAfterMs) ULong.MAX_VALUE
            else lastTelemetryAt + snapshot.staleAfterMs
        val delay = (if (deadline <= nowMs) 1UL else deadline - nowMs)
            .coerceAtMost(Long.MAX_VALUE.toULong()).toLong()
        rideFreshnessScheduledAt = lastTelemetryAt
        rideFreshnessTimer = timers.after(delay) {
            rideFreshnessTimer = null
            rideFreshnessScheduledAt = null
            view = view.copy(rideSession = AndroidRideSessionEffects.freshnessChecked(state, now()))
            publish()
        }
    }

    private inner class Connection(val token: MobileConnectionAttemptTokenDto, val device: BluetoothDevice, private val recordOnly: Boolean = false) {
        var gatt: BluetoothGatt? = null
        private var nativeNotificationCount = 0UL
        private var captureWriteId = 0UL
        private var captureLinkUp = false
        private var retired = false
        private var ending = false
        private var draining = false
        private val afterDrainCallbacks = mutableListOf<() -> Unit>()
        private val afterDrainFailureCallbacks = mutableListOf<(Throwable) -> Unit>()

        fun removeAfterDrainCallback(callback: () -> Unit) {
            afterDrainCallbacks.remove(callback)
        }
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
            },
            onCaptureNotification = { channel, service, bytes, semantic, evidence, atMs ->
                diagnostics.notification(token, channel, service, bytes, semantic, evidence, atMs)
            }) { snapshot, telemetry, settings ->
            if (current()) {
                if (snapshot.connection.deadlineMs == null) {
                    cancelDeadline?.invoke(); cancelDeadline = null
                }
                val connectionCandidate = if (snapshot.connection.readiness == MobileConnectionReadinessDto.VERIFIED) {
                    val displayName = view.devices.firstOrNull { it.id == token.platformIdentifier }
                        ?.candidate?.displayName ?: snapshot.identity?.model ?: "Connected device"
                    state.connectionAdmissionCandidate(token.platformIdentifier, displayName, false)
                } else null
                view = view.copy(session = snapshot, telemetry = telemetry, settings = settings,
                    connectionCandidate = connectionCandidate)
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
                    if (diagnostics.hasWriter(token)) {
                        diagnostics.linkUp(token, session.writeLimit.toInt())
                        captureLinkUp = diagnostics.hasWriter(token)
                    }
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
                if (diagnostics.hasWriter(token)) {
                    inventory.forEach { diagnostics.recordFingerprint(token, it) }
                }
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
                    if (status == BluetoothGatt.GATT_SUCCESS) {
                        session.notification(characteristic.uuid.wireBytes(), characteristic.service.uuid.wireBytes(), owned, capture = false)
                    }
                    // Protected/unreadable metadata is an observation gap, not a failed BLE link.
                    queue.complete(key("read", characteristic), true)
                    probe(); reschedule()
                }
            }
            override fun onCharacteristicChanged(gatt: BluetoothGatt, characteristic: BluetoothGattCharacteristic, value: ByteArray) {
                val owned = value.copyOf()
                callback(gatt) {
                    recordNativeNotification(characteristic)
                    session.notification(characteristic.uuid.wireBytes(), characteristic.service.uuid.wireBytes(), owned)
                    probe(); reschedule()
                }
            }
        }

        private fun characteristic(channel: ByteArray): BluetoothGattCharacteristic? =
            characteristics.filter { it.uuid == wireUuid(channel) }.singleOrNull()

        private fun key(kind: String, characteristic: BluetoothGattCharacteristic): String =
            "$kind:${characteristic.service.uuid}:${characteristic.uuid}"

        private fun recordNativeNotification(characteristic: BluetoothGattCharacteristic) {
            nativeNotificationCount++
            view = view.copy(notificationCount = nativeNotificationCount)
            publish()
        }

        private fun BluetoothGattCharacteristic.rolesForCapture(): List<MobileGattRoleDto> = buildList {
            val properties = properties
            if (properties and BluetoothGattCharacteristic.PROPERTY_READ != 0) add(MobileGattRoleDto.READ)
            if (properties and BluetoothGattCharacteristic.PROPERTY_WRITE != 0) add(MobileGattRoleDto.WRITE)
            if (properties and BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE != 0) add(MobileGattRoleDto.WRITE_WITHOUT_RESPONSE)
            if (properties and BluetoothGattCharacteristic.PROPERTY_NOTIFY != 0) add(MobileGattRoleDto.NOTIFY)
            if (properties and BluetoothGattCharacteristic.PROPERTY_INDICATE != 0) add(MobileGattRoleDto.INDICATE)
        }

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
            val writeType = mode ?: if (characteristic.properties and BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE != 0)
                BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE else BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
            val isWithoutResponse = writeType == BluetoothGattCharacteristic.WRITE_TYPE_NO_RESPONSE
            val captureId = if (isWithoutResponse && diagnostics.hasWriter(token)) ++captureWriteId else null
            val receipt = NativeWriteReceipt(chunks.size) { result ->
                session.writeReceipt(operationId, result)
                if (captureId != null) {
                    val disposition = when (result) {
                        NativeReceipt.QUEUED -> MobilePevcapWriteDispositionDto.QUEUED
                        NativeReceipt.SUBMITTED -> MobilePevcapWriteDispositionDto.SUBMITTED
                        NativeReceipt.REJECTED -> MobilePevcapWriteDispositionDto.REJECTED
                        NativeReceipt.CANCELLED -> MobilePevcapWriteDispositionDto.CANCELLED
                    }
                    diagnostics.writeReceipt(token, channel, bytes, captureId, disposition, now())
                }
            }
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
        fun finishTransport(
            failed: Boolean,
            afterDrainFailure: ((Throwable) -> Unit)? = null,
            afterDrain: (() -> Unit)? = null,
        ) {
            if (afterDrain != null) afterDrainCallbacks += afterDrain
            if (afterDrainFailure != null) afterDrainFailureCallbacks += afterDrainFailure
            if (retired || draining) return
            ending = true
            draining = true
            queue.close()
            cancelWakeup?.invoke(); cancelDeadline?.invoke()
            val endedAt = now()
            (context.applicationContext as CutoutApplication).rides.prepareForTransportEnd(state, token, endedAt) { result ->
                postOnOwner {
                    if (retired || connection !== this) return@postOnOwner
                    if (result.isFailure) {
                        draining = false
                        view = view.copy(problem = true)
                        publish()
                        val error = result.exceptionOrNull() ?: IllegalStateException("Bluetooth teardown failed")
                        afterDrainFailureCallbacks.toList().forEach { callback ->
                            try { callback(error) } catch (callbackError: Exception) {
                                Log.w("CutoutBle", "Bluetooth teardown failure callback failed", callbackError)
                            }
                        }
                        afterDrainFailureCallbacks.clear()
                        return@postOnOwner
                    }
                    if (diagnostics.hasWriter(token)) {
                        if (captureLinkUp) diagnostics.linkDown(token)
                        captureLinkUp = false
                        diagnostics.finishAfterLinkDown(token) {
                            completeTransportOnOwner(failed)
                        }
                    } else {
                        completeTransportOnOwner(failed)
                    }
                }
            }
        }

        private fun completeTransportOnOwner(failed: Boolean) {
            if (retired || connection !== this) return
            draining = false
            val wasVerified = state.deviceSessionSnapshot().connection.readiness == MobileConnectionReadinessDto.VERIFIED
            if (failed && wasVerified && !restoreSuppressed) {
                state.reduceRideSession(MobileRideSessionInputDto.BluetoothDisconnected(now()))
                rideReconnectPlatformIdentifier = token.platformIdentifier
                state.connectionLinkDown(token)
                retire(invalidate = false)
                connection = null
                view = view.copy(phase = NativePhase.IDLE, session = state.deviceSessionSnapshot(),
                    telemetry = null, settings = null, bms = null, problem = true, connectionCandidate = null)
                scheduleRustRetry(device, token)
            } else {
                if (failed) restoreAttemptedId = device.address
                if (failed) state.connectionTransportFailed(token) else state.disconnectConnectionAttempt()
                retire(invalidate = false)
                connection = null
                view = view.copy(phase = NativePhase.IDLE, session = state.deviceSessionSnapshot(),
                    telemetry = null, settings = null, bms = null, problem = failed, connectionCandidate = null)
            }
            publish()
            connectPendingOnOwner()
            afterDrainCallbacks.toList().forEach { callback ->
                try { callback() } catch (error: Exception) {
                    Log.w("CutoutBle", "Bluetooth shutdown callback failed", error)
                }
            }
            afterDrainCallbacks.clear()
            afterDrainFailureCallbacks.clear()
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

    companion object {
        private val secureRandom = SecureRandom()
    }
}
