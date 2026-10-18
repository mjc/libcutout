import java.nio.file.Files
import lol.cutout.smoke.checkCaptureHistoryBoundary
import lol.cutout.smoke.checkSessionAndCameraBoundary

fun main() {
    val storage = Files.createTempDirectory("cutout-kotlin-smoke-").toFile()
    try {
        checkSessionAndCameraBoundary(storage)
        println("PASS session and camera boundary")
        checkCaptureHistoryBoundary(storage)
        println("PASS persistence reopen and typed failures")
    } finally {
        check(storage.deleteRecursively())
    }
}
