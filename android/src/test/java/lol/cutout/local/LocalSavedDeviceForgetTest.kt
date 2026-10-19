package lol.cutout.local

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import lol.cutout.AndroidBleClient
import lol.cutout.CutoutApplication
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
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

    private fun <T> await(start: ((Result<T>) -> Unit) -> Unit): T {
        val result = AtomicReference<Result<T>>()
        val completed = CountDownLatch(1)
        start { result.set(it); completed.countDown() }
        check(awaitWithMainLooper(completed, 10, TimeUnit.SECONDS)) {
            "Saved-device operation did not complete"
        }
        return checkNotNull(result.get()).getOrThrow()
    }
}
