package lol.cutout.local

import androidx.test.core.app.ApplicationProvider
import android.content.Context
import lol.cutout.AndroidDiagnosticCaptureClient
import lol.cutout.CutoutApplication
import lol.cutout.R
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
            val generation = checkNotNull(client.start(
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
