package lol.cutout

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import uniffi.cutout_mobile_ffi.*
import java.io.File
import java.util.UUID
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicBoolean

data class DiagnosticCapturePresentation(
    val ready: Boolean = false,
    val lifecycle: MobileCaptureLifecycleSnapshotDto = MobileCaptureLifecycleSnapshotDto(null, true, true),
    val writerStatus: MobileCaptureWriterStatusDto? = null,
    val notificationCount: ULong = 0UL,
    val fileName: String? = null,
    val deviceName: String? = null,
    val note: String? = null,
    val error: String? = null,
    val historyError: String? = null,
    val managedCaptureCleanupError: String? = null,
    val captures: List<MobileLiveCaptureHistoryEntryDto> = emptyList(),
    val sessionArtifacts: List<DiagnosticCaptureArtifact> = emptyList(),
    val nextCursor: MobileLiveCaptureHistoryCursorDto? = null,
    val loading: Boolean = false,
)

data class DiagnosticCaptureArtifact(
    val id: String,
    val file: File,
    val origin: MobileCaptureOriginDto,
    val deviceName: String?,
    val description: String?,
    val status: MobileCaptureWriterStatusDto,
    val notificationCount: ULong,
    val error: String?,
)

internal fun localCapturePayloadMayBeShared(state: MobileCaptureSubmissionLocalPayloadStateDto?): Boolean =
    state == null || state == MobileCaptureSubmissionLocalPayloadStateDto.RETAINED

internal enum class CaptureTransportOwnership { BORROWED, CAPTURE_OWNED }

