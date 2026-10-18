package lol.cutout

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

internal sealed interface ChargingCredentialRecord {
    data object RegistrationStarted : ChargingCredentialRecord
    data class Ready(val identityId: String, val token: String) : ChargingCredentialRecord
    data class DeletionPending(val identityId: String, val token: String) : ChargingCredentialRecord
    data object Deleted : ChargingCredentialRecord
}

internal interface ChargingSecretCipher {
    fun encrypt(cleartext: ByteArray): ByteArray
    fun decrypt(ciphertext: ByteArray): ByteArray
}

/** Encrypted native credential storage; no token or identity is written to ordinary prefs/files. */
class AndroidChargingCredentials internal constructor(
    context: Context,
    cipher: ChargingSecretCipher,
    file: java.io.File,
) {
    constructor(context: Context) : this(
        context.applicationContext,
        AndroidKeystoreChargingCipher("${context.packageName}.charging.identity.v1"),
        java.io.File(context.applicationContext.noBackupFilesDir, "charging/identity.bin"),
    )

    private val atomicFile = AtomicFile(file)
    private val cipher = cipher

    @Synchronized
    internal fun load(): ChargingCredentialRecord? {
        if (!atomicFile.baseFile.exists()) return null
        val encrypted = atomicFile.openRead().use { it.readBytes() }
        return decode(cipher.decrypt(encrypted))
    }

    /** Writes the uncertainty fence before registration can make a remote identity. */
    @Synchronized
    internal fun beginRegistration(): Boolean {
        if (load() != null) return false
        write(ChargingCredentialRecord.RegistrationStarted)
        return true
    }

    @Synchronized
    internal fun saveIdentity(identityId: String, token: String) {
        check(load() == ChargingCredentialRecord.RegistrationStarted) {
            "Charging identity registration state changed"
        }
        require(identityId.isNotBlank() && token.length == 64 && token.all { it.digitToIntOrNull(16) != null })
        write(ChargingCredentialRecord.Ready(identityId, token))
    }

    @Synchronized
    internal fun markDeletionPending() {
        val ready = load() as? ChargingCredentialRecord.Ready ?: error("No ready charging identity")
        write(ChargingCredentialRecord.DeletionPending(ready.identityId, ready.token))
    }

    @Synchronized
    internal fun finishDeletion() {
        check(load() is ChargingCredentialRecord.DeletionPending) {
            "Charging account deletion is not pending"
        }
        write(ChargingCredentialRecord.Deleted)
    }

    @Synchronized
    private fun write(record: ChargingCredentialRecord) {
        val encrypted = cipher.encrypt(encode(record))
        atomicFile.baseFile.parentFile?.let { directory ->
            check(directory.isDirectory || directory.mkdirs()) { "Cannot create charging credential directory" }
        }
        val output = atomicFile.startWrite()
        try {
            output.write(encrypted)
            atomicFile.finishWrite(output)
        } catch (error: Throwable) {
            atomicFile.failWrite(output)
            throw error
        }
    }

    private fun encode(record: ChargingCredentialRecord): ByteArray = ByteArrayOutputStream().use { bytes ->
        DataOutputStream(bytes).use { output ->
            output.writeInt(STORE_VERSION)
            when (record) {
                ChargingCredentialRecord.RegistrationStarted -> output.writeByte(1)
                is ChargingCredentialRecord.Ready -> {
                    output.writeByte(2)
                    output.writeUTF(record.identityId)
                    output.writeUTF(record.token)
                }
                is ChargingCredentialRecord.DeletionPending -> {
                    output.writeByte(3)
                    output.writeUTF(record.identityId)
                    output.writeUTF(record.token)
                }
                ChargingCredentialRecord.Deleted -> output.writeByte(4)
            }
        }
        bytes.toByteArray()
    }

    private fun decode(cleartext: ByteArray): ChargingCredentialRecord = DataInputStream(ByteArrayInputStream(cleartext)).use { input ->
        require(input.readInt() == STORE_VERSION) { "Unsupported charging credential version" }
        val record = when (input.readUnsignedByte()) {
            1 -> ChargingCredentialRecord.RegistrationStarted
            2 -> ChargingCredentialRecord.Ready(input.readUTF(), input.readUTF())
            3 -> ChargingCredentialRecord.DeletionPending(input.readUTF(), input.readUTF())
            4 -> ChargingCredentialRecord.Deleted
            else -> error("Invalid charging credential state")
        }
        require(input.available() == 0) { "Trailing charging credential data" }
        record
    }

    private companion object { const val STORE_VERSION = 1 }
}

private class AndroidKeystoreChargingCipher(private val alias: String) : ChargingSecretCipher {
    override fun encrypt(cleartext: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val iv = cipher.iv
        val encrypted = cipher.doFinal(cleartext)
        return byteArrayOf(FORMAT_VERSION, iv.size.toByte()) + iv + encrypted
    }

    override fun decrypt(ciphertext: ByteArray): ByteArray {
        require(ciphertext.size > 2 && ciphertext[0] == FORMAT_VERSION) { "Invalid credential envelope" }
        val ivSize = ciphertext[1].toInt() and 0xff
        require(ivSize == GCM_IV_BYTES && ciphertext.size > 2 + ivSize) { "Invalid credential envelope" }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(GCM_TAG_BITS, ciphertext.copyOfRange(2, 2 + ivSize)))
        return cipher.doFinal(ciphertext.copyOfRange(2 + ivSize, ciphertext.size))
    }

    @Synchronized
    private fun key(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(KeyGenParameterSpec.Builder(
            alias,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        ).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setRandomizedEncryptionRequired(true)
            .build())
        return generator.generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val GCM_IV_BYTES = 12
        const val GCM_TAG_BITS = 128
        const val FORMAT_VERSION: Byte = 1
    }
}
