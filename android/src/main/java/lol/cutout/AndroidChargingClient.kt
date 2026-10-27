package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import kotlinx.coroutines.runBlocking
import uniffi.cutout_mobile_ffi.*
import java.io.Closeable
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.ConcurrentHashMap

/** Keeps the same immutable Rust operation alive across transport-unknown retries. */
internal class ChargingReportOperationStore<T>(
    private val create: (spotId: String, message: String) -> T,
    private val close: (T) -> Unit,
) : Closeable {
    private val operations = ConcurrentHashMap<String, T>()

    fun getOrCreate(spotId: String, message: String): T =
        operations.computeIfAbsent(spotId) { create(spotId, message) }

    fun get(spotId: String): T? = operations[spotId]

    fun received(spotId: String, operation: T) {
        if (operations.remove(spotId, operation)) close(operation)
    }

    override fun close() {
        operations.values.forEach(close)
        operations.clear()
    }
}

data class ChargingClientStatus(
    val credentialState: MobileChargingSyncCredentialState,
    val result: MobileChargingSyncResult? = null,
    val pendingCount: UInt = 0u,
    val downloaded: UInt = 0u,
    val uploaded: UInt = 0u,
    val lastUpdatedMilliseconds: ULong? = null,
    val message: String? = null,
)

/** Android-native owner for Rust charging sync and its encrypted contributor credential. */
class AndroidChargingClient(
    context: Context,
    private val runtime: RideMapRuntime,
    private val credentials: AndroidChargingCredentials = AndroidChargingCredentials(context),
) : Closeable {
    private data class PendingChargingReport(val message: String, val report: MobileChargingContentReport)
    private val main = Handler(Looper.getMainLooper())
    // Charging FFI calls can await bounded HTTP. Keep that wait and its retry delay away
    // from RideMapRuntime's serialized recording and required-callback worker.
    private val worker = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(32),
        { task -> Thread(task, "CutoutChargingIO").apply { isDaemon = true } },
        ThreadPoolExecutor.AbortPolicy(),
    )
    // Capture batches can wait on large HTTP transfers. Keep that work off the shared
    // charging/database worker while bounding both concurrency and queued batches.
    private val captureBatchWorker = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(1),
        { task -> Thread(task, "CutoutCaptureBatchIO").apply { isDaemon = true } },
        ThreadPoolExecutor.AbortPolicy(),
    )
    @Volatile var status: ChargingClientStatus = ChargingClientStatus(MobileChargingSyncCredentialState.MISSING)
        private set
    @Volatile private var closed = false
    private val reportOperations = ChargingReportOperationStore(
        create = { spotId, message -> PendingChargingReport(message, MobileChargingContentReport(spotId, message)) },
        close = { it.report.close() },
    )

    fun <T> withDatabase(operation: suspend (RideDatabaseHandle) -> T, completed: (Result<T>) -> Unit) {
        if (closed) {
            main.post { completed(Result.failure(IllegalStateException("Charging client is closed"))) }
            return
        }
        try {
            worker.execute {
                val result = runCatching {
                    check(!closed) { "Charging client is closed" }
                    val database = runtime.databaseForNativeOwner()
                    runBlocking { operation(database) }
                }
                main.post { completed(result) }
            }
        } catch (error: RejectedExecutionException) {
            main.post { completed(Result.failure(error)) }
        }
    }

    /** Runs one local capture review away from the charging worker. */
    internal fun <T> runCaptureReview(
        operation: suspend (RideDatabaseHandle) -> T,
        completed: (Result<T>) -> Unit,
    ) {
        runCaptureBatch({ operation(runtime.databaseForNativeOwner()) }, completed)
    }

    /** Runs one admitted, long-running capture batch away from charging/database work. */
    internal fun <T> runCaptureBatch(operation: suspend () -> T, completed: (Result<T>) -> Unit) {
        if (closed) {
            main.post { completed(Result.failure(IllegalStateException("Charging client is closed"))) }
            return
        }
        try {
            captureBatchWorker.execute {
                val result = runCatching {
                    check(!closed) { "Charging client is closed" }
                    runBlocking { operation() }
                }
                main.post { completed(result) }
            }
        } catch (error: RejectedExecutionException) {
            main.post { completed(Result.failure(error)) }
        }
    }

    /** Equivalent to Swift's charging-page sync request; registration uncertainty is retained. */
    fun synchronize(explicitRetry: Boolean = false, completed: (Result<ChargingClientStatus>) -> Unit) {
        withDatabase({ database -> synchronize(database, explicitRetry) }, completed)
    }

    private suspend fun synchronize(
        database: RideDatabaseHandle,
        explicitRetry: Boolean,
    ): ChargingClientStatus {
        check(!closed) { "Charging client is closed" }
        val stored = try {
            credentials.load()
        } catch (error: Throwable) {
            database.refreshChargingDirectory()
            return publish(ChargingClientStatus(
                MobileChargingSyncCredentialState.UNREADABLE,
                message = error.message,
                lastUpdatedMilliseconds = database.chargingDirectoryLastUpdatedMs(),
            ))
        }
        val admission = mobileChargingRegistrationAdmission(credentialState(stored), explicitRetry)
        val credential = try { when (admission) {
            MobileChargingRegistrationAdmission.BEGIN_REGISTRATION -> {
                if (credentials.beginRegistration()) registerAndPersist()
                else when (val raced = credentials.load()) {
                    is ChargingCredentialRecord.Ready -> MobileChargingIdentityDto(raced.identityId, raced.token)
                    else -> {
                        database.refreshChargingDirectory()
                        return publish(ChargingClientStatus(
                            MobileChargingSyncCredentialState.REGISTRATION_STARTED,
                            message = "Registration needs an explicit retry.",
                            lastUpdatedMilliseconds = database.chargingDirectoryLastUpdatedMs(),
                        ))
                    }
                }
            }
            MobileChargingRegistrationAdmission.REGISTER_IDENTITY -> registerAndPersist()
            MobileChargingRegistrationAdmission.SYNCHRONIZE -> {
                val ready = stored as? ChargingCredentialRecord.Ready
                    ?: error("Ready charging credential was unavailable")
                MobileChargingIdentityDto(ready.identityId, ready.token)
            }
            MobileChargingRegistrationAdmission.PUBLIC_REFRESH -> {
                val summary = database.refreshChargingDirectory()
                return publish(ChargingClientStatus(
                    credentialState(stored), resultFor(summary.outcome), summary.pendingCount,
                    summary.downloaded, summary.uploaded, database.chargingDirectoryLastUpdatedMs(),
                ))
            }
            MobileChargingRegistrationAdmission.REQUIRE_EXPLICIT_RETRY -> {
                database.refreshChargingDirectory()
                return publish(ChargingClientStatus(
                    MobileChargingSyncCredentialState.REGISTRATION_STARTED,
                    message = "Registration outcome is uncertain. Retry to continue.",
                    lastUpdatedMilliseconds = database.chargingDirectoryLastUpdatedMs(),
                ))
            }
        } } catch (error: Throwable) {
            val current = runCatching { credentials.load() }.getOrNull()
            if (current == ChargingCredentialRecord.RegistrationStarted) {
                runCatching { database.refreshChargingDirectory() }
                return publish(ChargingClientStatus(
                    MobileChargingSyncCredentialState.REGISTRATION_STARTED,
                    message = "Registration outcome is uncertain. Retry to continue.",
                    lastUpdatedMilliseconds = runCatching { database.chargingDirectoryLastUpdatedMs() }.getOrNull(),
                ))
            }
            throw error
        }

        var runState = MobileChargingSyncRunState(0u, 0u, 0u, 0u, 0u)
        while (true) {
            val step = try {
                val summary = database.synchronizeChargingSpots(credential.token)
                mobileChargingSyncObserveSummary(runState, summary)
            } catch (error: MobileChargingException) {
                mobileChargingSyncObserveError(runState, error)
            }
            runState = step.state
            when (val action = step.action) {
                MobileChargingSyncAction.NextRound -> continue
                is MobileChargingSyncAction.Retry -> {
                    Thread.sleep(action.afterSeconds.toLong() * 1_000L)
                }
                is MobileChargingSyncAction.Finished -> {
                    val next = ChargingClientStatus(
                        MobileChargingSyncCredentialState.READY,
                        action.result,
                        runState.pendingCount,
                        runState.downloaded,
                        runState.uploaded,
                        database.chargingDirectoryLastUpdatedMs(),
                    )
                    val message = if (action.result == MobileChargingSyncResult.AUTHENTICATION_REQUIRED) {
                        runCatching { database.refreshChargingDirectory() }.exceptionOrNull()?.message
                    } else null
                    return publish(next.copy(message = message))
                }
            }
        }
    }

    fun refreshDirectory(completed: (Result<MobileChargingSyncSummaryDto>) -> Unit) =
        withDatabase({ it.refreshChargingDirectory() }, completed)

    /** Reads the encrypted credential owner without triggering registration or network work. */
    fun refreshAccountCredentialState(completed: (Result<MobileChargingSyncCredentialState>) -> Unit) {
        if (closed) {
            main.post { completed(Result.failure(IllegalStateException("Charging client is closed"))) }
            return
        }
        try {
            worker.execute {
                val result = runCatching {
                    check(!closed) { "Charging client is closed" }
                    val current = credentialState(credentials.load())
                    publish(status.copy(credentialState = current))
                    current
                }
                main.post { completed(result) }
            }
        } catch (error: RejectedExecutionException) {
            main.post { completed(Result.failure(error)) }
        }
    }

    fun submitReport(spotId: String, message: String, completed: (Result<MobileChargingReportReceiptDto>) -> Unit) {
        val pending = reportOperations.getOrCreate(spotId, message)
        withDatabase({
            val credential = credentialForExplicitAction()
            pending.report.submit(credential.token)
        }) { result ->
            result.onSuccess { reportOperations.received(spotId, pending) }
            completed(result)
        }
    }

    fun pendingReportMessage(spotId: String): String? = reportOperations.get(spotId)?.message

    fun deleteAccount(completed: (Result<MobileCommunityAccountDeletionReceiptDto>) -> Unit) {
        withDatabase({ database ->
            when (val record = credentials.load()) {
                is ChargingCredentialRecord.Ready -> {
                    credentials.markDeletionPending()
                    publish(status.copy(credentialState = MobileChargingSyncCredentialState.DELETION_PENDING))
                }
                is ChargingCredentialRecord.DeletionPending -> Unit
                else -> error("No active charging identity is available")
            }
            val pending = credentials.load() as ChargingCredentialRecord.DeletionPending
            try {
                val receipt = database.deleteCommunityAccount(pending.token)
                credentials.finishDeletion()
                publish(ChargingClientStatus(
                    MobileChargingSyncCredentialState.DELETED,
                    result = MobileChargingSyncResult.SYNCED,
                    lastUpdatedMilliseconds = database.chargingDirectoryLastUpdatedMs(),
                    message = "Community account deleted.",
                ))
                receipt
            } catch (error: Throwable) {
                // Keep both Rust's deletion fence and the encrypted token for safe retry.
                throw error
            }
        }, completed)
    }

    /** Only an explicit capture Send may obtain or provision this existing private identity. */
    internal suspend fun captureCredential(): MobileChargingIdentityDto = credentialForExplicitAction()

    internal fun existingCaptureCredential(): MobileChargingIdentityDto? = when (val stored = credentials.load()) {
        is ChargingCredentialRecord.Ready -> MobileChargingIdentityDto(stored.identityId, stored.token)
        is ChargingCredentialRecord.DeletionPending -> MobileChargingIdentityDto(stored.identityId, stored.token)
        else -> null
    }

    private suspend fun credentialForExplicitAction(): MobileChargingIdentityDto {
        val stored = credentials.load()
        return when (mobileChargingRegistrationAdmission(credentialState(stored), false)) {
            MobileChargingRegistrationAdmission.SYNCHRONIZE -> {
                val ready = stored as? ChargingCredentialRecord.Ready ?: error("Credential unavailable")
                MobileChargingIdentityDto(ready.identityId, ready.token)
            }
            MobileChargingRegistrationAdmission.BEGIN_REGISTRATION -> {
                if (credentials.beginRegistration()) registerAndPersist()
                else {
                    val raced = credentials.load() as? ChargingCredentialRecord.Ready
                        ?: error("Charging identity registration needs an explicit retry")
                    MobileChargingIdentityDto(raced.identityId, raced.token)
                }
            }
            MobileChargingRegistrationAdmission.REGISTER_IDENTITY -> registerAndPersist()
            MobileChargingRegistrationAdmission.PUBLIC_REFRESH,
            MobileChargingRegistrationAdmission.REQUIRE_EXPLICIT_RETRY -> error("Charging identity needs recovery")
        }
    }

    private suspend fun registerAndPersist(): MobileChargingIdentityDto {
        val identity = mobileRegisterChargingIdentity()
        credentials.saveIdentity(identity.identityId, identity.token)
        return identity
    }

    private fun credentialState(record: ChargingCredentialRecord?): MobileChargingSyncCredentialState = when (record) {
        null -> MobileChargingSyncCredentialState.MISSING
        ChargingCredentialRecord.RegistrationStarted -> MobileChargingSyncCredentialState.REGISTRATION_STARTED
        is ChargingCredentialRecord.Ready -> MobileChargingSyncCredentialState.READY
        is ChargingCredentialRecord.DeletionPending -> MobileChargingSyncCredentialState.DELETION_PENDING
        ChargingCredentialRecord.Deleted -> MobileChargingSyncCredentialState.DELETED
    }

    private fun resultFor(outcome: MobileChargingSyncOutcome): MobileChargingSyncResult? = when (outcome) {
        MobileChargingSyncOutcome.MoreWork -> MobileChargingSyncResult.PENDING
        is MobileChargingSyncOutcome.Finished -> outcome.result
    }

    private fun publish(value: ChargingClientStatus): ChargingClientStatus {
        status = value
        return value
    }

    override fun close() {
        closed = true
        reportOperations.close()
        worker.shutdown()
        captureBatchWorker.shutdown()
    }
}
