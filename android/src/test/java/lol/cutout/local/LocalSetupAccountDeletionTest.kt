package lol.cutout.local

import android.app.Activity
import android.view.View
import android.widget.TextView
import lol.cutout.CutoutApplication
import lol.cutout.SetupPanel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.MobileCommunityAccountDeletionReceiptDto

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalSetupAccountDeletionTest {
    @Test
    fun onlySuccessfulDeletionReceiptNotifiesMapEvenAfterSetupDisposal() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        var completionCount = 0
        val panel = SetupPanel(
            activity,
            activity.application as CutoutApplication,
            onMusic = {},
            onCaptures = {},
            onLocation = {},
            onNotifications = {},
            onOpenUrl = {},
            onAccountDeleted = { completionCount++ },
        )
        activity.setContentView(panel)
        try {
            panel.accountDeletionCompleted(Result.failure(IllegalStateException("service unavailable")))
            assertEquals("A failed or unknown deletion must not refresh the map", 0, completionCount)
            assertEquals(View.VISIBLE, panel.findViewWithTag<TextView>("setup.account.error").visibility)

            // The durable Rust operation can finish after Setup has been left.
            // Its successful receipt still refreshes the retained charging map.
            panel.dispose()
            panel.accountDeletionCompleted(Result.success(MobileCommunityAccountDeletionReceiptDto(42UL)))
            assertEquals(1, completionCount)
            assertTrue("The detached Setup UI is not mutated after completion", panel.findViewWithTag<TextView>("setup.account.error").visibility == View.VISIBLE)
        } finally {
            panel.dispose()
            host.pause().stop().destroy()
        }
    }
}
