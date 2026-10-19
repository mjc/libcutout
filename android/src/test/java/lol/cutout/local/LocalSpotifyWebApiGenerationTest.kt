package lol.cutout.local

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import lol.cutout.AndroidSpotifyCredentialStore
import lol.cutout.AndroidSpotifyWebApi
import lol.cutout.SpotifyCredentialCipher
import net.openid.appauth.AuthState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executor
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
class LocalSpotifyWebApiGenerationTest {
    private class QueueExecutor : Executor {
        private val tasks = mutableListOf<Runnable>()
        override fun execute(command: Runnable) { synchronized(tasks) { tasks += command } }
        fun runAll() {
            while (true) {
                val next = synchronized(tasks) { if (tasks.isEmpty()) null else tasks.removeAt(0) } ?: return
                next.run()
            }
        }
    }

    private open class TestCipher : SpotifyCredentialCipher {
        override fun encrypt(plaintext: ByteArray): ByteArray = byteArrayOf(1) + plaintext +
            MessageDigest.getInstance("SHA-256").digest(plaintext)
        override fun decrypt(envelope: ByteArray): ByteArray {
            val plain = envelope.copyOfRange(1, envelope.size - 32)
            check(MessageDigest.isEqual(envelope.copyOfRange(envelope.size - 32, envelope.size),
                MessageDigest.getInstance("SHA-256").digest(plain)))
            return plain
        }
        override fun clearKey() = Unit
    }

    @Test fun disconnectSerializesWithTheActualRefreshedGrantWrite() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-api-${System.nanoTime()}.enc")
        val enteredEncrypt = CountDownLatch(1)
        val releaseEncrypt = CountDownLatch(1)
        val cipher = object : TestCipher() {
            override fun encrypt(plaintext: ByteArray): ByteArray {
                enteredEncrypt.countDown()
                check(releaseEncrypt.await(3, TimeUnit.SECONDS))
                return super.encrypt(plaintext)
            }
        }
        val queue = QueueExecutor()
        val store = AndroidSpotifyCredentialStore(file, cipher)
        val api = AndroidSpotifyWebApi(context, "", store, credentialExecutor = queue)
        val state = AuthState()
        val revision = api.currentGrantRevision()
        api.retireProviderWork()
        val refreshResult = arrayOfNulls<Result<Unit>>(1)
        val clearResult = arrayOfNulls<Result<Unit>>(1)
        val clearEntered = CountDownLatch(1)
        val clearFinished = CountDownLatch(1)
        val writer = Thread { refreshResult[0] = api.persistRefreshedState(state, revision) }
        val clearer = Thread {
            clearEntered.countDown()
            api.clearCredentials { result -> clearResult[0] = result; clearFinished.countDown() }
        }

