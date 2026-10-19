package lol.cutout.local

import android.app.Activity
import android.view.View
import android.widget.Switch
import android.widget.TextView
import lol.cutout.CutoutApplication
import lol.cutout.SetupPanel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalSetupContentParityTest {
    @Test
    fun setupUsesRideAutostartMeaningAndFullDiagnosticsPrivacyDisclosure() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val panel = SetupPanel(
            activity,
            activity.application as CutoutApplication,
            onMusic = {}, onCaptures = {}, onLocation = {}, onNotifications = {}, onOpenUrl = {},
        )
        activity.setContentView(panel)
        try {
            val autostart = checkNotNull(panel.findViewWithTag<Switch>("setup.ride-autostart"))
            assertEquals(panel.context.run { lol.cutout.CutoutUi(this).label("setup.ride_autostart") }, autostart.text.toString())
            val node = autostart.createAccessibilityNodeInfo()
            assertEquals(
                panel.context.run { lol.cutout.CutoutUi(this).label("setup.ride_autostart.hint") },
                node.hintText.toString(),
            )
            assertTrue("Autostart should be explained as starting a ride", node.hintText.toString().contains("Start a ride"))

            val diagnostics = checkNotNull(panel.findViewWithTag<TextView>("setup.diagnostics.explanation"))
            val disclosure = diagnostics.text.toString()
            assertTrue(disclosure.contains("device IDs and timing"))
            assertTrue(disclosure.contains("location"))
            assertTrue(disclosure.contains("not sent automatically"))
            assertTrue(disclosure.contains("Manual captures remain available"))
            assertEquals(View.VISIBLE, diagnostics.visibility)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }
}
