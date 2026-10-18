package lol.cutout.local

import android.os.Looper
import org.robolectric.Shadows.shadowOf
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Drains callbacks posted to Robolectric's paused main looper during bounded async waits. */
internal fun awaitWithMainLooper(
    latch: CountDownLatch,
    timeout: Long,
    unit: TimeUnit,
): Boolean {
    val deadline = System.nanoTime() + unit.toNanos(timeout)
    val mainLooper = shadowOf(Looper.getMainLooper())
    while (latch.count > 0 && System.nanoTime() < deadline) {
        mainLooper.idle()
        if (latch.count > 0) {
            val remaining = deadline - System.nanoTime()
            if (remaining > 0) {
                latch.await(minOf(TimeUnit.MILLISECONDS.toNanos(10), remaining), TimeUnit.NANOSECONDS)
            }
        }
    }
    mainLooper.idle()
    return latch.count == 0L
}
