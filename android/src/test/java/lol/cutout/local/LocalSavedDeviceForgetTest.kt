package lol.cutout.local

import android.content.Context
import android.bluetooth.le.ScanRecord
import android.bluetooth.le.ScanResult
import androidx.test.core.app.ApplicationProvider
import lol.cutout.AndroidBleClient
import lol.cutout.CutoutApplication
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.Shadows.shadowOf
import org.robolectric.shadows.ShadowLog
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
class LocalSavedDeviceForgetTest {
    @Test
    fun forgetClearsRustSelectionAndExplicitRetryIsSafe() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val app = context as CutoutApplication
        val client = AndroidBleClient(context)
        val id = "AA:BB:CC:DD:EE:91"
        val previous = await { done -> app.rides.withDatabase({ it.selectedDevice() }, done) }
        try {
            await { done -> app.rides.withDatabase({ database ->
                database.saveSelectedDevice(id, 1UL)
                database.selectedDevice()
            }, done) }.also { check(it == id) }

            check(await { done -> client.savedDevice(done) } == id) {
                "BLE owner did not read the exact Rust-persisted selection"
            }
            await { done -> client.forgetSavedDevice(done) }
            check(await { done -> app.rides.withDatabase({ it.selectedDevice() }, done) } == null) {
                "Forget completed without clearing the shared Rust selection"
            }
            check(await { done -> client.savedDevice(done) } == null) {
                "BLE owner kept a stale in-memory selection after Forget"
            }

            // A repeated explicit retry is safe after the transport and selection are already clear.
            await { done -> client.forgetSavedDevice(done) }
            check(await { done -> app.rides.withDatabase({ it.selectedDevice() }, done) } == null) {
                "Retry recreated a saved selection"
            }
        } finally {
            val restored = runCatching {
                await { done -> app.rides.withDatabase({ database ->
                    previous?.let { database.saveSelectedDevice(it, 2UL) } ?: database.clearSelectedDevice()
                    database.selectedDevice()
                }, done) }
            }
            check(restored.isSuccess) { "The shared test database selection could not be restored" }
            val closed = CountDownLatch(1)
            client.close { closed.countDown() }
            check(awaitWithMainLooper(closed, 10, TimeUnit.SECONDS)) {
                "BLE client close callback did not complete"
            }
        }
    }

    @Test
    fun manualCaptureCannotStartWhileForgetIsWaitingForTheRustDatabase() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val app = context as CutoutApplication
        val client = AndroidBleClient(context)
        val adapter = context.getSystemService(android.bluetooth.BluetoothManager::class.java).adapter
        val address = "02:00:00:00:00:91"
        val device = adapter.getRemoteDevice(address)
        val record = parseRecord(listOf(17, 0x07, 0x9E, 0xCA, 0xDC, 0x24, 0x0E, 0xE5, 0xA9,
            0xE0, 0x93, 0xF3, 0xA3, 0xB5, 0x01, 0x00, 0x40, 0x6E).map(Int::toByte).toByteArray())
        val result = ScanResult(device, record, -42, 1L)
        val wasEnabled = adapter.isEnabled
        val permissions = listOf(android.Manifest.permission.BLUETOOTH_SCAN, android.Manifest.permission.BLUETOOTH_CONNECT)
        val missingPermissions = permissions.filter {
            context.checkSelfPermission(it) != android.content.pm.PackageManager.PERMISSION_GRANTED
        }
        shadowOf(app).grantPermissions(*permissions.toTypedArray())
        shadowOf(adapter).setEnabled(true)
        val scanner = checkNotNull(adapter.bluetoothLeScanner)
        val shadowScanner = shadowOf(scanner)
        val enteredDatabase = CountDownLatch(1)
        val releaseDatabase = CountDownLatch(1)
        val scanStarted = CountDownLatch(1)
        val candidateReady = CountDownLatch(1)
        val forgetResult = AtomicReference<Result<Unit>>()
        val forgetDone = CountDownLatch(1)
        val observer = client.addObserver { presentation ->
            if (presentation.scanning) scanStarted.countDown()
            if (presentation.devices.any { it.id == address }) candidateReady.countDown()
        }
        val previous = await { done -> app.rides.withDatabase({ it.selectedDevice() }, done) }
        try {
            await { done -> app.rides.withDatabase({ database ->
                database.saveSelectedDevice(address, 1UL)
                database.selectedDevice()
            }, done) }

            client.scan()
            fun scanDiagnostics(): String =
                "presentation=${client.latestPresentation}, " +
                    "activeScans=${shadowScanner.getActiveScans().size}, " +
                    "callbacks=${shadowScanner.getScanCallbacks().size}, " +
                    "CutoutBleLogs=${ShadowLog.getLogsForTag("CutoutBle").joinToString()}"

            check(awaitWithMainLooper(scanStarted, 10, TimeUnit.SECONDS)) {
                "The production Bluetooth scan did not start: ${scanDiagnostics()}"
            }
            val callback = shadowScanner.getScanCallbacks().singleOrNull()
            check(callback != null && shadowScanner.getActiveScans().isNotEmpty()) {
                "Android did not register the production scan callback: ${scanDiagnostics()}"
            }
            // Deliver through the registered Android callback so the real callback-to-Rust
            // observation path runs; do not rely on Robolectric's cached-result replay.
            callback!!.onScanResult(android.bluetooth.le.ScanSettings.CALLBACK_TYPE_ALL_MATCHES, result)
            check(awaitWithMainLooper(candidateReady, 10, TimeUnit.SECONDS)) {
                "The production Bluetooth scan callback did not publish the Rust candidate: ${scanDiagnostics()}"
            }

            app.rides.withDatabase({
                enteredDatabase.countDown()
                check(releaseDatabase.await(10, TimeUnit.SECONDS)) { "Test database blocker was not released" }
                Unit
            }) { }
            check(enteredDatabase.await(10, TimeUnit.SECONDS)) { "Could not hold the shared Rust database worker" }

            // Both calls are serialized on the BLE owner. Forget enqueues its DB clear first;
            // manual capture must be refused before that clear can complete.
            client.forgetSavedDevice { resultValue -> forgetResult.set(resultValue); forgetDone.countDown() }
            val manualStarted = await { done ->
                client.startManualCapture(address, null, null) { done(Result.success(it)) }
            }
            check(!manualStarted) { "Manual capture was admitted during Forget" }
            check(client.latestPresentation.phase == lol.cutout.NativePhase.IDLE) {
                "Refused manual capture still started a Bluetooth transport"
            }
            check(client.diagnostics.current().writerStatus == null) {
                "Refused manual capture still created a diagnostic recording"
            }
            check(forgetDone.count == 1L) { "Forget completed before its Rust database work was released" }

            releaseDatabase.countDown()
            check(awaitWithMainLooper(forgetDone, 10, TimeUnit.SECONDS)) { "Forget did not complete after database release" }
            checkNotNull(forgetResult.get()).getOrThrow()
            check(await { done -> app.rides.withDatabase({ it.selectedDevice() }, done) } == null) {
                "Forget did not clear the Rust selection after refusing the competing recording"
            }
        } finally {
            releaseDatabase.countDown()
            observer.close()
            val restored = runCatching {
                await { done -> app.rides.withDatabase({ database ->
                    previous?.let { database.saveSelectedDevice(it, 2UL) } ?: database.clearSelectedDevice()
                    database.selectedDevice()
                }, done) }
            }
            check(restored.isSuccess) { "The shared test database selection could not be restored" }
            val closed = CountDownLatch(1)
            client.close { closed.countDown() }
            check(awaitWithMainLooper(closed, 10, TimeUnit.SECONDS)) { "BLE client close callback did not complete" }
            shadowOf(adapter).setEnabled(wasEnabled)
            shadowOf(app).denyPermissions(*missingPermissions.toTypedArray())
        }
    }

    private fun <T> await(start: ((Result<T>) -> Unit) -> Unit): T {
        val result = AtomicReference<Result<T>>()
        val completed = CountDownLatch(1)
        start { result.set(it); completed.countDown() }
        check(awaitWithMainLooper(completed, 10, TimeUnit.SECONDS)) {
            "Saved-device operation did not complete"
        }
        return checkNotNull(result.get()).getOrThrow()
    }

    private fun parseRecord(vararg structures: ByteArray): ScanRecord {
        val bytes = structures.flatMap { it.asIterable() }.toByteArray() + byteArrayOf(0)
        val parser = ScanRecord::class.java.getDeclaredMethod("parseFromBytes", ByteArray::class.java)
            .apply { isAccessible = true }
        return parser.invoke(null, bytes) as ScanRecord
    }
}
