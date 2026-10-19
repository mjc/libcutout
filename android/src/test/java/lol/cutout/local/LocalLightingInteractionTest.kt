package lol.cutout.local

import android.app.Activity
import android.content.Context
import android.os.Bundle
import android.view.InputDevice
import android.view.KeyEvent
import android.view.MotionEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityNodeProvider
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityManager
import android.view.View
import android.widget.FrameLayout
import android.widget.ListView
import android.widget.SeekBar
import android.os.SystemClock
import androidx.core.view.accessibility.AccessibilityNodeInfoCompat
import androidx.customview.widget.ExploreByTouchHelper
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.AndroidMelkLightingClient
import lol.cutout.CutoutUi
import lol.cutout.LightingPanel
import lol.cutout.LightingPresentation
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.Shadows
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import org.robolectric.shadows.ShadowAlertDialog
import uniffi.cutout_mobile_ffi.*

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-night-xhdpi", application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalLightingInteractionTest {
    @Test
    fun pairingDialogTracksRustCandidatesAndDisablesRowsAfterSelectionBegins() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val client = AndroidMelkLightingClient(activity.applicationContext)
        val selectedCandidates = mutableListOf<String>()
        val panel = LightingPanel(
            context = activity,
            client = client,
            onRequestBluetooth = {},
            submitPreset = client::savePreset,
            submitCandidateSelection = selectedCandidates::add,
        )
        // Settle the real initial client callback, then keep injected Rust presentation states isolated.
        client.observe(null)
        shadowOf(activity.mainLooper).idle()
        activity.setContentView(panel)
        try {
            val emptyScan = presentation(MobileMelkLightingSessionStateDto.Scanning)
            panel.render(emptyScan)
            shadowOf(activity.mainLooper).idle()
            checkNotNull(panel.findViewWithTag<View>("lighting.choose-accessory")).performClick()

            val dialog = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            val list = checkNotNull(dialog.window!!.decorView.findViewWithTag<ListView>("lighting.pairing.candidates"))
            val adapter = list.adapter
            assertEquals(0, adapter.count)

            val candidate = MobileMelkLightingSessionCandidateDto("stable-id", "MELK light", -48)
            panel.render(emptyScan.copy(candidates = listOf(candidate)))
            shadowOf(activity.mainLooper).idle()
            assertSame("The open pairing surface and its adapter should be retained", dialog,
                ShadowAlertDialog.getLatestAlertDialog())
            assertSame(adapter, list.adapter)
            assertEquals(1, adapter.count)
            assertTrue(adapter.isEnabled(0))
            val row = adapter.getView(0, null, list)
            assertEquals("lighting.candidate.stable-id", row.tag)
            assertTrue(row.contentDescription.toString().contains("MELK light"))
            assertTrue(row.contentDescription.toString().contains("-48 dBm"))

            val secondCandidate = MobileMelkLightingSessionCandidateDto("second-id", "Other light", -52)
            panel.render(emptyScan.copy(candidates = listOf(candidate, secondCandidate)))
            shadowOf(activity.mainLooper).idle()
            val visibleCandidateRow = adapter.getView(0, null, list)
            val firstStableId = adapter.getItemId(0)
            val secondStableId = adapter.getItemId(1)
            assertTrue("Candidate adapter IDs must remain unique", firstStableId != secondStableId)
            panel.render(emptyScan.copy(candidates = listOf(secondCandidate, candidate)))
            shadowOf(activity.mainLooper).idle()
            list.performItemClick(visibleCandidateRow, 0, adapter.getItemId(0))
            assertEquals("A live reorder must not redirect an already-bound row tap", listOf("stable-id"), selectedCandidates)

            val collisionA = MobileMelkLightingSessionCandidateDto("Aa", "First", -48)
            val collisionB = MobileMelkLightingSessionCandidateDto("BB", "Second", -49)
            panel.render(emptyScan.copy(candidates = listOf(collisionA, collisionB)))
            shadowOf(activity.mainLooper).idle()
            assertTrue("Distinct Rust identifiers with colliding String hashes keep unique row IDs",
                adapter.getItemId(0) != adapter.getItemId(1))

            panel.render(emptyScan.copy(
                snapshot = emptyScan.snapshot.copy(state = MobileMelkLightingSessionStateDto.Connecting),
                candidates = listOf(candidate.copy(rssi = -46)),
            ))
            shadowOf(activity.mainLooper).idle()
            assertSame(adapter, list.adapter)
            assertFalse("Rust session state makes candidate choices unavailable while connecting", adapter.isEnabled(0))
            assertTrue(dialog.isShowing)

            activity.setContentView(View(activity))
            assertFalse("A detached route must not leave its pairing dialog orphaned", dialog.isShowing)
        } finally {
            client.stop()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun accessibilityHueAndSaturationActionsCommitRatherThanLeaveOnlyPreviewState() {
        // Robolectric copies the WindowSession touch mode into ViewRootImpl when the Activity is created.
        InstrumentationRegistry.getInstrumentation().setInTouchMode(false)
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        Shadows.shadowOf(activity.getSystemService(AccessibilityManager::class.java)).apply {
            setEnabled(true)
            setTouchExplorationEnabled(true)
        }
        val client = AndroidMelkLightingClient(activity.applicationContext)
        val writes = mutableListOf<Pair<Triple<Int, Int, Int>, Boolean>>()
        lateinit var panel: LightingPanel
        panel = LightingPanel(
            activity,
            client,
            {},
            client::savePreset,
            submitSolidColor = { red, green, blue, preview ->
                writes += Triple(red, green, blue) to preview
                panel.render(presentation(
                    MobileMelkLightingSessionStateDto.Ready,
                    requested = MobileMelkLightingRestoreStateDto(
                        true, red.toUByte(), green.toUByte(), blue.toUByte(), 82u, MobileLightingPlaybackDto.Solid,
                    ),
                ))
            },
        )
        client.observe(null)
        shadowOf(activity.mainLooper).idle()
        val accessibilityHost = RecordingAccessibilityFrameLayout(activity).apply { addView(panel) }
        activity.setContentView(accessibilityHost)
        try {
            val requested = MobileMelkLightingRestoreStateDto(
                true, 255u, 0u, 0u, 82u, MobileLightingPlaybackDto.Solid,
            )
            panel.render(presentation(MobileMelkLightingSessionStateDto.Ready, requested = requested))
            NativeSurfaceCapture.save(accessibilityHost, "lighting-color-wheel-accessibility")
            val wheel = checkNotNull(panel.findViewWithTag<View>("lighting.color-wheel"))
            assertEquals("Color controls stay wheel-only in the visible layout", null,
                panel.findViewWithTag<View>("lighting.hue"))
            assertEquals("Color controls stay wheel-only in the visible layout", null,
                panel.findViewWithTag<View>("lighting.saturation"))
            val provider: AccessibilityNodeProvider = checkNotNull(wheel.accessibilityNodeProvider)
            val action = AccessibilityNodeInfoCompat.AccessibilityActionCompat.ACTION_SET_PROGRESS.id
            fun node(id: Int) = AccessibilityNodeInfoCompat.wrap(
                checkNotNull(provider.createAccessibilityNodeInfo(id)),
            )
            val hostNode = node(ExploreByTouchHelper.HOST_ID)
            assertEquals(CutoutUi(activity).label("lighting.color_wheel"), hostNode.contentDescription)
            assertEquals(CutoutUi(activity).label("lighting.color_wheel.hint"), hostNode.hintText)
            assertTrue("The wheel announces its current hue and saturation", hostNode.stateDescription.toString().contains("°"))
            val hueNode = node(1)
            val saturationNode = node(2)
            assertEquals(CutoutUi(activity).label("lighting.hue"), hueNode.contentDescription)
            assertEquals(CutoutUi(activity).label("lighting.saturation"), saturationNode.contentDescription)
            assertEquals(AccessibilityNodeInfoCompat.RangeInfoCompat.RANGE_TYPE_FLOAT, hueNode.rangeInfo.type)
            assertEquals(0f, hueNode.rangeInfo.min)
            assertEquals(1f, hueNode.rangeInfo.max)
            val hueBounds = android.graphics.Rect()
            val saturationBounds = android.graphics.Rect()
            val wheelBounds = android.graphics.Rect()
            hueNode.getBoundsInScreen(hueBounds)
            saturationNode.getBoundsInScreen(saturationBounds)
            assertTrue("The color wheel has a visible screen rectangle", wheel.getGlobalVisibleRect(wheelBounds))
            assertTrue("Hue has a nonempty on-screen exploration target", !hueBounds.isEmpty)
            assertTrue("Saturation has a nonempty on-screen exploration target", !saturationBounds.isEmpty)
            assertTrue("Hue target is clipped to the visible wheel", wheelBounds.contains(hueBounds))
            assertTrue("Saturation target is clipped to the visible wheel", wheelBounds.contains(saturationBounds))
            assertTrue("Virtual controls have distinct touch-exploration regions", hueBounds.right <= saturationBounds.left)
            assertTrue(hueNode.isFocusable)
            assertTrue(saturationNode.isFocusable)
            assertFalse("The attached native root is in keyboard-navigation mode", wheel.isInTouchMode)

            val hueLabel = CutoutUi(activity).label("lighting.hue")
            fun keyboardFocusLabel(): CharSequence? = provider.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
                ?.let { AccessibilityNodeInfoCompat.wrap(it).contentDescription }
            if (keyboardFocusLabel() != hueLabel) {
                val accepted = provider.performAction(1, AccessibilityNodeInfoCompat.ACTION_FOCUS, null)
                assertTrue(
                    "Hue receives native virtual keyboard focus " +
                        "(attached=${wheel.isAttachedToWindow}, focusable=${wheel.isFocusable}, " +
                        "focusableInTouchMode=${wheel.isFocusableInTouchMode}, inTouchMode=${wheel.isInTouchMode}, " +
                        "currentFocus=${keyboardFocusLabel()})",
                    accepted,
                )
            }
            assertEquals(CutoutUi(activity).label("lighting.hue"),
                keyboardFocusLabel())
            assertTrue("DPAD navigation advances between virtual controls", wheel.dispatchKeyEvent(
                KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_DPAD_RIGHT),
            ))
            assertEquals(CutoutUi(activity).label("lighting.saturation"),
                provider.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)?.let { AccessibilityNodeInfoCompat.wrap(it).contentDescription })
            provider.performAction(2, AccessibilityNodeInfoCompat.ACTION_CLEAR_FOCUS, null)

            assertTrue(provider.performAction(1, AccessibilityNodeInfoCompat.ACTION_FOCUS, null))
            panel.render(presentation(
                MobileMelkLightingSessionStateDto.Ready,
                requested = MobileMelkLightingRestoreStateDto(
                    true, 220u, 20u, 30u, 82u, MobileLightingPlaybackDto.Solid,
                ),
            ))
            shadowOf(activity.mainLooper).idle()
            assertSame("Keyboard focus retains the existing wheel through incoming Rust state", wheel,
                panel.findViewWithTag<View>("lighting.color-wheel"))
            assertEquals(CutoutUi(activity).label("lighting.hue"),
                provider.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)?.let { AccessibilityNodeInfoCompat.wrap(it).contentDescription })

            val centerY = wheel.height / 2f
            val eventTime = SystemClock.uptimeMillis()
            fun hover(action: Int, x: Float): Boolean {
                val event = MotionEvent.obtain(eventTime, eventTime, action, x, centerY, 0).apply {
                    source = InputDevice.SOURCE_TOUCHSCREEN
                }
                return try {
                    wheel.dispatchGenericMotionEvent(event)
                } finally {
                    event.recycle()
                }
            }
            assertTrue("Touch exploration can discover the hue virtual target", hover(
                MotionEvent.ACTION_HOVER_ENTER, wheel.width * 0.25f,
            ))
            assertTrue(accessibilityHost.events.any {
                it.eventType == AccessibilityEvent.TYPE_VIEW_HOVER_ENTER &&
                    it.contentDescription == CutoutUi(activity).label("lighting.hue")
            })
            assertTrue("Touch exploration can discover the saturation virtual target", hover(
                MotionEvent.ACTION_HOVER_MOVE, wheel.width * 0.75f,
            ))
            assertTrue(accessibilityHost.events.any {
                it.eventType == AccessibilityEvent.TYPE_VIEW_HOVER_ENTER &&
                    it.contentDescription == CutoutUi(activity).label("lighting.saturation")
            })
            hover(MotionEvent.ACTION_HOVER_EXIT, wheel.width * 0.75f)

            assertTrue(provider.performAction(1, AccessibilityNodeInfoCompat.ACTION_ACCESSIBILITY_FOCUS, null))
            assertTrue(provider.performAction(1, action, progressArguments(1f / 3f)))
            assertTrue("Hue accessibility adjustment must produce a durable color command", writes.isNotEmpty())
            assertFalse("Accessibility adjustment may not remain a coalesced preview", writes.last().second)
            assertTrue("The wheel host exposes its updated value after the virtual action",
                node(ExploreByTouchHelper.HOST_ID).stateDescription.toString().contains("119°"))
            assertSame("Focused virtual controls retain the wheel while Rust snapshots arrive", wheel,
                panel.findViewWithTag<View>("lighting.color-wheel"))

            writes.clear()
            assertTrue(provider.performAction(1, AccessibilityNodeInfoCompat.ACTION_CLEAR_ACCESSIBILITY_FOCUS, null))
            assertTrue(provider.performAction(2, AccessibilityNodeInfoCompat.ACTION_ACCESSIBILITY_FOCUS, null))
            assertTrue(provider.performAction(2, action, progressArguments(0.4f)))
            assertTrue("Saturation accessibility adjustment must produce a durable color command", writes.isNotEmpty())
            assertFalse("Accessibility adjustment may not remain a coalesced preview", writes.last().second)
            assertTrue(provider.performAction(2, AccessibilityNodeInfoCompat.ACTION_SCROLL_FORWARD, null))
            assertFalse("Virtual slider scroll adjustment must also commit", writes.last().second)
            assertTrue(provider.performAction(2, AccessibilityNodeInfoCompat.ACTION_CLEAR_ACCESSIBILITY_FOCUS, null))

            if (keyboardFocusLabel() != hueLabel) {
                assertTrue(provider.performAction(1, AccessibilityNodeInfoCompat.ACTION_FOCUS, null))
            }
            assertTrue(provider.performAction(2, AccessibilityNodeInfoCompat.ACTION_ACCESSIBILITY_FOCUS, null))
            assertTrue("Detach setup holds keyboard focus", provider.findFocus(AccessibilityNodeInfo.FOCUS_INPUT) != null)
            assertTrue("Detach setup holds accessibility focus", provider.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY) != null)
            accessibilityHost.removeView(panel)
            assertEquals("Detaching clears keyboard virtual focus", null,
                provider.findFocus(AccessibilityNodeInfo.FOCUS_INPUT))
            assertEquals("Detaching clears accessibility virtual focus", null,
                provider.findFocus(AccessibilityNodeInfo.FOCUS_ACCESSIBILITY))
        } finally {
            InstrumentationRegistry.getInstrumentation().setInTouchMode(true)
            client.stop()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun accessibilityActionsCommitBrightnessEffectSpeedAndMusicSensitivity() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val client = AndroidMelkLightingClient(activity.applicationContext)
        val brightnessReceipts = mutableListOf<Int>()
        val speedReceipts = mutableListOf<Pair<Int, Int?>>()
        val musicReceipts = mutableListOf<MobileLightingPlaybackDto>()
        val panel = LightingPanel(
            context = activity,
            client = client,
            onRequestBluetooth = {},
            submitPreset = client::savePreset,
            submitEffectSpeed = { value, pattern -> speedReceipts += value to pattern },
            submitBrightness = brightnessReceipts::add,
            submitMusicPlayback = musicReceipts::add,
        )
        activity.setContentView(panel)
        try {
            val effect = MobileMelkLightingRestoreStateDto(
                true, 255u, 0u, 0u, 82u, MobileLightingPlaybackDto.Effect(7u, 96u),
            )
            panel.render(presentation(MobileMelkLightingSessionStateDto.Ready, requested = effect))
            val brightness = checkNotNull(panel.findViewWithTag<SeekBar>("lighting.brightness"))
            assertTrue(brightness.performAccessibilityAction(setProgressAction(), progressArguments(61f)))
            assertEquals("Brightness accessibility input reaches the production command adapter", listOf(61), brightnessReceipts)

            checkNotNull(panel.findViewWithTag<View>("lighting.tab.effects")).performClick()
            val speed = checkNotNull(panel.findViewWithTag<SeekBar>("lighting.effect.speed"))
            assertTrue(speed.performAccessibilityAction(setProgressAction(), progressArguments(140f)))
            assertEquals("Effect speed remains bound to the selected pattern", listOf(115 to 7), speedReceipts)

            val music = MobileMelkLightingRestoreStateDto(
                true, 255u, 0u, 0u, 82u, MobileLightingPlaybackDto.Music(3u, 20u),
            )
            panel.render(presentation(MobileMelkLightingSessionStateDto.Ready, requested = music))
            checkNotNull(panel.findViewWithTag<View>("lighting.tab.music")).performClick()
            val sensitivity = checkNotNull(panel.findViewWithTag<SeekBar>("lighting.music.sensitivity"))
            assertTrue(sensitivity.performAccessibilityAction(setProgressAction(), progressArguments(73f)))
            assertEquals(
                "Music sensitivity accessibility input reaches the production command adapter",
                listOf(MobileLightingPlaybackDto.Music(3u, 73u)), musicReceipts,
            )
        } finally {
            client.stop()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun colorWheelRemainsTheOnlyVisibleColorControlAndSpeedTargetIsFenced() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val client = AndroidMelkLightingClient(activity.applicationContext)
        val panel = LightingPanel(activity, client) {}
        activity.setContentView(panel)
        try {
            val requested = MobileMelkLightingRestoreStateDto(
                true, 255u, 0u, 0u, 82u, MobileLightingPlaybackDto.Effect(7u, 96u),
            )
            panel.render(presentation(MobileMelkLightingSessionStateDto.Ready, requested = requested))
            val wheel = checkNotNull(panel.findViewWithTag<View>("lighting.color-wheel"))
            assertEquals(activity.resources.displayMetrics.density.toInt() * 248, wheel.layoutParams.height)
            assertEquals(null, panel.findViewWithTag<View>("lighting.hue"))
            assertEquals(null, panel.findViewWithTag<View>("lighting.saturation"))

            val originalPattern = (requested.playback as MobileLightingPlaybackDto.Effect).pattern.toInt()
            assertEquals(7, originalPattern)
            assertTrue(client.effectSpeedTargets(originalPattern, requested.playback as MobileLightingPlaybackDto.Effect))
            assertFalse(client.effectSpeedTargets(originalPattern,
                MobileLightingPlaybackDto.Effect(8u, 96u)))
            assertFalse(client.effectSpeedTargets(null, requested.playback as MobileLightingPlaybackDto.Effect))
        } finally {
            client.stop()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun effectSpeedGestureSubmitsItsOriginalPatternAfterPresentationChanges() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val client = AndroidMelkLightingClient(activity.applicationContext)
        val commands = mutableListOf<Pair<Int, Int?>>()
        val panel = LightingPanel(activity, client, {}, client::savePreset) { speed, pattern ->
            commands += speed to pattern
        }
        activity.setContentView(panel)
        try {
            val stateFor = { pattern: Int ->
                MobileMelkLightingRestoreStateDto(
                    true, 255u, 0u, 0u, 82u, MobileLightingPlaybackDto.Effect(pattern.toUByte(), 96u),
                )
            }
            panel.render(presentation(MobileMelkLightingSessionStateDto.Ready, requested = stateFor(7)))
            checkNotNull(panel.findViewWithTag<View>("lighting.tab.effects")).performClick()
            val speed = checkNotNull(panel.findViewWithTag<SeekBar>("lighting.effect.speed"))
            val density = activity.resources.displayMetrics.density
            val width = (300 * density).toInt()
            val height = (48 * density).toInt()
            speed.measure(
                View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
            )
            speed.layout(0, 0, width, height)
            val start = SystemClock.uptimeMillis()
            speed.dispatchTouchEvent(MotionEvent.obtain(start, start, MotionEvent.ACTION_DOWN, width * 0.2f, height / 2f, 0))
            speed.dispatchTouchEvent(MotionEvent.obtain(start, start + 20, MotionEvent.ACTION_MOVE, width * 0.7f, height / 2f, 0))

            panel.render(presentation(MobileMelkLightingSessionStateDto.Ready, requested = stateFor(8)))
            speed.dispatchTouchEvent(MotionEvent.obtain(start, start + 40, MotionEvent.ACTION_UP, width * 0.7f, height / 2f, 0))

            assertEquals("One completed gesture should carry the pattern that was active at gesture start",
                listOf((255 - speed.progress) to 7), commands)
            assertFalse(client.effectSpeedTargets(commands.single().second,
                MobileLightingPlaybackDto.Effect(8u, 96u)))
        } finally {
            client.stop()
            host.pause().stop().destroy()
        }
    }

    private fun presentation(
        state: MobileMelkLightingSessionStateDto,
        requested: MobileMelkLightingRestoreStateDto = MobileMelkLightingRestoreStateDto(
            false, 255u, 0u, 0u, 100u, MobileLightingPlaybackDto.Solid,
        ),
        candidates: List<MobileMelkLightingSessionCandidateDto> = emptyList(),
    ) = LightingPresentation(
        snapshot = MobileMelkLightingSessionSnapshotDto(state, null, null, 0u, state == MobileMelkLightingSessionStateDto.Ready),
        candidates = candidates,
        pairedIdentifier = null,
        alias = null,
        associatedVehicle = null,
        restoreEnabled = false,
        requested = requested,
        confirmed = requested,
        presets = emptyList(),
        permissionMissing = false,
    )

    private fun setProgressAction() = AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_PROGRESS.id
    private fun progressArguments(value: Float) = Bundle().apply {
        putFloat(AccessibilityNodeInfo.ACTION_ARGUMENT_PROGRESS_VALUE, value)
    }

    private class RecordingAccessibilityFrameLayout(context: Context) : FrameLayout(context) {
        val events = mutableListOf<AccessibilityEvent>()
        override fun requestSendAccessibilityEvent(child: View, event: AccessibilityEvent): Boolean {
            events += AccessibilityEvent.obtain(event)
            return true
        }
    }
}
