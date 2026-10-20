package lol.cutout.local

import lol.cutout.CutoutApplication
import java.io.File

/** Gives every Robolectric test in one JVM the same process-owned Rust database path. */
class LocalTestApplication : CutoutApplication() {
    override fun createRideMapSurface(context: android.content.Context): lol.cutout.RideMapSurface =
        lol.cutout.ffitest.TestRideMapSurface(context)

    override fun getDatabasePath(name: String): File {
        val artifacts = System.getProperty("cutout.ui.artifacts")
            ?: File(System.getProperty("java.io.tmpdir"), "libcutout-android-tests").absolutePath
        return File(File(artifacts, "database"), name)
    }
}
