import java.nio.file.Files
import lol.cutout.smoke.checkCaptureHistoryBoundary
import lol.cutout.smoke.checkSessionAndCameraBoundary
import lol.cutout.smoke.checkEucCaptureReplay
import lol.cutout.smoke.checkNativeGattQueue
import lol.cutout.smoke.checkProductionSessionReplay

fun main() {
    val storage = Files.createTempDirectory("cutout-kotlin-smoke-").toFile()
    try {
        checkSessionAndCameraBoundary(storage)
        println("PASS session and camera boundary")
        checkCaptureHistoryBoundary(storage)
        println("PASS persistence reopen and typed failures")
        checkEucCaptureReplay()
        println("PASS EUC capture replay and connection replacement")
        checkNativeGattQueue()
        println("PASS production GATT queue, write limits and receipts")
        checkProductionSessionReplay()
        println("PASS production session adapter and stale callbacks")
    } finally {
        check(storage.deleteRecursively())
    }
}
