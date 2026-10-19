package lol.cutout.local

import lol.cutout.CameraWifiNetworkSelection
import lol.cutout.CameraPreviewTask
import lol.cutout.CameraPreviewTaskOwner
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import org.junit.Test
import org.junit.Assert.*

class LocalCameraWifiNetworkSelectionTest {
    @Test
    fun selectsWifiTransportIndependentlyOfTheSystemDefaultNetwork() {
        val selector = CameraWifiNetworkSelection<String>()
        selector.observe("cellular-default", hasWifiTransport = false)
        check(selector.current() == null)

        check(selector.observe("camera-wifi", hasWifiTransport = true) == "camera-wifi")
        // A system-default cellular route does not replace the selected Wi-Fi transport.
        check(selector.observe("cellular-default", hasWifiTransport = false) == "camera-wifi")
        check(selector.current() == "camera-wifi")

        check(selector.lost("camera-wifi") == null)
    }

    @Test
    fun retainsAnotherWifiNetworkWhenTheSelectedOneIsLost() {
        val selector = CameraWifiNetworkSelection<String>()
        selector.observe("camera-wifi", hasWifiTransport = true)
        check(selector.observe("other-wifi", hasWifiTransport = true) == "camera-wifi")
        check(selector.lost("camera-wifi") == "other-wifi")
    }

    @Test
    fun installRechecksCurrentnessAfterPublishingTaskBeforeItStarts() {
        val owner = CameraPreviewTaskOwner<String>()
        var ran = false
        val job = CoroutineScope(Dispatchers.Unconfined).launch(start = CoroutineStart.LAZY) { ran = true }
        val task = CameraPreviewTask(routeToken = 4, network = "camera-wifi", job = job)

        assertFalse(owner.install(task) { false })
        assertTrue("An invalidated task must be cancelled", job.isCancelled)
        assertFalse("The preview coroutine must not start after route invalidation", ran)
    }

    @Test
    fun aLostOldNetworkCannotCancelTheCurrentPreviewOnAnotherNetwork() {
        val owner = CameraPreviewTaskOwner<String>()
        val job = Job()
        val task = CameraPreviewTask(routeToken = 7, network = "current-wifi", job = job)
        assertTrue(owner.install(task) { true })

        assertFalse(owner.cancelForNetwork(7, "lost-wifi"))
        assertTrue("A stale network callback must not cancel the current preview", job.isActive)
        assertTrue(owner.cancelForNetwork(7, "current-wifi"))
        assertTrue(job.isCancelled)
    }

    @Test
    fun anOldRouteCallbackCannotCancelANewPreviewOnTheSameNetwork() {
        val owner = CameraPreviewTaskOwner<String>()
        val job = Job()
        val task = CameraPreviewTask(routeToken = 8, network = "same-wifi", job = job)
        assertTrue(owner.install(task) { true })

        assertFalse(owner.cancelForNetwork(7, "same-wifi"))
        assertTrue("A callback from the previous route must not cancel the reopened route", job.isActive)
        assertTrue(owner.cancelForNetwork(8, "same-wifi"))
        assertTrue(job.isCancelled)
    }
}
