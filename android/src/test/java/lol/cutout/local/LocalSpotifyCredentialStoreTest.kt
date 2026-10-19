package lol.cutout.local

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import lol.cutout.AndroidSpotifyCredentialStore
import lol.cutout.SpotifyCredentialCipher
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class LocalSpotifyCredentialStoreTest {
    private class TestCipher : SpotifyCredentialCipher {
        var keyAvailable = true
        override fun encrypt(plaintext: ByteArray): ByteArray {
            val encrypted = plaintext.map { (it.toInt() xor 0x5a).toByte() }.toByteArray()
            return byteArrayOf(0x51) + encrypted + java.security.MessageDigest.getInstance("SHA-256").digest(plaintext)
        }
        override fun decrypt(envelope: ByteArray): ByteArray {
            check(keyAvailable) { "credential key unavailable" }
            require(envelope.firstOrNull() == 0x51.toByte())
            require(envelope.size > 33)
            val encrypted = envelope.copyOfRange(1, envelope.size - 32)
            val expected = envelope.copyOfRange(envelope.size - 32, envelope.size)
            val plaintext = encrypted.map { (it.toInt() xor 0x5a).toByte() }.toByteArray()
            require(java.security.MessageDigest.isEqual(expected, java.security.MessageDigest.getInstance("SHA-256").digest(plaintext)))
            return plaintext
        }
        override fun clearKey() { keyAvailable = false }
    }

    @Test fun blockedCredentialWriteCannotSurviveDisconnectClear() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-store-${System.nanoTime()}.enc")
        val enteredEncrypt = CountDownLatch(1)
        val releaseEncrypt = CountDownLatch(1)
        val cipher = object : SpotifyCredentialCipher {
            override fun encrypt(plaintext: ByteArray): ByteArray {
                enteredEncrypt.countDown()
                check(releaseEncrypt.await(3, TimeUnit.SECONDS))
                return ByteArray(32) { 7 }
            }
            override fun decrypt(envelope: ByteArray): ByteArray = error("not used")
            override fun clearKey() = Unit
        }
        val store = AndroidSpotifyCredentialStore(file, cipher)
        val writer = Thread { store.save("serialized authorization") }
        val clearer = Thread { store.clear() }

        writer.start()
        assertTrue("credential write must reach the encrypted persistence boundary", enteredEncrypt.await(3, TimeUnit.SECONDS))
        clearer.start()
        releaseEncrypt.countDown()
        writer.join(3_000)
        clearer.join(3_000)

        assertTrue("write must finish before the serialized clear receipt", !writer.isAlive)
        assertTrue("disconnect clear must finish", !clearer.isAlive)
        assertTrue("credential base must be absent", !file.exists())
        assertTrue("AtomicFile backup must be absent", !File(file.path + ".bak").exists())
        assertTrue("AtomicFile temporary must be absent", !File(file.path + ".new").exists())
        assertNull(store.load())
    }

    @Test fun storesOnlyEncryptedEnvelopeAndRoundTripsAtomically() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-store-${System.nanoTime()}.enc")
        val cipher = TestCipher()
        val store = AndroidSpotifyCredentialStore(file, cipher)
        val serialized = "{\"accessToken\":\"opaque-test-token\",\"refreshToken\":\"opaque-test-refresh\"}"

        store.save(serialized)

        assertEquals(serialized, store.load())
        val bytes = file.readBytes()
        assertEquals(AndroidSpotifyCredentialStore.FORMAT_VERSION, java.nio.ByteBuffer.wrap(bytes).int)
        assert(!String(bytes, Charsets.UTF_8).contains("opaque-test-token"))
        assert(!String(bytes, Charsets.UTF_8).contains("opaque-test-refresh"))
        store.clear()
        assertNull(store.load())
        File(file.path + ".new").createNewFile()
        store.clear()
        assertTrue("AtomicFile temporary must be removed before the clear receipt", !File(file.path + ".new").exists())
    }

    @Test fun restoresAtomicBackupWhenPrimaryFileIsAbsent() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-store-${System.nanoTime()}.enc")
        val store = AndroidSpotifyCredentialStore(file, TestCipher())
        val serialized = "serialized-auth-state"
        store.save(serialized)
        File(file.path + ".bak").writeBytes(file.readBytes())
        file.delete()

        assertEquals(serialized, store.load())
        assertTrue(file.exists())
    }

    @Test fun refusesSerializedStateThatWouldExceedTheLoadEnvelopeLimit() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-store-${System.nanoTime()}.enc")
        val store = AndroidSpotifyCredentialStore(file, TestCipher())

        assertThrows(IllegalArgumentException::class.java) {
            store.save("x".repeat(AndroidSpotifyCredentialStore.MAX_ENVELOPE_SIZE))
        }
        assertTrue("oversized authorization must not create a partial file", !file.exists())
    }

    @Test fun rejectsTruncatedAndTamperedEnvelopeAndDoesNotRestoreAfterKeyClear() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val file = File(context.cacheDir, "spotify-store-${System.nanoTime()}.enc")
        val store = AndroidSpotifyCredentialStore(file, TestCipher())

        file.writeBytes(byteArrayOf(0, 0, 0, 1))
        assertThrows(IllegalStateException::class.java) { store.load() }
        store.save("serialized-auth-state")
        val tampered = file.readBytes().also { bytes -> bytes[bytes.lastIndex] = (bytes.last().toInt() xor 0x1).toByte() }
        file.writeBytes(tampered)
        assertThrows(IllegalStateException::class.java) { store.load() }
        store.save("serialized-auth-state")
        store.clear()
        assertNull(store.load())
    }
}
