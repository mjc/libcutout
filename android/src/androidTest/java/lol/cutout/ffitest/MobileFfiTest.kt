package lol.cutout.ffitest

import android.os.Bundle
import android.os.Process
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.nio.file.Files
import lol.cutout.smoke.checkCaptureHistoryBoundary
import lol.cutout.smoke.checkControllerGeometryBoundary
import lol.cutout.smoke.checkSessionAndCameraBoundary
import lol.cutout.smoke.checkEucCaptureReplay
import lol.cutout.smoke.checkNativeGattQueue
import lol.cutout.smoke.checkProductionSessionReplay
import org.junit.Before
import org.junit.Test

class MobileFfiTest {
    @Before
    fun reportProcess() {
        val status = Bundle()
        status.putString("cutoutPid", Process.myPid().toString())
        InstrumentationRegistry.getInstrumentation().sendStatus(2, status)
    }

    @Test
    fun sessionAndCameraBoundary() = withStorage(::checkSessionAndCameraBoundary)

    @Test
    fun persistenceReopenAndTypedFailures() = withStorage(::checkCaptureHistoryBoundary)

    @Test
    fun controllerGeometryIsolationAndReopen() = withStorage(::checkControllerGeometryBoundary)

    @Test
    fun eucCaptureReplayAndConnectionReplacement() = checkEucCaptureReplay()

    @Test
    fun nativeGattQueueAndReceipts() = checkNativeGattQueue()

    @Test
    fun productionSessionReplayAndStaleCallbacks() = checkProductionSessionReplay()

    private fun withStorage(test: (File) -> Unit) {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val storage = Files.createTempDirectory(context.cacheDir.toPath(), "cutout-ffi-").toFile()
        try {
            test(storage)
        } finally {
            check(storage.deleteRecursively())
        }
    }
}
