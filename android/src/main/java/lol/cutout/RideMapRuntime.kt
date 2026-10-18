package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import uniffi.cutout_mobile_ffi.*
import java.io.Closeable
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit

/** A UI-facing copy of the authoritative Rust ride state. */
data class RideMapPresentation(
    val snapshot: MobileRideMapCoreSnapshotDto? = null,
    val error: String? = null,
    val ready: Boolean = false,
    val locationAcquisition: MobileRideMapLocationAcquisitionDto? = null,
)

/** Native callback owner that must drain Rust-required effects before a Stop reaches Core. */
interface RideMapStopAdmissionFence {
    fun beforeRideStop(proceed: () -> Unit)
    fun onRideStopFailed()
    fun onRideRecordingActive()
}

/**
 * Process-scoped owner for the one durable ride database and its Rust ride-map core.
 * All FFI reads and mutations share one bounded serialized worker. Platform location and
 * permission effects remain with the Activity/foreground-service owner.
 */
class RideMapRuntime internal constructor(context: Context, private val databaseName: String) : Closeable {
    constructor(context: Context) : this(context, DATABASE_NAME)
    private val appContext = context.applicationContext
    private val main = Handler(Looper.getMainLooper())
    private val worker = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(64),
        { task -> Thread(task, "CutoutRideMap").apply { isDaemon = true } },
        ThreadPoolExecutor.AbortPolicy(),
    )
    private val observers = linkedSetOf<(RideMapPresentation) -> Unit>()
    private val lock = Any()
    private val requiredWorkLock = Any()
    private val databaseReady = CompletableFuture<RideDatabaseHandle>()

    @Volatile private var database: RideDatabaseHandle? = null
    @Volatile private var core: MobileRideMapCore? = null
    @Volatile private var presentation = RideMapPresentation()
    @Volatile private var closed = false
    @Volatile private var stopAdmissionFence: RideMapStopAdmissionFence? = null
    @Volatile private var futureMusicHistoryPolicy = MobileMusicHistoryPolicyDto.DISABLED

    init {
        enqueue(
            work = {
                var openedDatabase: RideDatabaseHandle? = null
                var openedCore: MobileRideMapCore? = null
                try {
                    val databaseFile = appContext.getDatabasePath(databaseName)
                    databaseFile.parentFile?.mkdirs()
                    val db = openRideDatabase(databaseFile.absolutePath)
                    openedDatabase = db
                    val rideCore = MobileRideMapCore.withDatabase(db)
                    openedCore = rideCore
                    // Restore must complete on the serialized ride owner before any ride command.
                    rideCore.restore(elapsedRealtimeMillis())
                    val restored = RideMapPresentation(
                        snapshot = rideCore.currentSnapshot(elapsedRealtimeMillis()),
                        error = rideCore.initializationError()?.toString(),
                        ready = rideCore.isReady(),
                    )
                    database = db
                    core = rideCore
                    databaseReady.complete(db)
                    restored
                } catch (error: Exception) {
                    databaseReady.completeExceptionally(error)
                    runCatching { openedCore?.close() }
                    runCatching { openedDatabase?.close() }
                    throw error
                }
            },
            completed = { result ->
                publish(result.fold(
                    onSuccess = { it },
                    onFailure = {
                        databaseReady.completeExceptionally(it)
                        RideMapPresentation(error = it.message ?: it.toString(), ready = false)
                    },
                ))
            },
        )
    }

    /** Returns this runtime's shared database to another serialized native owner. */
    fun databaseForNativeOwner(): RideDatabaseHandle {
        check(Looper.myLooper() != Looper.getMainLooper()) {
            "Waiting for the ride database is not allowed on the main thread"
        }
        return try {
            databaseReady.get()
        } catch (error: Exception) {
            throw IllegalStateException("Ride database initialization failed", error.cause ?: error)
        }
    }

    fun observe(observer: (RideMapPresentation) -> Unit): AutoCloseable {
        synchronized(lock) { observers += observer }
        main.post { observer(presentation) }
        return Closeable { synchronized(lock) { observers -= observer } }
    }

    fun <T> withDatabase(work: (RideDatabaseHandle) -> T, completed: (Result<T>) -> Unit) {
        enqueue({ work(checkNotNull(database) { "Ride database is not ready" }) }, completed)
    }

    fun <T> withCore(work: (MobileRideMapCore) -> T, completed: (Result<T>) -> Unit) {
        enqueue({ work(checkNotNull(core) { "Ride map is not ready" }) }, completed)
    }

    /** Registers the one app-scoped music owner whose accepted history work precedes Stop. */
    fun setStopAdmissionFence(fence: RideMapStopAdmissionFence?) {
        stopAdmissionFence = fence
    }

    /** Supplies Rust's default for future BLE-created rides without constructing a music adapter. */
    fun setFutureRideMusicHistoryPolicy(policy: MobileMusicHistoryPolicyDto) {
        futureMusicHistoryPolicy = policy
    }

    /** Binds observation admission to this runtime's exact Rust Core before client callbacks start. */
    fun bindMusicObservationLifecycle(
        lifecycle: MobileMusicProviderLifecycle,
        completed: (Result<Unit>) -> Unit,
    ) {
        withCore({ it.bindMusicObservationLifecycle(lifecycle) }, completed)
    }

    fun startGpsRide(
        policy: MobileMusicHistoryPolicyDto = MobileMusicHistoryPolicyDto.DISABLED,
        completed: (Result<MobileRideMapCoreSnapshotDto>) -> Unit,
    ) {
        mutate({ result ->
            if (result.isSuccess) stopAdmissionFence?.onRideRecordingActive()
            completed(result)
        }) { rideCore ->
            val command = rideCore.beginStartGpsOnlyCommand(elapsedRealtimeMillis(), policy)
            try {
                while (true) {
                    when (val state = command.poll()) {
                        MobileRideMapLifecyclePollDto.Pending -> Thread.sleep(POLL_INTERVAL_MS)
                        is MobileRideMapLifecyclePollDto.Completed -> return@mutate state.snapshot
                    }
                }
                error("Unreachable ride start state")
            } finally {
                command.close()
            }
        }
    }

    fun pause(completed: (Result<MobileRideMapCoreSnapshotDto>) -> Unit) =
        mutate(completed) { it.pauseAt(elapsedRealtimeMillis()) }

    fun resume(completed: (Result<MobileRideMapCoreSnapshotDto>) -> Unit) =
        mutate({ result ->
            if (result.isSuccess) stopAdmissionFence?.onRideRecordingActive()
            completed(result)
        }) { it.resumeAt(elapsedRealtimeMillis()) }

    fun stop(completed: (Result<MobileRideMapCoreSnapshotDto>) -> Unit) {
        val submit = {
            mutate({ result ->
                if (result.isFailure) stopAdmissionFence?.onRideStopFailed()
                completed(result)
            }) { it.stopAt(elapsedRealtimeMillis()) }
        }
        val fence = stopAdmissionFence
        if (fence == null) submit() else fence.beforeRideStop(submit)
    }

    fun save(completed: (Result<MobileRideMapCoreSnapshotDto>) -> Unit) =
        mutate(completed) { it.save() }

    fun discard(completed: (Result<MobileRideMapCoreSnapshotDto>) -> Unit) =
        mutate(completed) { it.discard() }

    fun refreshLocationEnvironment(
        environment: MobileRideMapLocationEnvironmentDto,
        completed: (Result<MobileRideMapLocationAcquisitionDto>) -> Unit,
    ) {
        enqueue({
            val acquisition = checkNotNull(core) { "Ride map is not ready" }
                .observeLocationEnvironment(environment)
            publishOnWorker(presentation.copy(locationAcquisition = acquisition))
            acquisition
        }, completed)
    }

    /**
     * Admits one native location callback using Rust's recording token and timestamp policy.
     * Samples are forwarded as received. Invalid/stale/future admission is decided in Rust.
     */
    fun submitLocationCallback(
        samples: List<MobilePhoneLocationSampleDto>,
        receiptMonotonicMs: ULong = elapsedRealtimeMillis(),
        receiptWallClockUnixMs: ULong = System.currentTimeMillis().toULong(),
        completed: (Result<List<MobileRideMapCoreOutcomeDto>>) -> Unit,
    ) {
        enqueue({
            val rideCore = checkNotNull(core) { "Ride map is not ready" }
            val snapshot = rideCore.currentSnapshot(elapsedRealtimeMillis())
                ?: error("There is no active ride to record location")
            val outcomes = rideCore.ingestLocationCallbackWithOutcomes(
                snapshot.recordingToken,
                receiptMonotonicMs,
                receiptWallClockUnixMs,
                samples,
            )
            outcomes.lastOrNull()?.let { publishOnWorker(presentation.copy(snapshot = it.snapshot)) }
            outcomes
        }, completed)
    }

    /** Applies a Rust-verified Bluetooth connection using the existing session owner. */
    fun observeConnection(
        value: BlePresentation,
        sessionState: CutoutSessionStateHandle,
        musicHistoryPolicy: MobileMusicHistoryPolicyDto? = null,
        completed: ((Result<MobileRideMapCoreSnapshotDto?>) -> Unit)? = null,
    ) {
        enqueue({
            val session = value.session ?: return@enqueue null
            val connection = session.connection
            val token = connection.token ?: return@enqueue null
            if (connection.readiness != MobileConnectionReadinessDto.VERIFIED) return@enqueue null
            val rideCore = checkNotNull(core) { "Ride map is not ready" }
            val admission = sessionState.beginRideRecordingForVerifiedConnection(
                rideCore, token, elapsedRealtimeMillis(), musicHistoryPolicy ?: futureMusicHistoryPolicy,
            )
            try {
                val snapshot = admission.wait()
                if (snapshot != null) {
                    publishOnWorker(presentation.copy(snapshot = snapshot))
                    if (snapshot.state == MobileRideLifecycleStateDto.ACTIVE) {
                        stopAdmissionFence?.onRideRecordingActive()
                    }
                }
                snapshot
            } finally {
                admission.close()
            }
        }, completed ?: {})
    }

    /**
     * Admits one ordered Rust session step from the BLE owner. The callback time and speed
     * sample time are supplied by PevSession from that exact Rust session step.
     */
    fun observeRideTelemetry(
        sessionState: CutoutSessionStateHandle,
        token: MobileConnectionAttemptTokenDto,
        receivedAtMs: ULong,
        speedObservation: MobileRideMapSpeedObservationDto?,
        musicHistoryPolicy: MobileMusicHistoryPolicyDto? = null,
        completed: ((Result<MobileRideMapTelemetryObservationDto>) -> Unit)? = null,
    ) {
        val permit = try {
            check(Looper.myLooper() != Looper.getMainLooper()) {
                "BLE ride observations must not block the main thread"
            }
            checkNotNull(core) { "Ride map is not ready" }.reserveRecordingWork(1UL)
        } catch (error: Exception) {
            main.post { completed?.invoke(Result.failure(error)) }
            return
        }
        enqueueRequired(permit, {
            val rideCore = checkNotNull(core) { "Ride map is not ready" }
            val admission = sessionState.beginRideRecordingForVerifiedConnection(
                rideCore,
                token,
                receivedAtMs,
                musicHistoryPolicy ?: futureMusicHistoryPolicy,
            )
            try {
                val admitted = admission.wait()
                if (admitted != null) {
                    publishOnWorker(presentation.copy(snapshot = admitted))
                    if (admitted.state == MobileRideLifecycleStateDto.ACTIVE) {
                        stopAdmissionFence?.onRideRecordingActive()
                    }
                }
            } finally {
                admission.close()
            }

            val observed = sessionState.observeRideTelemetryForVerifiedConnection(
                rideCore,
                token,
                receivedAtMs,
                speedObservation,
            )
            rideCore.currentSnapshot(receivedAtMs)?.let { current ->
                publishOnWorker(presentation.copy(snapshot = current, error = null))
            }
            observed
        }, completed ?: {})
    }

    /**
     * Drains all earlier BLE telemetry before closing the Rust connection attempt. Callers
     * must await this main-thread completion before invalidating the token or releasing GATT.
     */
    fun prepareForTransportEnd(
        sessionState: CutoutSessionStateHandle,
        token: MobileConnectionAttemptTokenDto,
        atMs: ULong,
        completed: (Result<MobileRideMapCoreSnapshotDto?>) -> Unit,
    ) {
        val permit = try {
            check(Looper.myLooper() != Looper.getMainLooper()) {
                "BLE transport teardown must not block the main thread"
            }
            checkNotNull(core) { "Ride map is not ready" }.reserveRecordingWork(1UL)
        } catch (error: Exception) {
            main.post { completed(Result.failure(error)) }
            return
        }
        enqueueRequired(permit, {
            val rideCore = checkNotNull(core) { "Ride map is not ready" }
            if (!sessionState.connectionAttemptIsCurrent(token)) {
                return@enqueueRequired rideCore.currentSnapshot(atMs)
            }
            val current = rideCore.currentSnapshot(atMs)
            val prepared = rideCore.prepareDisconnect(current?.recordingToken, atMs)
            val latest = prepared ?: current
            publishOnWorker(presentation.copy(snapshot = latest))
            latest
        }, completed)
    }

    private fun mutate(
        completed: (Result<MobileRideMapCoreSnapshotDto>) -> Unit,
        operation: (MobileRideMapCore) -> MobileRideMapCoreSnapshotDto,
    ) {
        enqueue({
            val updated = operation(checkNotNull(core) { "Ride map is not ready" })
            publishOnWorker(presentation.copy(snapshot = updated, error = null))
            updated
        }, completed)
    }

    private fun <T> enqueue(work: () -> T, completed: (Result<T>) -> Unit) {
        if (closed) {
            main.post { completed(Result.failure(IllegalStateException("Ride map runtime is closed"))) }
            return
        }
        try {
            worker.execute {
                val result = runCatching(work)
                result.exceptionOrNull()?.let { error ->
                    publishOnWorker(presentation.copy(error = error.message ?: error.toString()))
                }
                main.post { completed(result) }
            }
        } catch (error: RejectedExecutionException) {
            main.post { completed(Result.failure(error)) }
        }
    }

    /**
     * Required transport work is capacity-admitted by Rust before this handoff. Queue insertion
     * waits off-main instead of dropping an accepted observation or allowing a terminal barrier
     * to overtake it when the ordinary bounded executor queue is full.
     */
    private fun <T> enqueueRequired(
        permit: MobileRideMapRecordingWorkPermit,
        work: () -> T,
        completed: (Result<T>) -> Unit,
    ) {
        val task = Runnable {
            try {
                val result = runCatching(work)
                result.exceptionOrNull()?.let { error ->
                    publishOnWorker(presentation.copy(error = error.message ?: error.toString()))
                }
                main.post { completed(result) }
            } finally {
                permit.close()
            }
        }
        try {
            synchronized(requiredWorkLock) {
                if (closed) throw IllegalStateException("Ride map runtime is closed")
                worker.queue.put(task)
            }
        } catch (error: InterruptedException) {
            Thread.currentThread().interrupt()
            permit.close()
            main.post { completed(Result.failure(error)) }
        } catch (error: Exception) {
            permit.close()
            main.post { completed(Result.failure(error)) }
        }
    }

    private fun publishOnWorker(next: RideMapPresentation) = publish(next)

    internal fun reportLocationFailure(message: String?) {
        publish(presentation.copy(error = message))
    }

    private fun publish(next: RideMapPresentation) {
        presentation = next
        val callbacks = synchronized(lock) { observers.toList() }
        main.post { callbacks.forEach { it(next) } }
    }

    override fun close() {
        synchronized(requiredWorkLock) {
            if (closed) return
            closed = true
        }
        try {
            worker.execute {
                core?.close()
                database?.close()
                core = null
                database = null
                databaseReady.cancel(false)
            }
        } catch (_: RejectedExecutionException) { }
        worker.shutdown()
        synchronized(lock) { observers.clear() }
    }

    private fun elapsedRealtimeMillis(): ULong = (SystemClock.elapsedRealtimeNanos() / 1_000_000L).toULong()

    companion object {
        private const val DATABASE_NAME = "ride.sqlite"
        private const val POLL_INTERVAL_MS = 25L
    }
}
