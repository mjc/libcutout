package lol.cutout.local

import android.bluetooth.le.ScanRecord
import android.bluetooth.le.ScanResult
import android.content.Context
import androidx.test.core.app.ApplicationProvider
import lol.cutout.androidBleScanObservation
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.DiscoveryCandidateSupport
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AndroidBleScanObservationTest {
    @Test
    fun frameworkParsedFfe0AndNordicAdvertisementsReachRustCandidateProjection() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val adapter = context.getSystemService(android.bluetooth.BluetoothManager::class.java).adapter
        val cases = listOf(
            "FFE0" to byteArrayOf(3, 0x03, 0xE0.toByte(), 0xFF.toByte()),
            "NUS" to listOf(17, 0x07, 0x9E, 0xCA, 0xDC, 0x24, 0x0E, 0xE5, 0xA9,
                0xE0, 0x93, 0xF3, 0xA3, 0xB5, 0x01, 0x00, 0x40, 0x6E).map(Int::toByte).toByteArray(),
        )

        cases.forEachIndexed { index, (label, serviceAd) ->
            val address = "02:00:00:00:00:${(index + 1).toString(16).padStart(2, '0')}"
            val record = parseRecord(serviceAd)
            val result = ScanResult(adapter.getRemoteDevice(address), record, -42, 1L)
            val observation = androidBleScanObservation(result).toRustObservation()
            CutoutSessionStateHandle().use { rust ->
                val snapshot = rust.observeDiscovery(observation)
                val candidate = snapshot.pickerCandidates.singleOrNull()
                assertTrue("$label service advertisement should be admitted by Rust", candidate != null)
                assertEquals(DiscoveryCandidateSupport.PROBE_RECOMMENDED, candidate?.support)
            }
        }
    }

    @Test
    fun frameworkParsedNoiseDoesNotBecomeARustPickerCandidate() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val adapter = context.getSystemService(android.bluetooth.BluetoothManager::class.java).adapter
        val record = parseRecord(byteArrayOf(2, 0x01, 0x06))
        val result = ScanResult(adapter.getRemoteDevice("02:00:00:00:00:03"), record, -50, 1L)
        val observation = androidBleScanObservation(result).toRustObservation()

        CutoutSessionStateHandle().use { rust ->
            assertTrue(rust.observeDiscovery(observation).pickerCandidates.isEmpty())
        }
    }

    private fun parseRecord(vararg structures: ByteArray): ScanRecord {
        val bytes = structures.flatMap { it.asIterable() }.toByteArray() + byteArrayOf(0)
        val parser = ScanRecord::class.java.getDeclaredMethod("parseFromBytes", ByteArray::class.java)
            .apply { isAccessible = true }
        return parser.invoke(null, bytes) as ScanRecord
    }

}
