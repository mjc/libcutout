package lol.cutout

import android.content.Context
import android.util.AtomicFile
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Encryption boundary kept injectable so host tests can exercise persistence without a device key. */
internal interface SpotifyCredentialCipher {
    fun encrypt(plaintext: ByteArray): ByteArray
    fun decrypt(envelope: ByteArray): ByteArray
    fun clearKey()
}

/** Atomic encrypted storage for AppAuth's serialized AuthState (access and refresh tokens). */
internal class AndroidSpotifyCredentialStore(
    file: File,
    private val cipher: SpotifyCredentialCipher,
) {
    private val file = AtomicFile(file)

    @Synchronized
    fun save(authState: String) {
        require(authState.isNotBlank())
        val plaintext = authState.toByteArray(Charsets.UTF_8)
        require(plaintext.size <= MAX_ENVELOPE_SIZE - ENVELOPE_HEADER_BYTES - MIN_CIPHER_OVERHEAD)
        val ciphertext = cipher.encrypt(plaintext)
        require(ciphertext.size in MIN_ENVELOPE_SIZE..MAX_ENVELOPE_SIZE - ENVELOPE_HEADER_BYTES)
        val output = file.startWrite()
        try {
            DataOutputStream(output).also { stream ->
                stream.writeInt(FORMAT_VERSION)
                stream.writeInt(ciphertext.size)
                stream.write(ciphertext)
                stream.flush()
            }
            file.finishWrite(output)
        } catch (error: Throwable) {
            file.failWrite(output)
            throw error
        }
    }

    @Synchronized
    fun load(): String? {
        val backup = File(file.baseFile.path + ".bak")
        if (!file.baseFile.exists() && !backup.exists()) return null
        return try {
            DataInputStream(file.openRead()).use { input ->
                check(input.readInt() == FORMAT_VERSION)
                val size = input.readInt()
                check(size in MIN_ENVELOPE_SIZE..MAX_ENVELOPE_SIZE - ENVELOPE_HEADER_BYTES)
                val ciphertext = ByteArray(size)
                input.readFully(ciphertext)
                check(input.read() == -1)
                cipher.decrypt(ciphertext).toString(Charsets.UTF_8).also { check(it.isNotBlank()) }
            }
        } catch (_: Exception) {
            throw IllegalStateException("Saved Spotify authorization is unavailable")
        }
    }

    @Synchronized
    fun clear() {
        file.delete()
        check(!file.baseFile.exists() && !File(file.baseFile.path + ".bak").exists() &&
            !File(file.baseFile.path + ".new").exists()) {
            "Saved Spotify authorization could not be cleared"
        }
        cipher.clearKey()
    }

    internal companion object {
        const val FORMAT_VERSION = 1
        const val MAX_ENVELOPE_SIZE = 64 * 1024
        const val MIN_ENVELOPE_SIZE = 32
        private const val ENVELOPE_HEADER_BYTES = 8
        private const val MIN_CIPHER_OVERHEAD = 28

        fun fromContext(context: Context): AndroidSpotifyCredentialStore = AndroidSpotifyCredentialStore(
            File(context.noBackupFilesDir, "spotify-auth.enc"), AndroidKeystoreSpotifyCipher(),
        )
    }
}

/** AES-GCM key material is non-exportable and owned by Android Keystore. */
internal class AndroidKeystoreSpotifyCipher : SpotifyCredentialCipher {
    override fun encrypt(plaintext: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val encrypted = cipher.doFinal(plaintext)
        return ByteArrayOutputStream().use { bytes ->
            DataOutputStream(bytes).use { output ->
                output.writeInt(cipher.iv.size)
                output.write(cipher.iv)
                output.write(encrypted)
            }
            bytes.toByteArray()
        }
    }

    override fun decrypt(envelope: ByteArray): ByteArray {
        require(envelope.size in AndroidSpotifyCredentialStore.MIN_ENVELOPE_SIZE..AndroidSpotifyCredentialStore.MAX_ENVELOPE_SIZE)
        val input = DataInputStream(ByteArrayInputStream(envelope))
        val ivLength = input.readInt()
        require(ivLength == GCM_IV_BYTES)
        val iv = ByteArray(ivLength)
        input.readFully(iv)
        val encrypted = ByteArray(input.available())
        input.readFully(encrypted)
        require(encrypted.size >= GCM_TAG_BYTES)
        return Cipher.getInstance(TRANSFORMATION).run {
            init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(GCM_TAG_BITS, iv))
            doFinal(encrypted)
        }
    }

    override fun clearKey() {
        val store = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        if (store.containsAlias(KEY_ALIAS)) store.deleteEntry(KEY_ALIAS)
    }

    @Synchronized
    private fun key(): SecretKey {
        val store = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (store.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance("AES", ANDROID_KEYSTORE)
        generator.init(android.security.keystore.KeyGenParameterSpec.Builder(
            KEY_ALIAS,
            android.security.keystore.KeyProperties.PURPOSE_ENCRYPT or android.security.keystore.KeyProperties.PURPOSE_DECRYPT,
        ).setBlockModes(android.security.keystore.KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(android.security.keystore.KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .build())
        return generator.generateKey()
    }

    private companion object {
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val KEY_ALIAS = "lol.cutout.spotify.auth.v1"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val GCM_IV_BYTES = 12
        const val GCM_TAG_BITS = 128
        const val GCM_TAG_BYTES = 16
    }
}
