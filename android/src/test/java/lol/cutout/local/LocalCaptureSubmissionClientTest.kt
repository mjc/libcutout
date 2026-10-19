package lol.cutout.local

import android.os.Looper
import kotlinx.coroutines.CompletableDeferred
import lol.cutout.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import uniffi.cutout_mobile_ffi.*
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
class LocalCaptureSubmissionClientTest {
    private val source = MobileCaptureSubmissionSourceDto.LiveCapture("selected-capture")

    @Test
    fun prepareHasNoCredentialEffectAndRetryReusesTheOperationAfterUncertainSend() {
        val backend = Backend()
        val credentials = mutableListOf<Boolean>()
        val owner = owner(backend, credentials)
        val model = owner.controller(source)
        val observe = model.observe {}
        try {
            model.refresh(); await(model) { !it.working }
            model.prepare(); await(model) { !it.working && it.status != null }
            assertTrue(credentials.isEmpty())
            assertEquals(source, backend.lastSource)
            backend.sendAction = { operation ->
                backend.persisted = state(MobileCaptureSubmissionPhaseDto.SENDING)
                throw MobileCaptureSubmissionException.Unavailable()
            }
            model.send(); await(model) { !it.working && !it.sending }
            assertEquals(MobileCaptureSubmissionPhaseDto.SENDING, model.current().status?.phase)
            assertEquals("capture.submission.error.unavailable", model.current().error)
            backend.sendAction = { state(MobileCaptureSubmissionPhaseDto.SENT) }
            model.send(); await(model) { !it.working && !it.sending }
            assertEquals(listOf("snapshot-operation", "snapshot-operation"), backend.sends)
            assertEquals(listOf(true, true), credentials)
            assertEquals(1, backend.prepares)
        } finally { observe.close(); owner.close() }
    }

    @Test
    fun cancellationRunsDuringSendAndItsResultSurvivesTheLateSendCallback() {
        val backend = Backend().apply { persisted = state(MobileCaptureSubmissionPhaseDto.PREPARED) }
        val credentials = mutableListOf<Boolean>()
        val sent = CompletableDeferred<MobileCaptureSubmissionStatusDto>()
        val sendEntered = CountDownLatch(1)
        backend.sendAction = { sendEntered.countDown(); sent.await() }
        val owner = owner(backend, credentials)
        val model = owner.controller(source)
        val observe = model.observe {}
        try {
            model.refresh(); await(model) { !it.working && it.status != null }
            model.send()
            assertTrue(awaitWithMainLooper(sendEntered, 10, TimeUnit.SECONDS))
            model.cancel(); await(model) { !it.cancelling && it.status?.phase == MobileCaptureSubmissionPhaseDto.CANCELLED }
            assertTrue("Send is still awaiting its independent native result", model.current().sending)
            sent.complete(state(MobileCaptureSubmissionPhaseDto.SENT))
            await(model) { !it.sending }
            assertEquals(MobileCaptureSubmissionPhaseDto.CANCELLED, model.current().status?.phase)
            assertEquals(listOf(true, false), credentials)
            assertEquals(1, backend.cancels)
        } finally { sent.cancel(); observe.close(); owner.close() }
    }

    @Test
    fun cancellationAndDeletionNeverCreateAnAccountOrClaimAnUnconfirmedRemoval() {
        val backend = Backend().apply { persisted = state(MobileCaptureSubmissionPhaseDto.PREPARED) }
        val credentials = mutableListOf<Boolean>()
        val owner = AndroidCaptureSubmissionClient({ backend }, { allow -> credentials += allow; null },
            AndroidCaptureSubmissionClient::errorKey)
        val model = owner.controller(source)
        val observe = model.observe {}
        try {
            model.refresh(); await(model) { !it.working && it.status != null }
            model.cancel(); await(model) { !it.working && !it.cancelling }
            assertEquals(MobileCaptureSubmissionPhaseDto.CANCELLED, model.current().status?.phase)
            backend.persisted = state(MobileCaptureSubmissionPhaseDto.SENT)
            model.refresh(); await(model) { !it.working && it.status?.phase == MobileCaptureSubmissionPhaseDto.SENT }
            model.delete(); await(model) { !it.working }
            assertEquals(0, backend.deletes)
            assertEquals(MobileCaptureSubmissionPhaseDto.SENT, model.current().status?.phase)
            assertEquals("capture.submission.error.unauthorized", model.current().error)
            assertEquals(listOf(false, false), credentials)
        } finally { observe.close(); owner.close() }
    }

