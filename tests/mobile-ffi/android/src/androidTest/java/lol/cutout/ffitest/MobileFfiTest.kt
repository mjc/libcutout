package lol.cutout.ffitest

import android.os.Bundle
import android.os.Process
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.nio.file.Files
import lol.cutout.smoke.checkCaptureHistoryBoundary
import lol.cutout.smoke.checkSessionAndCameraBoundary
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
