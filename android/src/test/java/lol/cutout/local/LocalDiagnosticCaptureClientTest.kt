package lol.cutout.local

import androidx.test.core.app.ApplicationProvider
import android.content.Context
import lol.cutout.AndroidDiagnosticCaptureClient
import lol.cutout.CutoutApplication
import lol.cutout.R
import lol.cutout.AndroidCaptureSubmissionClient
import lol.cutout.CaptureSubmissionPresentation
import lol.cutout.CaptureSubmissionController
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.*
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import java.util.UUID

/** Local-only capture lifecycle coverage; device runs must use an isolated target database. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalDiagnosticCaptureClientTest {
    @Test
    fun recordOnlyWriterUsesRustAdmissionThenPublishesAndExportsPrivateHistory() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val application = context as CutoutApplication
        val database = await { done -> application.rides.withDatabase({ it }, done) }.getOrThrow()
        val state = CutoutSessionStateHandle.withDatabase(database)
        val platformId = "capture-test-${UUID.randomUUID()}"
        val client = AndroidDiagnosticCaptureClient(
            context, { state }, { database }, { action -> action(); true }, {},
        )
        var exported: File? = null
        try {
            val token = checkNotNull(state.beginConnectionAttempt(platformId, 1UL).token)
            val startWindow = android.os.SystemClock.elapsedRealtime().toULong()
            val generation = checkNotNull(client.start(
                MobileCaptureOriginDto.MANUAL, platformId, emptyList(), "Test device", "read-only test", token,
                evidence = "unverified",
            ))
            val startedElapsedMillis = checkNotNull(client.current().startedElapsedMillis)
            check(startedElapsedMillis >= startWindow && startedElapsedMillis <= android.os.SystemClock.elapsedRealtime().toULong())
            check(client.current().finishedElapsedMillis == null)
            // Swift presents the capture description as deviceKind; it is annotation text, not a type claim.
            check(client.current().deviceKind == "read-only test")
            check(state.captureLifecycleSnapshot().attempt?.stage == MobileCaptureStageDto.RECORDING)
            check(client.activeCameraCaptureIdentity()?.generation == state.captureLifecycleSnapshot().attempt?.generation)
            client.linkUp(token, 20)
            client.notification(
                token, ByteArray(16) { it.toByte() }, ByteArray(16) { (it + 1).toByte() }, byteArrayOf(5, 6), null,
                MobileCaptureNotificationEvidenceDto.UNCLASSIFIED, android.os.SystemClock.elapsedRealtime().toULong(),
            )

            check(client.current().notificationCount == 1UL) { "The fixture notification was not admitted" }

            val catalog = checkNotNull(captureResearchCatalog("nosfet_aero"))
            val draft = MobileCaptureSettingsDraftDto(
                MobileCaptureResearchProfileDto(catalog.profileId, catalog.pevCategory, catalog.model, null,
                    MobileCaptureResearchProfileSelectionDto.MANUAL),
                0UL, MobileCaptureObservationSourceDto.WHEEL_DISPLAY,
                listOf(MobileCaptureSettingObservationValueDto("displayed_pack_voltage",
                    MobileCaptureObservedValueDto.Entered("124.7"), "V", null)), "independent display reading",
            )
            val savedSettings = await<MobileCaptureSettingsObservationDto> { done ->
                client.recordSettings(generation, draft, done)
            }.getOrThrow()
            check(client.current().settingsObservations == listOf(savedSettings))
            check(savedSettings.observedAtMs == 0UL)
            val activeSettings = await<List<MobileCaptureSettingsObservationDto>> { done -> client.activeSettings(generation, done) }.getOrThrow()
            check(activeSettings == listOf(savedSettings))
            check(client.current().settingsObservations == activeSettings) {
                "A successful active observation read must publish the loaded list for the capture summary"
            }
            val requests = listOf(MobileCaptureResearchReadbackRequestDto(catalog.profileId, 0UL))
            val activeReadback = await<List<MobileCaptureResearchReadbackDto>> { done ->
                client.activeSettingsReadback(generation, requests, done)
            }.getOrThrow()
            check(activeReadback.single().values.isEmpty()) { "Unrelated packet bytes must not invent decoded settings" }
            val staleSettings = await<MobileCaptureSettingsObservationDto> { done ->
                client.recordSettings(MobileCaptureGenerationDto(generation.value + 1UL), draft, done)
            }
            check(staleSettings.exceptionOrNull() is MobileCaptureResearchException.NotRecording)
            val invalidSettings = await<MobileCaptureSettingsObservationDto> { done ->
                client.recordSettings(generation, draft.copy(values = listOf(draft.values.single().copy(settingId = "not-a-field"))), done)
            }
            check(invalidSettings.exceptionOrNull() is MobileCaptureResearchException.InvalidObservation)
            check(client.current().settingsObservations == activeSettings) { "Rejected drafts must not appear saved" }

            val ride = await<List<MobileCaptureLabelDto>> { done ->
                client.changeLabel(generation, MobileCaptureLabelActionDto.Start(MobileCaptureLabelDto.RIDE), done)
            }.getOrThrow()
            check(ride == listOf(MobileCaptureLabelDto.RIDE))
            val capacity = await<List<MobileCaptureLabelDto>> { done ->
                client.changeLabel(generation, MobileCaptureLabelActionDto.Start(MobileCaptureLabelDto.CHARGING), done)
            }
            check(capacity.exceptionOrNull() is MobileCaptureAnnotationException.CapacityReached) {
                "The required header and description leave no room for a second interval and its closing boundary"
            }
            check(client.current().activeLabels == ride) { "Capacity rejection must retain the admitted interval" }
            val stale = await<List<MobileCaptureLabelDto>> { done ->
                client.changeLabel(MobileCaptureGenerationDto(generation.value + 1UL),
                    MobileCaptureLabelActionDto.Stop(MobileCaptureLabelDto.RIDE), done)
            }
            check(stale.exceptionOrNull() is MobileCaptureAnnotationException.NotRecording)
            check(client.current().activeLabels == ride) { "A stale UI action must not alter the recording" }

            val admitted = await { done -> client.beginManualFinish { done(Result.success(it)) } }.getOrThrow()
            check(admitted) { "Rust refused a durable manual finish flush" }
            client.linkDown(token)
            val finishTimestamp = AtomicReference<ULong?>()
            val finishPublished = CountDownLatch(1)
            val finishObserver = client.observe { snapshot ->
                snapshot.finishedElapsedMillis?.let {
                    check(snapshot.lifecycle.attempt?.stage == MobileCaptureStageDto.FINALIZING) {
                        "The admitted end timestamp must carry the Rust finalizing stage"
                    }
                    finishTimestamp.set(it)
                    finishPublished.countDown()
                }
            }
            await { done -> client.finishAfterLinkDown(token) { done(Result.success(Unit)) } }.getOrThrow()
            check(awaitCallback(finishPublished, 10, TimeUnit.SECONDS)) { "finish admission timestamp was not published" }
            finishObserver.close()
            val finishedElapsedMillis = checkNotNull(finishTimestamp.get())
            check(finishedElapsedMillis >= startedElapsedMillis)
            check(state.captureLifecycleSnapshot().attempt?.stage == MobileCaptureStageDto.SAVED)
            check(client.activeCameraCaptureIdentity() == null)
            check(client.current().startedElapsedMillis == null && client.current().finishedElapsedMillis == null) {
                "Completion must clear active-capture monotonic metadata"
            }

            val observer = client.observe { snapshot ->
                if (!snapshot.loading && snapshot.captures.any {
                        it.recording?.platformIdentifier == platformId
                    }) historyLoaded.countDown()
            }
            client.refreshHistory()
            check(awaitCallback(historyLoaded, 10, TimeUnit.SECONDS)) { "saved Rust history did not arrive" }
            observer.close()
            val capture = checkNotNull(client.current().captures.firstOrNull {
                it.recording?.platformIdentifier == platformId
            })
            check(capture.eventCount == 4UL) {
                "expected link-up, notification, settings observation, and link-down events; got ${capture.eventCount}"
            }

            val savedSource = MobileCaptureSubmissionSourceDto.LiveCapture(capture.liveCaptureId)
            val reopenedSettings = await<List<MobileCaptureSettingsObservationDto>> { done -> client.savedSettings(savedSource, done) }.getOrThrow()
            check(reopenedSettings == activeSettings) { "Settings must survive durable capture publication and database readback" }
            val savedReadback = await<List<MobileCaptureResearchReadbackDto>> { done ->
                client.savedSettingsReadback(savedSource, requests, done)
            }.getOrThrow()
            check(savedReadback == activeReadback)

            // Freeze and cancel through the real production submission boundary, entirely locally.
            val nativeSubmission = CaptureSubmissionClientHandle(database)
            val submissionOwner = AndroidCaptureSubmissionClient({ nativeSubmission }, { allowRegistration ->
                check(!allowRegistration) { "Reading/preparing/cancelling must not register an account" }
                null
            }, AndroidCaptureSubmissionClient::errorKey)
            val submission = submissionOwner.controller(savedSource)
            val subscription = submission.observe {}
            try {
                submission.prepare()
                awaitSubmission(submission) { !it.working && it.status?.phase == MobileCaptureSubmissionPhaseDto.PREPARED }
                val operation = checkNotNull(submission.current().status).operationId
                check(submission.current().status!!.categories.settingsObservations)
                submission.cancel()
                awaitSubmission(submission) { !it.working && !it.cancelling && it.status?.phase == MobileCaptureSubmissionPhaseDto.CANCELLED }
                check(submission.current().status?.operationId == operation)
                val reopened = CaptureSubmissionClientHandle(database)
                try {
                    val persisted = kotlinx.coroutines.runBlocking { reopened.status(savedSource) }
                    check(persisted?.phase == MobileCaptureSubmissionPhaseDto.CANCELLED)
                    check(persisted?.operationId == operation)
                } finally { reopened.close() }
            } finally { subscription.close(); submissionOwner.close(); nativeSubmission.close() }

            exported = await { done -> client.export(capture.liveCaptureId, done) }.getOrThrow()
            check(exported?.parentFile?.canonicalFile == File(context.cacheDir, "diagnostic-exports").canonicalFile)
            check(exported?.isFile == true && exported!!.length() > 0L)
            val text = exported!!.readText()
            val slug = captureLabelSlug(MobileCaptureLabelDto.RIDE)
            check(text.contains("capture_label=${slug}_start")) { "Start marker must survive durable export" }
            check(text.contains("capture_label=${slug}_stop")) { "Finishing must close every active interval" }
            check(client.current().activeLabels.isEmpty())
            check(client.current().settingsObservations.isEmpty())
            val nextGeneration = checkNotNull(client.start(
                MobileCaptureOriginDto.MANUAL, "$platformId-next", emptyList(), "Next device", null, token,
                evidence = "unverified",
            ))
            check(nextGeneration != generation)
            check(client.current().startedElapsedMillis != null && client.current().finishedElapsedMillis == null)
            check(client.current().deviceKind == null) { "A new identity must not inherit capture presentation metadata" }
            val previousCaptureAction = await<List<MobileCaptureLabelDto>> { done ->
                client.changeLabel(generation, MobileCaptureLabelActionDto.Start(MobileCaptureLabelDto.RIDE), done)
            }
            check(previousCaptureAction.exceptionOrNull() is MobileCaptureAnnotationException.NotRecording)
            check(client.current().activeLabels.isEmpty()) { "Old controls must not annotate a replacement writer" }
            val oldSettings = await<MobileCaptureSettingsObservationDto> { done -> client.recordSettings(generation, draft, done) }
            check(oldSettings.exceptionOrNull() is MobileCaptureResearchException.NotRecording)
            check(client.current().settingsObservations.isEmpty()) { "Old worksheets must not write to the replacement capture" }

        } finally {
            exported?.delete()
            try { await { done -> client.close { done(Result.success(Unit)) } } }
            finally { state.close() }
            // The app-scoped RideMapRuntime and its single Rust-owned database remain open.
        }
    }

    @Test
    fun automaticVescCaptureUsesVerifiedOwnerBeforeTelemetryAndWhenWriterStartsLater() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val application = context as CutoutApplication
        val database = await { done -> application.rides.withDatabase({ it }, done) }.getOrThrow()
        val reply = byteArrayOf(
            2, 20, 157.toByte(), 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
            38, 208.toByte(), 3,
        )
        for (identifyBeforeStart in listOf(false, true)) {
            val state = CutoutSessionStateHandle.withDatabase(database)
            val priorConsent = state.automaticDiagnosticRecordingEnabled()
            val platformId = "capture-vesc-${UUID.randomUUID()}"
            val client = AndroidDiagnosticCaptureClient(context, { state }, { database }, { it(); true }, {})
            var exported: File? = null
            try {
                state.setAutomaticDiagnosticRecordingEnabled(true)
                val token = checkNotNull(state.beginConnectionAttempt(platformId, android.os.SystemClock.elapsedRealtime().toULong()).token)
                state.connectionLinkEstablished(token)
                val session = lol.cutout.PevSession(state, token, { android.os.SystemClock.elapsedRealtime().toULong() }, {},
                    onCaptureNotification = { channel, service, bytes, semantic, evidence, atMs ->
                        client.notification(token, channel, service, bytes, semantic, evidence, atMs)
                    },
                ) { _, _, _ -> client.updateIdentity(token) }
                if (identifyBeforeStart) session.notification(ByteArray(16), ByteArray(16), reply)
                checkNotNull(client.start(
                    MobileCaptureOriginDto.AUTOMATIC, platformId, emptyList(), "VESC", null, token,
                ))
                // The pre-start case verifies the shared identity without requiring another telemetry frame.
                if (!identifyBeforeStart) session.notification(ByteArray(16), ByteArray(16), reply)
                check(state.deviceSessionSnapshot().connection.readiness == MobileConnectionReadinessDto.VERIFIED)
                await { done -> client.finishAfterLinkDown(token) { done(Result.success(Unit)) } }.getOrThrow()
                val loaded = CountDownLatch(1)
                val observer = client.observe { snapshot ->
                    if (!snapshot.loading && snapshot.captures.any { it.recording?.platformIdentifier == platformId })
                        loaded.countDown()
                }
                try {
                    client.refreshHistory()
                    check(awaitCallback(loaded, 10, TimeUnit.SECONDS)) { "VESC capture history did not arrive" }
                } finally { observer.close() }
                val capture = checkNotNull(client.current().captures.firstOrNull {
                    it.recording?.platformIdentifier == platformId
                })
                exported = await { done -> client.export(capture.liveCaptureId, done) }.getOrThrow()
                val objects = exported!!.readLines().map { org.json.JSONObject(it) }
                val identity = objects.first().getJSONObject("header").getJSONObject("resolved_identity")
                check(identity.getString("protocol_family") == "Vesc")
                check(identity.isNull("model") && identity.isNull("firmware"))
                check(identifyBeforeStart || objects.any { entry ->
                    val bytes = entry.optJSONObject("record")?.optJSONArray("bytes")
                    bytes != null && bytes.length() == reply.size && reply.indices.all { index ->
                        bytes.getInt(index) == (reply[index].toInt() and 255)
                    }
                }) { "The VESC wire notification must remain unchanged" }
            } finally {
                exported?.delete()
                try { await { done -> client.close { done(Result.success(Unit)) } } }
                finally {
                    state.setAutomaticDiagnosticRecordingEnabled(priorConsent)
                    state.close()
                }
            }
        }
    }

    @Test
    fun acceptedVescConflictClearsExportedIdentityAndRetainsTheInvalidatingNotification() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val application = context as CutoutApplication
        val database = await { done -> application.rides.withDatabase({ it }, done) }.getOrThrow()
        val reply = byteArrayOf(
            2, 20, 157.toByte(), 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101,
            114, 104, 97, 115, 104, 0, 38, 208.toByte(), 3,
        )
        val conflict = ByteArray(24).apply {
            this[0] = 0x55; this[1] = 0xaa.toByte()
            byteArrayOf(0, 0x18, 0x5a, 0x5a, 0x5a, 0x5a).copyInto(this, 18)
        }
        val channel = byteArrayOf(0x6e, 0x40, 0, 3, 0xb5.toByte(), 0xa3.toByte(), 0xf3.toByte(),
            0x93.toByte(), 0xe0.toByte(), 0xa9.toByte(), 0xe5.toByte(), 0x0e, 0x24, 0xdc.toByte(), 0xca.toByte(), 0x9e.toByte())
        val service = channel.copyOf().apply { this[3] = 1 }
        for (invalidatesIdentity in listOf(false, true)) {
            val state = CutoutSessionStateHandle.withDatabase(database)
            val platformId = "capture-conflict-${UUID.randomUUID()}"
            val client = AndroidDiagnosticCaptureClient(context, { state }, { database }, { it(); true }, {})
            var exported: File? = null
            try {
                var at = android.os.SystemClock.elapsedRealtime().toULong()
                val stale = checkNotNull(state.beginConnectionAttempt("old-$platformId", at).token)
                val token = checkNotNull(state.beginConnectionAttempt(platformId, at).token)
                state.connectionLinkEstablished(token)
                fun session(attempt: MobileConnectionAttemptTokenDto) = lol.cutout.PevSession(
                    state, attempt, { at }, {},
                    onCaptureNotification = { wireChannel, wireService, bytes, semantic, evidence, atMs ->
                        client.notification(attempt, wireChannel, wireService, bytes, semantic, evidence, atMs)
                    },
                ) { _, _, _ -> client.updateIdentity(attempt) }
                val current = session(token)
                val startedAt = android.os.SystemClock.elapsedRealtime().toULong()
                checkNotNull(client.start(
                    MobileCaptureOriginDto.MANUAL, platformId, emptyList(), "VESC", null, token,
                ))
                check(android.os.SystemClock.elapsedRealtime().toULong() == startedAt) {
                    "The paused fixture clock must stay fixed during synchronous writer startup"
                }
                session(stale).notification(channel, service, conflict)
                check(client.current().notificationCount == 0UL)
                at = startedAt + 10UL
                current.notification(channel, service, reply)
                check(state.deviceSessionSnapshot().connection.readiness == MobileConnectionReadinessDto.VERIFIED)
                if (invalidatesIdentity) {
                    at = startedAt + 20UL
                    current.notification(channel, service, conflict)
                    check(state.deviceSessionSnapshot().connection.readiness == MobileConnectionReadinessDto.CONFLICTED)
                    check(state.deviceSessionSnapshot().connection.token == null)
                    current.notification(channel, service, byteArrayOf(99))
                    check(client.current().notificationCount == 2UL)
                } else {
                    state.connectionLinkDown(token)
                }
                await { done -> client.finishAfterLinkDown(token) { done(Result.success(Unit)) } }.getOrThrow()
                val loaded = CountDownLatch(1)
                val observer = client.observe { snapshot ->
                    if (!snapshot.loading && snapshot.captures.any { it.recording?.platformIdentifier == platformId })
                        loaded.countDown()
                }
                try {
                    client.refreshHistory()
                    check(awaitCallback(loaded, 10, TimeUnit.SECONDS)) { "Conflict capture history did not arrive" }
                } finally { observer.close() }
                val capture = checkNotNull(client.current().captures.firstOrNull {
                    it.recording?.platformIdentifier == platformId
                })
                exported = await { done -> client.export(capture.liveCaptureId, done) }.getOrThrow()
                val objects = exported!!.readLines().map { org.json.JSONObject(it) }
                val header = objects.first().getJSONObject("header")
                val identity = header.getJSONObject("resolved_identity")
                if (invalidatesIdentity) {
                    check(identity.isNull("protocol_family") && identity.isNull("model") && identity.isNull("firmware"))
                } else {
                    check(identity.getString("protocol_family") == "Vesc") { "Ordinary disconnect retains verified proof" }
                }
                val annotations = header.getJSONArray("annotations")
                check((0 until annotations.length()).any { annotations.getString(it) == "route=record_only" })
                check((0 until annotations.length()).any { annotations.getString(it).startsWith("resolved_evidence=") })
                fun matches(record: org.json.JSONObject, bytes: ByteArray): Boolean {
                    val raw = record.optJSONArray("bytes") ?: return false
                    return raw.length() == bytes.size && bytes.indices.all { raw.getInt(it) == (bytes[it].toInt() and 255) }
                }
                val records = objects.mapNotNull { it.optJSONObject("record") }
                check(records.count { matches(it, reply) } == 1)
                val conflicts = records.filter { matches(it, conflict) }
                check(conflicts.size == if (invalidatesIdentity) 1 else 0)
                if (invalidatesIdentity) {
                    val record = conflicts.single()
                    check(record.getLong("monotonic_ms") == 20L)
                    for ((key, expected) in listOf("characteristic" to channel, "service" to service)) {
                        val actual = record.getJSONArray(key)
                        check(actual.length() == expected.size && expected.indices.all {
                            actual.getInt(it) == (expected[it].toInt() and 255)
                        })
                    }
                    check(record.isNull("telemetry") && record.isNull("semantic_telemetry"))
                }
            } finally {
                exported?.delete()
                try { await { done -> client.close { done(Result.success(Unit)) } } }
                finally { state.close() }
            }
        }
    }

    @Test
    fun automaticCaptureStartRemainsRefusedWithoutRustConsent() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val application = context as CutoutApplication
        val database = await { done -> application.rides.withDatabase({ it }, done) }.getOrThrow()
        val state = CutoutSessionStateHandle.withDatabase(database)
        val client = AndroidDiagnosticCaptureClient(context, { state }, { database }, { it(); true }, {})
        try {
            check(client.start(
                MobileCaptureOriginDto.AUTOMATIC, "capture-test-no-consent", emptyList(), null, null, null,
            ) == null)
            check(state.captureLifecycleSnapshot().attempt == null)
            check(client.current().error == context.getString(R.string.capture_not_admitted))
        } finally {
            try { await { done -> client.close { done(Result.success(Unit)) } } }
            finally { state.close() }
            // Do not close or delete the shared app-owned database.
        }
    }

    private val historyLoaded = CountDownLatch(1)

    private fun awaitSubmission(model: CaptureSubmissionController, condition: (CaptureSubmissionPresentation) -> Boolean) {
        val ready = CountDownLatch(1)
        val observer = model.observe { if (condition(it)) ready.countDown() }
        try { check(awaitCallback(ready, 15, TimeUnit.SECONDS)) { "Submission state did not arrive: ${model.current()}" } }
        finally { observer.close() }
    }

    private fun <T> await(start: ((Result<T>) -> Unit) -> Unit): Result<T> {
        val latch = CountDownLatch(1)
        val result = AtomicReference<Result<T>?>()
        start { value -> result.set(value); latch.countDown() }
        check(awaitCallback(latch, 15, TimeUnit.SECONDS)) { "Capture callback did not complete" }
        return checkNotNull(result.get())
    }

    private fun awaitCallback(latch: CountDownLatch, timeout: Long, unit: TimeUnit): Boolean =
        awaitWithMainLooper(latch, timeout, unit)
}
