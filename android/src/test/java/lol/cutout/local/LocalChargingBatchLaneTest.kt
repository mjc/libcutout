package lol.cutout.local

import android.app.Activity
import lol.cutout.AndroidChargingClient
import lol.cutout.RideMapRuntime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalChargingBatchLaneTest {
    @Test
    fun blockedCaptureBatchDoesNotOccupyChargingDatabaseWorker() {
        val host = Robolectric.buildActivity(Activity::class.java).setup()
        val activity = host.get()
        val runtime = RideMapRuntime(activity)
        val charging = AndroidChargingClient(activity, runtime)
        val batchStarted = CountDownLatch(1)
        val releaseBatch = CountDownLatch(1)
        val batchCompleted = CountDownLatch(1)
        val databaseCompleted = CountDownLatch(1)
        val callbackCount = AtomicInteger()
        val batchResult = AtomicReference<Result<String>?>()
        val databaseResult = AtomicReference<Result<Unit>?>()

        try {
            charging.runCaptureBatch({
                batchStarted.countDown()
                check(releaseBatch.await(10, TimeUnit.SECONDS)) { "Fake batch transport was not released" }
                "sent"
            }) { result ->
                callbackCount.incrementAndGet()
                batchResult.set(result)
                batchCompleted.countDown()
            }
            assertTrue(awaitWithMainLooper(batchStarted, 5, TimeUnit.SECONDS))

            charging.withDatabase({ database ->
                database.chargingDirectoryLastUpdatedMs()
                Unit
            }) { result ->
                databaseResult.set(result)
                databaseCompleted.countDown()
            }
            assertTrue(
                "Unrelated charging database work waited behind the batch transport",
                awaitWithMainLooper(databaseCompleted, 5, TimeUnit.SECONDS),
            )
            assertTrue(databaseResult.get()?.isSuccess == true)
        } finally {
            releaseBatch.countDown()
            if (batchStarted.count == 0L) awaitWithMainLooper(batchCompleted, 5, TimeUnit.SECONDS)
            charging.close()
            runtime.close()
            host.pause().stop().destroy()
        }

        assertTrue(awaitWithMainLooper(batchCompleted, 5, TimeUnit.SECONDS))
        assertEquals("sent", batchResult.get()?.getOrThrow())
        assertEquals(1, callbackCount.get())
    }
}
