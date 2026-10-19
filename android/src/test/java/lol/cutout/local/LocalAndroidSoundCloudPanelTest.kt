package lol.cutout.local

import lol.cutout.ffitest.AndroidSoundCloudPanelTest
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalAndroidSoundCloudPanelTest : AndroidSoundCloudPanelTest() {
    override fun capturePanel(panel: android.view.View) {
        NativeSurfaceCapture.save(panel, "music-soundcloud-dark")
    }
}
