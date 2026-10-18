package lol.cutout.local

import lol.cutout.ffitest.RideHistoryControllerTest
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalRideHistoryControllerTest : RideHistoryControllerTest() {
    override fun awaitCallback(latch: CountDownLatch, timeout: Long, unit: TimeUnit): Boolean =
        awaitWithMainLooper(latch, timeout, unit)
}
