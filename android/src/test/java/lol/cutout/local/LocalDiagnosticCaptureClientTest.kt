package lol.cutout.local

import androidx.test.core.app.ApplicationProvider
import android.content.Context
import lol.cutout.AndroidDiagnosticCaptureClient
import lol.cutout.localCapturePayloadMayBeShared
import lol.cutout.CutoutApplication
import lol.cutout.R
import org.junit.Test
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
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
    fun onlyRetainedOrUnsubmittedLocalPayloadCanBeShared() {
        assertTrue(localCapturePayloadMayBeShared(null))
        assertTrue(localCapturePayloadMayBeShared(MobileCaptureSubmissionLocalPayloadStateDto.RETAINED))
        assertFalse(localCapturePayloadMayBeShared(MobileCaptureSubmissionLocalPayloadStateDto.REMOVAL_PENDING))
        assertFalse(localCapturePayloadMayBeShared(MobileCaptureSubmissionLocalPayloadStateDto.REMOVED))
    }

    @Test
    fun recordOnlyWriterUsesRustAdmissionThenPublishesAndExportsPrivateHistory() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val application = context as CutoutApplication
        val database = await { done -> application.rides.withDatabase({ it }, done) }.getOrThrow()
        val state = CutoutSessionStateHandle.withDatabase(database)
        val platformId = "capture-test-${UUID.randomUUID()}"
        val client = AndroidDiagnosticCaptureClient(
            context, { state }, { database }, { action -> action(); true }, { _, _ -> },
        )
        var exported: File? = null
        try {
            val token = checkNotNull(state.beginConnectionAttempt(platformId, 1UL).token)
            checkNotNull(client.start(
                MobileCaptureOriginDto.MANUAL, platformId, emptyList(), "Test device", "read-only test", token,
                evidence = "unverified",
            ))
            check(state.captureLifecycleSnapshot().attempt?.stage == MobileCaptureStageDto.RECORDING)
            check(client.activeCameraCaptureIdentity()?.generation == state.captureLifecycleSnapshot().attempt?.generation)
            client.linkUp(token, 20)
            client.notification(
                token, ByteArray(16) { it.toByte() }, ByteArray(16) { (it + 1).toByte() }, byteArrayOf(5, 6), null,
                MobileCaptureNotificationEvidenceDto.UNCLASSIFIED, android.os.SystemClock.elapsedRealtime().toULong(),
            )

            check(client.current().notificationCount == 1UL) { "The fixture notification was not admitted" }

            val admitted = await { done -> client.beginManualFinish { done(Result.success(it)) } }.getOrThrow()
            check(admitted) { "Rust refused a durable manual finish flush" }
            client.linkDown(token)
            await { done -> client.finishAfterLinkDown(token) { done(Result.success(Unit)) } }.getOrThrow()
            check(state.captureLifecycleSnapshot().attempt?.stage == MobileCaptureStageDto.SAVED)
            check(client.activeCameraCaptureIdentity() == null)

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
            check(capture.eventCount == 3UL) {
                "expected link-up, notification, and link-down events; got ${capture.eventCount}"
            }

            exported = await { done -> client.export(capture.liveCaptureId, done) }.getOrThrow()
            check(exported?.parentFile?.canonicalFile == File(context.cacheDir, "diagnostic-exports").canonicalFile)
            check(exported?.isFile == true && exported!!.length() > 0L)
        } finally {
            exported?.delete()
            try { await { done -> client.close { done(Result.success(Unit)) } } }
            finally { state.close() }
            // The app-scoped RideMapRuntime and its single Rust-owned database remain open.
        }
    }

    @Test
    fun manualVescCaptureUsesVerifiedOwnerBeforeTelemetryAndWhenWriterStartsLater() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val application = context as CutoutApplication
        val database = await { done -> application.rides.withDatabase({ it }, done) }.getOrThrow()
        val reply = byteArrayOf(
            2, 20, 157.toByte(), 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
            38, 208.toByte(), 3,
        )
        for (identifyBeforeStart in listOf(false, true)) {
            val state = CutoutSessionStateHandle.withDatabase(database)
            val platformId = "capture-vesc-${UUID.randomUUID()}"
            val client = AndroidDiagnosticCaptureClient(context, { state }, { database }, { it(); true }, { _, _ -> })
            var exported: File? = null
            try {
                val token = checkNotNull(state.beginConnectionAttempt(platformId, android.os.SystemClock.elapsedRealtime().toULong()).token)
                state.connectionLinkEstablished(token)
                val session = lol.cutout.PevSession(state, token, { android.os.SystemClock.elapsedRealtime().toULong() }, {},
                    onCaptureNotification = { channel, service, bytes, semantic, evidence, atMs ->
                        client.notification(token, channel, service, bytes, semantic, evidence, atMs)
                    },
                ) { _, _, _ -> client.updateIdentity(token) }
                if (identifyBeforeStart) session.notification(ByteArray(16), ByteArray(16), reply)
                checkNotNull(client.start(
                    MobileCaptureOriginDto.MANUAL, platformId, emptyList(), "VESC", null, token,
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
            val client = AndroidDiagnosticCaptureClient(context, { state }, { database }, { it(); true }, { _, _ -> })
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
        val priorConsent = state.automaticDiagnosticRecordingEnabled()
        val client = AndroidDiagnosticCaptureClient(context, { state }, { database }, { it(); true }, { _, _ -> })
        try {
            for (legacyConsent in listOf(false, true)) {
                state.setAutomaticDiagnosticRecordingEnabled(legacyConsent)
                check(client.start(
                    MobileCaptureOriginDto.AUTOMATIC,
                    "capture-test-legacy-consent-$legacyConsent",
                    emptyList(), null, null, null,
                ) == null) { "Automatic diagnostic capture must remain refused with legacy consent=$legacyConsent" }
                check(state.captureLifecycleSnapshot().attempt == null)
                check(client.current().error == context.getString(R.string.capture_not_admitted))
            }
        } finally {
            try { await { done -> client.close { done(Result.success(Unit)) } } }
            finally {
                state.setAutomaticDiagnosticRecordingEnabled(priorConsent)
                state.close()
            }
            // Do not close or delete the shared app-owned database.
        }
    }

    @Test
    fun borrowedManualCaptureFinishKeepsVerifiedConnectionTokenAndTransport() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val application = context as CutoutApplication
        val database = await { done -> application.rides.withDatabase({ it }, done) }.getOrThrow()
        val state = CutoutSessionStateHandle.withDatabase(database)
        val platformId = "capture-borrowed-${UUID.randomUUID()}"
        var finishOwnership: lol.cutout.CaptureTransportOwnership? = null
        val client = AndroidDiagnosticCaptureClient(
            context, { state }, { database }, { action -> action(); true }, { token, ownership ->
                check(token == state.connectionAttemptSnapshot().token)
                finishOwnership = ownership
            },
        )
        try {
            val token = checkNotNull(state.beginConnectionAttempt(platformId, android.os.SystemClock.elapsedRealtime().toULong()).token)
            state.connectionLinkEstablished(token)
            val reply = byteArrayOf(
                2, 20, 157.toByte(), 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115, 104, 0,
                38, 208.toByte(), 3,
            )
            val session = lol.cutout.PevSession(
                state, token, { android.os.SystemClock.elapsedRealtime().toULong() }, {},
            ) { _, _, _ -> client.updateIdentity(token) }
            session.notification(ByteArray(16), ByteArray(16), reply)
            check(state.deviceSessionSnapshot().connection.readiness == MobileConnectionReadinessDto.VERIFIED)

            checkNotNull(client.start(
                MobileCaptureOriginDto.MANUAL, platformId, emptyList(), "VESC", "borrowed test", token,
                transportOwnership = lol.cutout.CaptureTransportOwnership.BORROWED,
            ))
            client.linkUp(token, 20)
            val admitted = await { done -> client.beginManualFinish { done(Result.success(it)) } }.getOrThrow()
            check(admitted)
            check(finishOwnership == lol.cutout.CaptureTransportOwnership.BORROWED)
            check(state.connectionAttemptSnapshot().token == token)
            check(state.connectionAttemptSnapshot().transport == MobileConnectionTransportStateDto.CONNECTED)
            check(state.deviceSessionSnapshot().connection.readiness == MobileConnectionReadinessDto.VERIFIED)
        } finally {
            try { await { done -> client.close { done(Result.success(Unit)) } } }
            finally { state.close() }
        }
    }

    private val historyLoaded = CountDownLatch(1)

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
