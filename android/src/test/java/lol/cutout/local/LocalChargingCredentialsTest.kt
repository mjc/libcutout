package lol.cutout.local

import java.io.File
import java.nio.file.Files
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec
import lol.cutout.AndroidChargingCredentials
import lol.cutout.ChargingCredentialRecord
import lol.cutout.ChargingSecretCipher
import lol.cutout.AndroidChargingClient
import lol.cutout.CutoutApplication
import uniffi.cutout_mobile_ffi.MobileChargingIdentityDto
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
class LocalChargingCredentialsTest {
    @Test
    fun captureCredentialReadsPreserveMissingAndDeletionFencesWithoutRegistration() {
        val root = Files.createTempDirectory("cutout-capture-credentials").toFile()
        val context = RuntimeEnvironment.getApplication() as CutoutApplication
        val store = AndroidChargingCredentials(context, TestAesGcmCipher(), File(root, "identity.bin"))
        val token = "a1".repeat(32)
        val runtime = context.rides
        val client = AndroidChargingClient(context, runtime, store)
        fun read(allowRegistration: Boolean = false): MobileChargingIdentityDto? {
            if (!allowRegistration) return kotlinx.coroutines.runBlocking { client.existingCredentialForCaptureSubmission() }
            val ready = CountDownLatch(1)
            val result = AtomicReference<Result<MobileChargingIdentityDto?>?>()
            client.credentialForCaptureSubmission { result.set(it); ready.countDown() }
            check(awaitWithMainLooper(ready, 10, TimeUnit.SECONDS))
            return checkNotNull(result.get()).getOrThrow()
        }
        try {
            assertEquals(null, read())
            assertEquals(null, store.load())
            store.beginRegistration()
            assertEquals(null, read())
            assertEquals(ChargingCredentialRecord.RegistrationStarted, store.load())
            store.saveIdentity("existing-identity", token)
            assertEquals(MobileChargingIdentityDto("existing-identity", token), read())
            assertEquals(MobileChargingIdentityDto("existing-identity", token), read(true))
            val workerEntered = CountDownLatch(1)
            val releaseWorker = CountDownLatch(1)
            val workerFinished = CountDownLatch(1)
            client.withDatabase({ workerEntered.countDown(); releaseWorker.await(); Unit }) { workerFinished.countDown() }
            try {
                check(awaitWithMainLooper(workerEntered, 10, TimeUnit.SECONDS))
                val independentRead = java.util.concurrent.CompletableFuture.supplyAsync { read() }
                assertEquals(MobileChargingIdentityDto("existing-identity", token), independentRead.get(3, TimeUnit.SECONDS))
                assertEquals("Credential read must complete before the charging network worker is released", 1L, releaseWorker.count)
            } finally { releaseWorker.countDown(); check(awaitWithMainLooper(workerFinished, 10, TimeUnit.SECONDS)) }
            store.markDeletionPending()
            assertEquals(null, read())
            assertEquals(ChargingCredentialRecord.DeletionPending("existing-identity", token), store.load())
            store.finishDeletion()
            assertEquals(null, read())
            assertEquals(ChargingCredentialRecord.Deleted, store.load())
        } finally { client.close(); root.deleteRecursively() }
    }

    @Test
    fun registrationAndDeletionFencesSurviveAtomicEncryptedStorage() {
        val root = Files.createTempDirectory("cutout-charging-credentials").toFile()
        val token = "a1".repeat(32)
        val store = AndroidChargingCredentials(
            RuntimeEnvironment.getApplication(), TestAesGcmCipher(), File(root, "charging/identity.bin"),
        )

        assertEquals(null, store.load())
        assertEquals(true, store.beginRegistration())
        assertEquals(ChargingCredentialRecord.RegistrationStarted, store.load())
        store.saveIdentity("identity-123", token)
        assertEquals(ChargingCredentialRecord.Ready("identity-123", token), store.load())
        assertFalse(File(root, "charging/identity.bin").readBytes().toString(Charsets.ISO_8859_1).contains(token))
        store.markDeletionPending()
        assertEquals(ChargingCredentialRecord.DeletionPending("identity-123", token), store.load())
        store.finishDeletion()
        assertEquals(ChargingCredentialRecord.Deleted, store.load())
    }

    private class TestAesGcmCipher : ChargingSecretCipher {
        private val key = SecretKeySpec(ByteArray(32) { it.toByte() }, "AES")
        override fun encrypt(cleartext: ByteArray): ByteArray {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, key)
            return byteArrayOf(1, cipher.iv.size.toByte()) + cipher.iv + cipher.doFinal(cleartext)
        }
        override fun decrypt(ciphertext: ByteArray): ByteArray {
            val size = ciphertext[1].toInt() and 0xff
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, ciphertext.copyOfRange(2, 2 + size)))
            return cipher.doFinal(ciphertext.copyOfRange(2 + size, ciphertext.size))
        }
    }
}