        try {
            writer.start()
            assertTrue("refresh must reach the serialized credential write", enteredEncrypt.await(3, TimeUnit.SECONDS))
            clearer.start()
            assertTrue(clearEntered.await(3, TimeUnit.SECONDS))
            releaseEncrypt.countDown()
            writer.join(3_000)
            clearer.join(3_000)
            assertFalse("refresh write settled", writer.isAlive)
            assertFalse("disconnect fenced the grant before queuing durable deletion", clearer.isAlive)
            queue.runAll()
            assertTrue("clear completion returned", clearFinished.await(3, TimeUnit.SECONDS))
            assertTrue(refreshResult[0]?.isSuccess == true)
            assertTrue(clearResult[0]?.isSuccess == true)
            assertNull(store.load())
            assertFalse(api.isAuthorized)
        } finally {
            releaseEncrypt.countDown()
            api.close()
        }
    }

    @Test fun oldAuthorizationCallbackCannotInstallAfterDisconnect() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-api-${System.nanoTime()}.enc")
        val queue = QueueExecutor()
        val store = AndroidSpotifyCredentialStore(file, TestCipher())
        val api = AndroidSpotifyWebApi(context, "", store, credentialExecutor = queue)
        val oldRevision = api.currentGrantRevision()
        var clearResult: Result<Unit>? = null
        api.clearCredentials { clearResult = it }

        try {
            assertTrue("disconnect advances grant identity before clearing disk",
                api.installAuthorizationState(AuthState(), oldRevision).isFailure)
            queue.runAll()
            assertTrue(clearResult?.isSuccess == true)
            assertNull(store.load())
            assertFalse(api.isAuthorized)
        } finally { api.close() }
    }

    @Test fun authorizationObjectAndGrantRevisionAreCapturedAtomically() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-api-${System.nanoTime()}.enc")
        val queue = QueueExecutor()
        val store = AndroidSpotifyCredentialStore(file, TestCipher())
        val api = AndroidSpotifyWebApi(context, "", store, credentialExecutor = queue)
        val selectedState = AuthState()
        val installRevision = api.currentGrantRevision()

        try {
            assertTrue(api.installAuthorizationState(selectedState, installRevision).isSuccess)
            val (capturedState, capturedRevision) = api.currentAuthorizationSnapshot()

            assertSame("the selected state and its identity come from the same grant", selectedState, capturedState)
            assertEquals(installRevision, capturedRevision)

            api.clearCredentials()
            assertTrue("a late refresh cannot persist against the retired grant",
                api.persistRefreshedState(capturedState!!, capturedRevision).isFailure)
            queue.runAll()
            assertNull(store.load())
        } finally { api.close() }
    }

    @Test fun queuedDisconnectCannotDeleteAReplacementGrant() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-api-${System.nanoTime()}.enc")
        val queue = QueueExecutor()
        val store = AndroidSpotifyCredentialStore(file, TestCipher())
        val api = AndroidSpotifyWebApi(context, "", store, credentialExecutor = queue)
        var clearResult: Result<Unit>? = null

        try {
            api.clearCredentials { clearResult = it }
            api.retireAuthorization(99uL)
            val replacementRevision = api.currentGrantRevision()
            val replacement = AuthState()
            assertTrue("new authorization is installed before the queued clear executes",
                api.installAuthorizationState(replacement, replacementRevision).isSuccess)
            val encryptedReplacement = store.load()
            assertTrue("replacement grant was durably written", encryptedReplacement != null)

            queue.runAll()

            assertTrue("the superseded clear reports stale", clearResult?.isFailure == true)
            assertEquals("queued clear must preserve the replacement grant", encryptedReplacement, store.load())
            assertSame(replacement, api.currentAuthorizationSnapshot().first)
        } finally { api.close() }
    }

    @Test fun closingOwnerDoesNotCancelAlreadyRequestedCredentialClear() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-api-${System.nanoTime()}.enc")
        val queue = QueueExecutor()
        val store = AndroidSpotifyCredentialStore(file, TestCipher())
        store.save("previous authorization")
        val api = AndroidSpotifyWebApi(context, "", store, credentialExecutor = queue)
        var clearResult: Result<Unit>? = null

        api.clearCredentials { clearResult = it }
        api.close()
        assertTrue("closed owner rejects late authorization installation",
            api.installAuthorizationState(AuthState(), api.currentGrantRevision()).isFailure)
        queue.runAll()

        assertTrue("closed owner still completes explicit durable cleanup", clearResult?.isSuccess == true)
        assertNull(store.load())
    }

    @Test fun failedOldCredentialRestoreCannotOverwriteNewerGrantStatus() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-api-${System.nanoTime()}.enc")
        val queue = QueueExecutor()
        val store = AndroidSpotifyCredentialStore(file, TestCipher())
        val api = AndroidSpotifyWebApi(context, "", store, credentialExecutor = queue)
        store.save("not an AppAuth state")
        var restoreResult: Result<Boolean>? = null

        try {
            api.retryCredentialRestore { restoreResult = it }
            api.retireAuthorization(99uL)
            queue.runAll()

            assertTrue(restoreResult?.isFailure == true)
            assertFalse("stale restore failure must not overwrite the newer generation", api.hasCredentialRestoreFailure)
        } finally { api.close() }
    }
}
