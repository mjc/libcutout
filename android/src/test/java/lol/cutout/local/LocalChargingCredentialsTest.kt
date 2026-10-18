package lol.cutout.local

import java.io.File
import java.nio.file.Files
import javax.crypto.Cipher
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec
import lol.cutout.AndroidChargingCredentials
import lol.cutout.ChargingCredentialRecord
import lol.cutout.ChargingSecretCipher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LocalChargingCredentialsTest {
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