    @Test
    fun leavingTheViewKeepsItsTransferOwnerAndReopeningDoesNotStartAnotherSend() {
        val backend = Backend().apply { persisted = state(MobileCaptureSubmissionPhaseDto.PREPARED) }
        val sent = CompletableDeferred<MobileCaptureSubmissionStatusDto>()
        val sendEntered = CountDownLatch(1)
        backend.sendAction = { sendEntered.countDown(); sent.await() }
        val owner = owner(backend, mutableListOf())
        val first = owner.controller(source)
        var observation = first.observe {}
        try {
            first.refresh(); await(first) { !it.working && it.status != null }
            first.send(); assertTrue(awaitWithMainLooper(sendEntered, 10, TimeUnit.SECONDS))
            observation.close()
            val reopened = owner.controller(source)
            assertSame(first, reopened)
            observation = reopened.observe {}
            reopened.refresh()
            assertEquals(1, backend.sends.size)
            sent.complete(state(MobileCaptureSubmissionPhaseDto.SENT))
            await(reopened) { !it.sending && !it.working }
            observation.close()
            val idle = owner.controller(source)
            observation = idle.observe {}
            idle.refresh(); await(idle) { !it.working && it.status != null }
            assertEquals(MobileCaptureSubmissionPhaseDto.SENT, idle.current().status?.phase)
            assertEquals(1, backend.sends.size)
        } finally { sent.cancel(); observation.close(); owner.close() }
    }

    private fun owner(backend: Backend, credentials: MutableList<Boolean>) =
        AndroidCaptureSubmissionClient({ backend }, { allow ->
            credentials += allow
            MobileChargingIdentityDto("existing-account", "test-token")
        }, AndroidCaptureSubmissionClient::errorKey)

    private fun await(model: CaptureSubmissionController, condition: (CaptureSubmissionPresentation) -> Boolean) {
        val ready = CountDownLatch(1)
        val observation = model.observe { if (condition(it)) ready.countDown() }
        try { assertTrue("Capture presentation did not reach the expected state", awaitWithMainLooper(ready, 10, TimeUnit.SECONDS)) }
        finally { observation.close() }
        shadowOf(Looper.getMainLooper()).idle()
    }

    private class Backend : CaptureSubmissionClientHandleInterface {
        @Volatile var persisted: MobileCaptureSubmissionStatusDto? = null
        @Volatile var lastSource: MobileCaptureSubmissionSourceDto? = null
        var prepares = 0
        var cancels = 0
        var deletes = 0
        val sends = mutableListOf<String>()
        var sendAction: suspend (String) -> MobileCaptureSubmissionStatusDto = { state(MobileCaptureSubmissionPhaseDto.SENT) }
        override suspend fun status(source: MobileCaptureSubmissionSourceDto): MobileCaptureSubmissionStatusDto? {
            lastSource = source; return persisted
        }
        override suspend fun prepare(source: MobileCaptureSubmissionSourceDto): MobileCaptureSubmissionStatusDto {
            lastSource = source; prepares++; return state(MobileCaptureSubmissionPhaseDto.PREPARED).also { persisted = it }
        }
        override suspend fun send(operationId: String, accountId: String, token: String): MobileCaptureSubmissionStatusDto {
            sends += operationId; return sendAction(operationId).also { persisted = it }
        }
        override suspend fun cancel(operationId: String, accountId: String?, token: String?): MobileCaptureSubmissionStatusDto {
            cancels++; return state(MobileCaptureSubmissionPhaseDto.CANCELLED).also { persisted = it }
        }
        override suspend fun delete(operationId: String, accountId: String, token: String): MobileCaptureSubmissionStatusDto {
            deletes++; return state(MobileCaptureSubmissionPhaseDto.DELETED).also { persisted = it }
        }
    }

    companion object {
        private fun state(phase: MobileCaptureSubmissionPhaseDto) = MobileCaptureSubmissionStatusDto(
            "snapshot-operation", phase, MobileCaptureSubmissionCategoriesDto(true, false, true, true, true),
            0UL, 4096UL, null, null,
        )
    }
}