/** Native writer/filesystem effects; Rust remains the lifecycle, admission, and history owner. */
class AndroidDiagnosticCaptureClient internal constructor(
    private val context: Context,
    private val state: () -> CutoutSessionStateHandle,
    private val database: () -> RideDatabaseHandle,
    private val postOwner: (() -> Unit) -> Boolean,
    private val onFinishManual: (MobileConnectionAttemptTokenDto?, CaptureTransportOwnership) -> Unit,
) {
    private data class Writer(
        val generation: MobileCaptureGenerationDto,
        val builder: MobilePevcapCaptureBuilder,
        val path: String,
        val platformId: String,
        val advertisedName: String?,
        val origin: MobileCaptureOriginDto,
        val description: String?,
        val connectionToken: MobileConnectionAttemptTokenDto?,
        val transportOwnership: CaptureTransportOwnership,
        var priorOutcome: MobileCaptureWriteOutcomeDto = MobileCaptureWriteOutcomeDto.Accepted,
        var notificationCount: ULong = 0UL,
        var finalizationStarted: Boolean = false,
        val finishWaiters: MutableList<() -> Unit> = mutableListOf(),
    )

    private val main = Handler(Looper.getMainLooper())
    private val observers = CopyOnWriteArrayList<(DiagnosticCapturePresentation) -> Unit>()
    private val managedCaptureDeduplicationQueued = AtomicBoolean(false)
    private val managedCaptureDeduplicationRerunRequested = AtomicBoolean(false)
    private val io = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(16),
        { Thread(it, "CutoutCaptureIO") }, ThreadPoolExecutor.AbortPolicy(),
    )
    // Keep one queue slot available for the Rust writer's final publication receipt.
    private val ioSubmissionLock = Any()
    private var writer: Writer? = null // Accessed only on AndroidBleClient's Bluetooth owner.
    private var closing = false // Protected by ioSubmissionLock for submissions; otherwise owner-only.
    private var closeBarrierQueued = false
    private var closeComplete = false
    private val closeWaiters = mutableListOf<() -> Unit>()
    @Volatile private var presentation = DiagnosticCapturePresentation()

    fun current(): DiagnosticCapturePresentation = presentation

    fun observe(observer: (DiagnosticCapturePresentation) -> Unit): AutoCloseable {
        observers += observer
        main.post { if (observer in observers) observer(presentation) }
        return AutoCloseable { observers.remove(observer) }
    }

    fun refresh() {
        postOwner {
            val snapshot = state().captureLifecycleSnapshot()
            val active = writer
            publish(presentation.copy(
                ready = true,
                lifecycle = snapshot,
                writerStatus = active?.builder?.writerStatus(),
                notificationCount = active?.notificationCount ?: 0UL,
                fileName = active?.let { File(it.path).name },
                deviceName = active?.advertisedName,
                note = active?.description,
            ))
        }
        refreshHistory()
    }

    fun refreshHistory(cursor: MobileLiveCaptureHistoryCursorDto? = null) {
        postOwner {
            publish(presentation.copy(loading = true))
            try {
                enqueueIo {
                    val result = runCatching { database().listLiveCaptureHistory(cursor, 50u) }
                    postOwner {
                        result.onSuccess { page ->
                            val existing = if (cursor == null) emptyList() else presentation.captures
                            publish(presentation.copy(
                                captures = existing + page.captures,
                                nextCursor = page.nextCursor,
                                loading = false,
                                historyError = null,
                            ))
                        }.onFailure { error ->
                            publish(presentation.copy(loading = false, historyError = error.message ?: error.toString()))
                        }
                    }
                }
            } catch (error: java.util.concurrent.RejectedExecutionException) {
                publish(presentation.copy(loading = false, historyError = error.message ?: context.getString(R.string.capture_error)))
            }
        }
    }

    /** Reconciles only app-managed capture files with Rust's exact durable SQLite bytes. */
    fun deduplicateManagedCaptureFiles() {
        if (!managedCaptureDeduplicationQueued.compareAndSet(false, true)) {
            managedCaptureDeduplicationRerunRequested.set(true)
            return
        }
        try {
            enqueueIo {
                var runAgain: Boolean
                do {
                    managedCaptureDeduplicationRerunRequested.set(false)
                    val outcome = runCatching {
                        val originals = File(context.filesDir, "DiagnosticCaptures")
                        check(originals.isDirectory || originals.mkdirs()) {
                            "Could not create app capture directory"
                        }
                        val exports = File(context.cacheDir, "diagnostic-exports")
                        val database = database()
                        listOf(originals, exports).fold(0UL to 0UL) { totals, directory ->
                            val pass = database.deduplicateManagedCaptureFiles(directory.absolutePath)
                            (totals.first + pass.removedFiles) to (totals.second + pass.removedBytes)
                        }
                    }
                    postOwner {
                        outcome.onSuccess { result ->
                            publish(presentation.copy(managedCaptureCleanupError = null))
                            if (result.first > 0UL) {
                                Log.i(
                                    TAG,
                                    "Removed ${result.first} duplicate managed capture files " +
                                        "(${result.second} bytes)",
                                )
                            }
                        }.onFailure { error ->
                            publish(presentation.copy(
                                managedCaptureCleanupError = error.message ?: context.getString(R.string.capture_error),
                            ))
                        }
                    }
                    runAgain = managedCaptureDeduplicationRerunRequested.getAndSet(false)
                } while (runAgain)
                managedCaptureDeduplicationQueued.set(false)
                if (managedCaptureDeduplicationRerunRequested.getAndSet(false)) {
                    deduplicateManagedCaptureFiles()
                }
            }
        } catch (error: Exception) {
            managedCaptureDeduplicationQueued.set(false)
            postOwner {
                publish(presentation.copy(
                    managedCaptureCleanupError = error.message ?: context.getString(R.string.capture_error),
                ))
            }
            if (managedCaptureDeduplicationRerunRequested.getAndSet(false)) {
                deduplicateManagedCaptureFiles()
            }
        }
    }

    fun export(liveCaptureId: String, completed: (Result<File>) -> Unit) {
        try {
            enqueueIo {
                val result = runCatching {
                    requireLocalPayloadRetained(
                        database(), MobileCaptureSubmissionSourceDto.LiveCapture(liveCaptureId),
                    )
                    val directory = File(context.cacheDir, "diagnostic-exports").apply { mkdirs() }
                    val destination = File(directory, "cutout-capture-${UUID.randomUUID()}.jsonl").canonicalFile
                    val exported = database().exportLiveCapture(liveCaptureId, destination.absolutePath)
                    val returned = File(exported.path).canonicalFile
                    require(returned.parentFile == directory.canonicalFile) { "Capture export escaped its sharing directory" }
                    returned
                }
                main.post { completed(result) }
            }
        } catch (error: java.util.concurrent.RejectedExecutionException) {
            main.post { completed(Result.failure(error)) }
        }
    }

    fun exportSessionArtifact(artifact: DiagnosticCaptureArtifact, completed: (Result<File>) -> Unit) {
        try {
            enqueueIo {
                val result = runCatching {
                    requireLocalPayloadRetained(
                        database(), MobileCaptureSubmissionSourceDto.RecordedArtifact(artifact.id),
                    )
                    val directory = File(context.cacheDir, "diagnostic-exports").apply { mkdirs() }.canonicalFile
                    val destination = File(directory, "cutout-capture-${UUID.randomUUID()}.jsonl").canonicalFile
                    try {
                        val exported = database().exportRecordedCapture(artifact.id, destination.absolutePath)
                        File(exported.path)
                    } catch (error: MobileRideDatabaseException.PevcapCaptureNotFound) {
                        val source = artifact.file.canonicalFile
                        val captures = File(context.filesDir, "DiagnosticCaptures").canonicalFile
                        require(source.parentFile == captures && source.isFile) {
                            "Capture artifact is unavailable"
                        }
                        source.copyTo(destination, overwrite = false)
                        destination
                    }
                }
                main.post { completed(result) }
            }
        } catch (error: java.util.concurrent.RejectedExecutionException) {
            main.post { completed(Result.failure(error)) }
        }
    }

    internal fun start(
        origin: MobileCaptureOriginDto,
        platformId: String,
        advertisedServices: List<ByteArray>,
        advertisedName: String?,
        description: String?,
        connectionToken: MobileConnectionAttemptTokenDto?,
        evidence: String = "hardware_tested",
        transportOwnership: CaptureTransportOwnership = CaptureTransportOwnership.CAPTURE_OWNED,
    ): MobileCaptureGenerationDto? {
        if (writer != null) return null
        val owner = state()
        val generation = owner.beginConsentedCapture(origin) ?: run {
            update(lifecycle = owner.captureLifecycleSnapshot(), error = context.getString(R.string.capture_not_admitted))
            return null
        }
        try {
            val directory = File(context.filesDir, "DiagnosticCaptures").apply { mkdirs() }
            val prepared = prepareCaptureWriter(
                MobileCaptureStartRequestDto(
                    wallClockUnixSeconds = System.currentTimeMillis() / 1000.0,
                    platformId = platformId,
                    advertisedServices = advertisedServices,
                    directoryPath = directory.absolutePath,
                    filenameNonce = UUID.randomUUID().toString(),
                    source = "android-app",
                    reason = if (origin == MobileCaptureOriginDto.MANUAL) "record-only" else "protocol-detection",
                    evidence = evidence,
                    annotations = listOfNotNull(
                        if (description.isNullOrBlank()) null
                        else formatPevcapAnnotation("capture_description", description),
                        if (origin == MobileCaptureOriginDto.MANUAL) "route=record_only" else null,
                    ),
                    origin = origin,
                ),
                database(),
            )
            val started = prepared.start(SystemClock.elapsedRealtime().toULong())
            val writer = Writer(
                generation, started.builder, started.path, platformId, advertisedName,
                origin, description, connectionToken, transportOwnership,
            )
            if (!owner.captureWriterStarted(generation)) {
                started.builder.finishWriterAndPublishCapture(null)
                owner.captureWriterStartFailed(generation)
                update(lifecycle = owner.captureLifecycleSnapshot(), error = context.getString(R.string.capture_start_failed))
                return null
            }
            this.writer = writer
            connectionToken?.let(::updateIdentity)
            publish(presentation.copy(
                ready = true,
                lifecycle = owner.captureLifecycleSnapshot(),
                writerStatus = started.builder.writerStatus(),
                notificationCount = 0UL,
                fileName = File(started.path).name,
                deviceName = advertisedName,
                note = description,
                error = null,
            ))
            return generation
        } catch (error: Exception) {
            owner.captureWriterStartFailed(generation)
            Log.w(TAG, "Diagnostic capture writer could not start", error)
            update(lifecycle = owner.captureLifecycleSnapshot(), error = error.message ?: context.getString(R.string.capture_start_failed))
            return null
        }
    }

    internal fun updateIdentity(token: MobileConnectionAttemptTokenDto) {
        ownedWriter(token)?.let { active ->
            state().updateCaptureIdentity(token, active.builder)?.let { applyWriteOutcome(active, it) }
        }
    }

    internal fun hasWriter(token: MobileConnectionAttemptTokenDto): Boolean = writer?.connectionToken == token

    /** Immutable camera capture provenance for consumers on other owners. */
    fun activeCameraCaptureIdentity(): CameraCaptureIdentity? {
        val snapshot = presentation
        val attempt = snapshot.lifecycle.attempt ?: return null
        if (attempt.stage != MobileCaptureStageDto.RECORDING) return null
        val name = snapshot.fileName?.takeIf(String::isNotBlank) ?: return null
        return CameraCaptureIdentity(attempt.generation, name)
    }

    internal fun recordFingerprint(token: MobileConnectionAttemptTokenDto, fingerprint: MobileGattFingerprintDto) {
        ownedWriter(token)?.let { applyWriteOutcome(it, it.builder.addGattFingerprint(fingerprint)) }
    }

    internal fun linkUp(token: MobileConnectionAttemptTokenDto, maxWriteLen: Int?) {
        ownedWriter(token)?.let { active ->
            applyWriteOutcome(active, active.builder.recordLinkUp(
                SystemClock.elapsedRealtime().toULong().let(::MobileMonotonicMillisDto),
                maxWriteLen?.let { MobileTransportWriteLimitDto(it.toUShort()) },
            ))
        }
    }

    internal fun linkDown(token: MobileConnectionAttemptTokenDto) {
        ownedWriter(token)?.let { active ->
            applyWriteOutcome(active, active.builder.recordLinkDown(
                SystemClock.elapsedRealtime().toULong().let(::MobileMonotonicMillisDto),
            ))
        }
    }

    internal fun notification(
        token: MobileConnectionAttemptTokenDto,
        channel: ByteArray,
        service: ByteArray,
        bytes: ByteArray,
        semanticTelemetry: MobileTelemetrySnapshotDto?,
        evidence: MobileCaptureNotificationEvidenceDto,
        atMs: ULong,
    ) {
        ownedWriter(token)?.let { active ->
            val outcome = active.builder.recordDecodedNotification(
                MobileMonotonicMillisDto(atMs), channel, service, bytes.copyOf(), null, semanticTelemetry, evidence,
            )
            applyWriteOutcome(active, outcome)
            updateIdentity(token)
            if (outcome == MobileCaptureWriteOutcomeDto.Accepted) active.notificationCount++
            publish(presentation.copy(notificationCount = active.notificationCount))
        }
    }

    internal fun writeReceipt(
        token: MobileConnectionAttemptTokenDto,
        channel: ByteArray,
        bytes: ByteArray,
        writeId: ULong,
        disposition: MobilePevcapWriteDispositionDto,
        atMs: ULong,
    ) {
        ownedWriter(token)?.let { active ->
            applyWriteOutcome(active, active.builder.recordWriteWithoutResponseReceipt(
                MobileMonotonicMillisDto(atMs), channel, bytes.copyOf(), writeId, disposition,
            ))
        }
    }

    internal fun finishAfterLinkDown(token: MobileConnectionAttemptTokenDto, completed: () -> Unit = {}) {
        val active = writer?.takeIf { it.connectionToken == token }
        if (active == null) { completed(); return }
        finish(active, completed)
    }

    internal fun finishWithdrawn(generation: MobileCaptureGenerationDto) {
        val active = writer?.takeIf { it.generation == generation } ?: return
        finish(active)
    }

    internal fun beginManualFinish(completed: (Boolean) -> Unit) {
        if (!postOwner {
            val active = writer?.takeIf { it.origin == MobileCaptureOriginDto.MANUAL }
            if (active == null) { main.post { completed(false) }; return@postOwner }
            val token = state().beginCaptureFinish(active.generation)
            if (token == null) { main.post { completed(false) }; return@postOwner }
            update(lifecycle = state().captureLifecycleSnapshot(), error = null)
            try {
                enqueueIo {
                    val outcome = runCatching { active.builder.flushWriterOutcome() }
                    postOwner {
                        val receipt = outcome.getOrElse { MobileCaptureFlushOutcomeDto.Failed(it.message ?: "flush failed") }
                        state().applyCaptureFlushOutcome(active.generation, receipt)
                        val admitted = state().finishCaptureFlush(token, receipt)
                        if (admitted) {
                            onFinishManual(active.connectionToken, active.transportOwnership)
                            if (active.transportOwnership == CaptureTransportOwnership.BORROWED) finish(active)
                        }
                        else update(lifecycle = state().captureLifecycleSnapshot(), error = context.getString(R.string.capture_save_failed))
                        main.post { completed(admitted) }
                    }
                }
            } catch (error: java.util.concurrent.RejectedExecutionException) {
                state().applyCaptureFlushOutcome(active.generation, MobileCaptureFlushOutcomeDto.Rejected)
                state().finishCaptureFlush(token, MobileCaptureFlushOutcomeDto.Rejected)
                update(lifecycle = state().captureLifecycleSnapshot(), error = context.getString(R.string.capture_save_failed))
                main.post { completed(false) }
            }
        }) main.post { completed(false) }
    }

    private fun ownedWriter(token: MobileConnectionAttemptTokenDto): Writer? = writer?.takeIf {
        it.connectionToken == token && !it.finalizationStarted
    }

    private fun applyWriteOutcome(active: Writer, outcome: MobileCaptureWriteOutcomeDto) {
        active.priorOutcome = when (state().applyCaptureWriteOutcome(active.generation, outcome)) {
            MobileCaptureWriteDecisionDto.FAIL_WRITER -> MobileCaptureWriteOutcomeDto.Failed
            MobileCaptureWriteDecisionDto.STALE_FAILURE -> active.priorOutcome
            MobileCaptureWriteDecisionDto.ADMISSION_LOST -> outcome as? MobileCaptureWriteOutcomeDto.AdmissionLost
                ?: MobileCaptureWriteOutcomeDto.AdmissionLost(1uL)
            MobileCaptureWriteDecisionDto.REJECTED -> MobileCaptureWriteOutcomeDto.Rejected
            MobileCaptureWriteDecisionDto.CONTINUE -> active.priorOutcome
        }
        publish(presentation.copy(
            lifecycle = state().captureLifecycleSnapshot(),
            writerStatus = active.builder.writerStatus(),
        ))
    }

    private fun finish(active: Writer, completed: (() -> Unit)? = null) {
        if (writer !== active) { completed?.invoke(); return }
        if (active.finalizationStarted) { completed?.let(active.finishWaiters::add); return }
        active.finalizationStarted = true
        completed?.let(active.finishWaiters::add)
        state().retireCaptureWriter(active.generation)
        val publication = MobileCaptureHistoryPublicationDto(
            origin = active.origin,
            advertisedName = active.advertisedName,
            publishedAtUnixMs = MobileWallClockUnixMillisDto(System.currentTimeMillis().coerceAtLeast(0).toULong()),
        )
        try {
            enqueueIo(finalization = true) {
                val completion = try { active.builder.finishWriterAndPublishCapture(publication) }
                catch (error: Exception) {
                    Log.w(TAG, "Diagnostic capture finalization failed", error)
                    null
                }
                postOwner { completeFinishOnOwner(active, completion) }
            }
        } catch (error: java.util.concurrent.RejectedExecutionException) {
            // The queue reserves a slot for this operation; rejection means another task won
            // a race for that slot. Keep every completion waiter and retry after queue pressure.
            active.finalizationStarted = false
            Log.w(TAG, "Diagnostic capture finalization queue is unavailable", error)
            update(error = context.getString(R.string.capture_save_failed))
            if (!io.isShutdown) main.postDelayed({
                postOwner { if (writer === active && !active.finalizationStarted) finish(active) }
            }, 100L)
        }
    }

    private fun completeFinishOnOwner(active: Writer, completion: MobileCaptureCompletionDto?) {
        if (writer !== active) return
        if (completion == null) {
            state().captureWriterFailed(active.generation)
            writer = null
            update(lifecycle = state().captureLifecycleSnapshot(), error = context.getString(R.string.capture_save_failed))
            active.finishWaiters.toList().forEach { it() }
            active.finishWaiters.clear()
            return
        }
        val decision = state().completeCaptureWriter(active.generation, completion, active.priorOutcome)
        writer = null
        val sessionArtifact = (decision as? MobileCaptureCompletionDecisionDto.PublishArtifact)?.artifact?.let { artifact ->
            val file = File(artifact.path).canonicalFile
            if (file.parentFile == File(context.filesDir, "DiagnosticCaptures").canonicalFile && file.isFile) {
                DiagnosticCaptureArtifact(
                    id = artifact.id.toString(), file = file, origin = active.origin,
                    deviceName = active.advertisedName, description = active.description,
                    status = artifact.status,
                    notificationCount = active.notificationCount,
                    error = if (decision.databasePublicationFailed) context.getString(R.string.capture_database_publish_failed) else null,
                )
            } else null
        }
        val error = when (decision) {
            MobileCaptureCompletionDecisionDto.PublishFailure -> context.getString(R.string.capture_save_failed)
            is MobileCaptureCompletionDecisionDto.PublishArtifact -> if (decision.databasePublicationFailed)
                context.getString(R.string.capture_save_failed) else null
            is MobileCaptureCompletionDecisionDto.PublishDatabase -> if (decision.databasePublicationFailed)
                context.getString(R.string.capture_save_failed) else null
            MobileCaptureCompletionDecisionDto.Rejected -> context.getString(R.string.capture_stale_completion)
        }
        update(lifecycle = state().captureLifecycleSnapshot(), writerStatus = null, fileName = null,
            notificationCount = 0UL, deviceName = null, note = null, error = error,
            sessionArtifacts = if (sessionArtifact == null) presentation.sessionArtifacts else presentation.sessionArtifacts + sessionArtifact)
        refreshHistory()
        active.finishWaiters.toList().forEach { it() }
        active.finishWaiters.clear()
    }

    private fun update(
        lifecycle: MobileCaptureLifecycleSnapshotDto = presentation.lifecycle,
        error: String? = presentation.error,
        writerStatus: MobileCaptureWriterStatusDto? = presentation.writerStatus,
        notificationCount: ULong = presentation.notificationCount,
        fileName: String? = presentation.fileName,
        deviceName: String? = presentation.deviceName,
        note: String? = presentation.note,
        sessionArtifacts: List<DiagnosticCaptureArtifact> = presentation.sessionArtifacts,
    ) = publish(presentation.copy(
        lifecycle = lifecycle, error = error, writerStatus = writerStatus, notificationCount = notificationCount,
        fileName = fileName, deviceName = deviceName, note = note,
        sessionArtifacts = sessionArtifacts,
    ))

    private fun publish(next: DiagnosticCapturePresentation) {
        presentation = next
        main.post { observers.forEach { observer -> runCatching { observer(next) } } }
    }

    /** Must be called on the BLE owner. Completion runs there after queued capture I/O drains. */
    fun close(completed: () -> Unit = {}) {
        if (closeComplete) { completed(); return }
        closeWaiters += completed
        synchronized(ioSubmissionLock) { closing = true }
        if (closeBarrierQueued) return
        val active = writer
        if (active == null) queueCloseBarrier() else finish(active) { queueCloseBarrier() }
    }

    private fun queueCloseBarrier() {
        if (closeComplete || closeBarrierQueued) return
        closeBarrierQueued = true
        try {
            enqueueIo(finalization = true) {
                postOwner {
                    io.shutdown()
                    closeBarrierQueued = false
                    closeComplete = true
                    val callbacks = closeWaiters.toList()
                    closeWaiters.clear()
                    callbacks.forEach { callback ->
                        try { callback() } catch (error: Exception) {
                            Log.w(TAG, "Capture close callback failed", error)
                        }
                    }
                }
            }
        } catch (error: java.util.concurrent.RejectedExecutionException) {
            closeBarrierQueued = false
            Log.w(TAG, "Capture I/O drain barrier could not be queued", error)
            if (!io.isShutdown) main.postDelayed({ postOwner { queueCloseBarrier() } }, 100L)
        }
    }

    private fun enqueueIo(finalization: Boolean = false, action: () -> Unit) {
        synchronized(ioSubmissionLock) {
            if (closing && !finalization) {
                throw java.util.concurrent.RejectedExecutionException("Capture client is closing")
            }
            if (!finalization && io.queue.size >= 15) {
                throw java.util.concurrent.RejectedExecutionException("Capture work queue is reserved for writer finalization")
            }
            io.execute(action)
        }
    }

    /** A receipt may remove only the immutable submission payload; that payload cannot be shared after cleanup starts. */
    private fun requireLocalPayloadRetained(
        database: RideDatabaseHandle,
        source: MobileCaptureSubmissionSourceDto,
    ) {
        val status = kotlinx.coroutines.runBlocking {
            CaptureSubmissionClientHandle(database).use { it.status(source) }
        }
        check(localCapturePayloadMayBeShared(status?.localPayloadState)) {
            "The local capture payload is being removed or has already been removed. Receipt and server-delete details remain available."
        }
    }

    private companion object { const val TAG = "CutoutCapture" }
}
