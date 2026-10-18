package lol.cutout.local

import lol.cutout.CutoutApplication
import java.io.File
import java.lang.management.ManagementFactory

/** Gives every Robolectric test in one JVM the same process-owned Rust database path. */
class LocalTestApplication : CutoutApplication() {
    override fun getDatabasePath(name: String): File {
        val processIdentity = ManagementFactory.getRuntimeMXBean().name
            .substringBefore('@')
            .map { if (it.isLetterOrDigit() || it == '-') it else '_' }
            .joinToString("")
        return File(
            File(System.getProperty("java.io.tmpdir"), "libcutout-android-tests-$processIdentity"),
            name,
        )
    }
}
