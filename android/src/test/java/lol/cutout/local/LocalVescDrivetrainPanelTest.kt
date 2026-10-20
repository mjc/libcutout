package lol.cutout.local

import android.app.Activity
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import lol.cutout.*
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowAlertDialog
import uniffi.cutout_mobile_ffi.*
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
class LocalVescDrivetrainPanelTest {
    /** Host UI proof with simulated BLE observations; selection and saves use production owners. */
    @Test
    @Config(qualifiers = "w430dp-h932dp-night-xhdpi")
    @org.robolectric.annotation.GraphicsMode(org.robolectric.annotation.GraphicsMode.Mode.NATIVE)
    fun mainActivitySelectsEskateOpensTuneAndReopensTheDurableDrivetrain() {
        val host = Robolectric.buildActivity(MainActivity::class.java).setup().visible()
        var activity = host.get()
        val app = activity.application as CutoutApplication
        val client = app.bluetooth
        // Only the BLE observation is simulated. The Activity, dialog, native callbacks,
        // Rust session and database owner remain the production instances.
        client.observe(null)
        client.stopScan()
        awaitResult<Boolean> { client.diagnosticRecordingEnabled(it) }.getOrThrow()
        val state = awaitResult<CutoutSessionStateHandle> { done ->
            app.rides.withDatabase({ app.sessionStateForNativeOwner() }, done)
        }.getOrThrow()
        val id = "android-main-flow-${UUID.randomUUID()}"
        val token = standardVesc(state, id)
        fun observeFixture() {
            val current = checkNotNull(state.deviceSessionSnapshot().connection.token)
            val frame = BlePresentation(session = state.deviceSessionSnapshot(),
                telemetry = state.connectionTelemetrySnapshot(current), settings = state.settings(),
                connectionCandidate = state.connectionAdmissionCandidate(current.platformIdentifier, "VESC", false))
            // Recreation reads the application-owned BLE frame before attaching the
            // Activity observer. Simulate that retained observation at its owner too.
            val owner = org.robolectric.util.ReflectionHelpers.getField<android.os.Handler>(client, "owner")
            awaitResult<Unit> { done ->
                check(owner.post {
                    done(runCatching {
                        org.robolectric.util.ReflectionHelpers.setField(client, "view", frame)
                        org.robolectric.util.ReflectionHelpers.setField(client, "latestPresentation", frame)
                    })
                })
            }.getOrThrow()
            activity.render(frame)
        }
        try {
            observeFixture()
            val candidate = state.connectionAdmissionCandidate(token.platformIdentifier, "VESC", false)
            assertEquals(DiscoveryConnectionRoute.VESC, candidate.connectionRoute)
            assertNull(candidate.electricUnicycleModel)
            clickNavigation(activity, R.string.ride)
            val choose = checkNotNull(activity.window.decorView.findViewWithTag<View>("ride.choose-vehicle-kind"))
            assertTrue(choose.isShown)
            assertTrue(choose.performClick())
            val dialog = checkNotNull(ShadowAlertDialog.getLatestAlertDialog())
            assertTrue(dialog.isShowing)
            awaitCondition("Vehicle choices did not finish showing") { dialog.listView.tag == "device.type.selection" }
            assertEquals("device.type.selection", dialog.listView.tag)
            val index = userSelectableVehicleKinds().indexOf(MobileVehicleKindDto.ESKATE)
            assertTrue(index >= 0)
            val choices = dialog.listView.adapter
            val choice = choices.getView(index, null, dialog.listView)
            assertEquals("device.type.choice.eskate", choice.tag)
            assertTrue(dialog.listView.performItemClick(choice, index, choices.getItemId(index)))
            awaitCondition("Vehicle type was not committed by the dialog") {
                state.deviceSessionSnapshot().identity?.vehicleKind == MobileVehicleKindDto.ESKATE
            }
            assertEquals(MobileVehicleKindDto.ESKATE, awaitResult<MobileVehicleKindDto?> { done ->
                app.rides.withDatabase({ it.userVehicleKind(id) }, done)
            }.getOrThrow())
            observeFixture()
            val speed = checkNotNull(activity.window.decorView.findViewWithTag<TextView>("ride.hero.speed"))
            assertTrue(speed.isShown)
            assertEquals(CutoutUi(activity).label("ride.hero.value.unavailable"), speed.text.toString())
            assertEquals("--", speed.text.toString())
            NativeSurfaceCapture.save(activity.window.decorView, "host-main-eskate-ride-missing-geometry",
                widthDp = 430, heightDp = 932)
            val openTune = checkNotNull(activity.window.decorView.findViewWithTag<View>("ride.open-tune"))
            assertTrue(openTune.isShown)
            assertTrue(openTune.performClick())
            val editor = checkNotNull(activity.window.decorView.findViewWithTag<View>("vesc.drivetrain.editor"))
            assertTrue(editor.isShown)
            awaitEnabled(editor)
            field(editor, "wheel-diameter").setText("90.125")
            field(editor, "motor-pole-pairs").setText("7")
            field(editor, "reduction").setText("2.750")
            assertTrue(editor.findViewWithTag<Button>("vesc.drivetrain.save").performClick())
            awaitEnabled(editor)
            val expected = MobileVescDrivetrainDraft("90.125", "7", "2.75")
            assertEquals(expected, state.connectionVescDrivetrain(token))
            assertEquals("2.75", field(editor, "reduction").text.toString())
            NativeSurfaceCapture.save(activity.window.decorView, "host-main-eskate-tune-saved",
                widthDp = 430, heightDp = 932)

            // A new connection clears the Rust in-memory profile. Its real Tune loader
            // must recover geometry from the app database, rather than a view draft.
            val reconnected = standardVesc(state, id)
            assertNull(state.connectionVescDrivetrain(reconnected))
            awaitResult<Unit> { done ->
                app.rides.withDatabase({ database ->
                    val savedKind = checkNotNull(database.userVehicleKind(id))
                    check(state.restoreConnectionVehicleKind(reconnected, savedKind))
                }, done)
            }.getOrThrow()
            observeFixture()
            awaitEnabled(editor)
            assertEquals(expected, state.connectionVescDrivetrain(reconnected))
            assertEquals("90.125", field(editor, "wheel-diameter").text.toString())
            assertEquals("2.75", field(editor, "reduction").text.toString())

            host.recreate()
            activity = host.get()
            assertSame(app, activity.application)
            client.observe(null)
            client.stopScan()
            awaitResult<Boolean> { client.diagnosticRecordingEnabled(it) }.getOrThrow()
            observeFixture()
            val recreated = checkNotNull(activity.window.decorView.findViewWithTag<View>("vesc.drivetrain.editor"))
            assertNotSame(editor, recreated)
            assertTrue(recreated.isShown)
            awaitEnabled(recreated)
            assertEquals("90.125", field(recreated, "wheel-diameter").text.toString())
            assertEquals("7", field(recreated, "motor-pole-pairs").text.toString())
            assertEquals("2.75", field(recreated, "reduction").text.toString())
            clickNavigation(activity, R.string.ride)
            assertTrue(activity.window.decorView.findViewWithTag<View>("ride.open-tune").performClick())
            assertTrue(recreated.isShown)
            assertEquals(expected, state.connectionVescDrivetrain(reconnected))
        } finally {
            client.observe(null)
            state.disconnectConnectionAttempt()
            client.disconnect()
            awaitResult<Boolean> { client.diagnosticRecordingEnabled(it) }.getOrThrow()
            host.pause().stop().destroy()
        }
    }

    @Test
    fun fractionalDraftUsesRustValidationAndReopensFromTheProductionDatabaseOwner() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        val app = activity.application as CutoutApplication
        val client = AndroidBleClient(activity)
        val state = awaitResult<CutoutSessionStateHandle> { done ->
            app.rides.withDatabase({ app.sessionStateForNativeOwner() }, done)
        }.getOrThrow()
        val token = standardVesc(state, "android-drivetrain-${UUID.randomUUID()}")
        try {
            val panel = DrivetrainPanel(activity, client::loadVescDrivetrain, client::saveVescDrivetrain)
            activity.setContentView(panel)
            panel.bind(token)
            awaitEnabled(panel)
            field(panel, "wheel-diameter").setText("90.125")
            field(panel, "motor-pole-pairs").setText("7")
            field(panel, "reduction").setText("2.750")
            panel.findViewWithTag<Button>("vesc.drivetrain.save").performClick()
            awaitEnabled(panel)
            assertEquals("90.125", field(panel, "wheel-diameter").text.toString())
            assertEquals("2.75", field(panel, "reduction").text.toString())
            val saved = awaitResult<MobileVescDrivetrainDraft?> { client.loadVescDrivetrain(token, it) }.getOrThrow()
            assertEquals("90.125", saved?.wheelDiameterMm)
            field(panel, "reduction").setText("0")
            panel.findViewWithTag<Button>("vesc.drivetrain.save").performClick()
            awaitEnabled(panel)
            assertNotNull(field(panel, "reduction").error)
            assertEquals(saved, awaitResult<MobileVescDrivetrainDraft?> { client.loadVescDrivetrain(token, it) }.getOrThrow())

            // A fresh attempt loses its in-memory geometry and must use the same app database.
            val reconnected = standardVesc(state, token.platformIdentifier)
            val reopened = DrivetrainPanel(activity, client::loadVescDrivetrain, client::saveVescDrivetrain)
            activity.setContentView(reopened)
            reopened.bind(reconnected)
            awaitEnabled(reopened)
            assertEquals("90.125", field(reopened, "wheel-diameter").text.toString())
            assertEquals("2.75", field(reopened, "reduction").text.toString())
            assertTrue(awaitResult<MobileVescDrivetrainDraft> { done ->
                client.saveVescDrivetrain(token, MobileVescDrivetrainDraft("100", "7", "3"), done)
            }.exceptionOrNull() is MobileVescDrivetrainException.ConnectionUnavailable)
        } finally {
            state.disconnectConnectionAttempt()
            val closed = CountDownLatch(1)
            client.close { closed.countDown() }
            assertTrue(awaitWithMainLooper(closed, 10, TimeUnit.SECONDS))
            host.pause().stop().destroy()
        }
    }

    @Test
    fun typeSelectionUsesRustChoicesAndPersistsOnlyTheRequestedDevice() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val app = host.get().application as CutoutApplication
        val client = AndroidBleClient(host.get())
        val state = awaitResult<CutoutSessionStateHandle> { done ->
            app.rides.withDatabase({ app.sessionStateForNativeOwner() }, done)
        }.getOrThrow()
        val id = "android-type-${UUID.randomUUID()}"
        val token = standardVesc(state, id)
        try {
            assertEquals(MobileVehicleKindDto.UNKNOWN, state.deviceSessionSnapshot().identity?.vehicleKind)
            assertTrue(userSelectableVehicleKinds().contains(MobileVehicleKindDto.ESKATE))
            val selected = awaitResult<MobileDeviceSessionSnapshotDto> {
                client.selectVehicleKind(token, MobileVehicleKindDto.ESKATE, it)
            }.getOrThrow()
            assertEquals(MobileVehicleKindDto.ESKATE, selected.identity?.vehicleKind)
            assertEquals(MobileVehicleClassificationProvenanceDto.USER_PROVIDED, selected.identity?.classificationProvenance)
            assertEquals(MobileVehicleKindDto.ESKATE, awaitResult<MobileVehicleKindDto?> { done ->
                app.rides.withDatabase({ it.userVehicleKind(id) }, done)
            }.getOrThrow())
            val replacement = standardVesc(state, "android-other-${UUID.randomUUID()}")
            assertTrue(awaitResult<MobileDeviceSessionSnapshotDto> {
                client.selectVehicleKind(token, MobileVehicleKindDto.EBIKE, it)
            }.isFailure)
            assertEquals(replacement, state.deviceSessionSnapshot().connection.token)
            assertEquals(MobileVehicleKindDto.UNKNOWN, state.deviceSessionSnapshot().identity?.vehicleKind)
            assertNull(awaitResult<MobileVehicleKindDto?> { done ->
                app.rides.withDatabase({ it.userVehicleKind(replacement.platformIdentifier) }, done)
            }.getOrThrow())
        } finally {
            state.disconnectConnectionAttempt()
            val closed = CountDownLatch(1)
            client.close { closed.countDown() }
            assertTrue(awaitWithMainLooper(closed, 10, TimeUnit.SECONDS))
            host.pause().stop().destroy()
        }
    }

    @Test
    fun tuneKeepsDrivetrainEditorsAndUserDraftWhenSpeedArrives() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        try {
            CutoutSessionStateHandle().use { state ->
                standardVesc(state, "tune-persistence")
                var choicesOpened = 0
                val panel = SettingsPanel(host.get(), "test-owner",
                    onChooseVehicleKind = { choicesOpened++ },
                    loadDrivetrain = { _, done -> done(Result.success(MobileVescDrivetrainDraft("90.125", "7", "2.75"))) },
                    saveDrivetrain = { _, draft, done -> done(Result.success(draft)) },
                    submit = { _, _, _, _ -> })
                host.get().setContentView(panel)
                val value = BlePresentation(session = state.deviceSessionSnapshot())
                panel.renderVehicleSetup(value)
                val editor = checkNotNull(panel.findViewWithTag<View>("vesc.drivetrain.editor"))
                val diameter = field(editor, "wheel-diameter")
                diameter.setText("105.5")
                assertTrue(editor.isShown)
                assertTrue(panel.findViewWithTag<View>("settings.choose-vehicle-kind").performClick())
                assertEquals(1, choicesOpened)
                val measuredSpeed = lol.cutout.ffitest.productionReplayPresentation().telemetry
                assertNotNull(measuredSpeed?.speed)
                panel.renderVehicleSetup(value.copy(telemetry = measuredSpeed))
                assertSame(editor, panel.findViewWithTag<View>("vesc.drivetrain.editor"))
                assertTrue(editor.isShown)
                assertSame(diameter, field(editor, "wheel-diameter"))
                assertEquals("105.5", diameter.text.toString())
            }
        } finally { host.pause().stop().destroy() }
    }

    @Test
    fun loadFailureRequiresRetryAndLateCompletionCannotReplaceAnotherConnectionDraft() {
        val host = Robolectric.buildActivity(Activity::class.java).setup().visible()
        val activity = host.get()
        try {
            val first = MobileConnectionAttemptTokenDto(platformIdentifier = "first", generation = 1UL)
            val second = MobileConnectionAttemptTokenDto(platformIdentifier = "second", generation = 2UL)
            val loads = mutableListOf<(Result<MobileVescDrivetrainDraft?>) -> Unit>()
            val saves = mutableListOf<(Result<MobileVescDrivetrainDraft>) -> Unit>()
            val panel = DrivetrainPanel(activity,
                load = { _, completed -> loads += completed },
                save = { _, _, completed -> saves += completed })
            activity.setContentView(panel)
            panel.bind(first)
            loads.removeAt(0)(Result.failure(IllegalStateException("storage unavailable")))
            assertFalse(panel.findViewWithTag<Button>("vesc.drivetrain.save").isEnabled)
            assertTrue(panel.findViewWithTag<View>("vesc.drivetrain.retry-load").isShown)
            panel.findViewWithTag<View>("vesc.drivetrain.retry-load").performClick()
            loads.removeAt(0)(Result.success(null))
            field(panel, "wheel-diameter").setText("90.125")
            field(panel, "motor-pole-pairs").setText("7")
            field(panel, "reduction").setText("2.750")
            panel.findViewWithTag<View>("vesc.drivetrain.save").performClick()
            panel.bind(second)
            loads.removeAt(0)(Result.success(MobileVescDrivetrainDraft("105.5", "10", "1.125")))
            saves.removeAt(0)(Result.success(MobileVescDrivetrainDraft("90.125", "7", "2.750")))
            assertEquals("105.5", field(panel, "wheel-diameter").text.toString())
            assertEquals("1.125", field(panel, "reduction").text.toString())
            assertTrue(panel.findViewWithTag<Button>("vesc.drivetrain.save").isEnabled)
        } finally { host.pause().stop().destroy() }
    }

    private fun clickNavigation(activity: Activity, label: Int) {
        fun descendants(view: View): Sequence<View> = sequence {
            yield(view)
            if (view is ViewGroup) for (index in 0 until view.childCount) yieldAll(descendants(view.getChildAt(index)))
        }
        val target = descendants(activity.window.decorView).single {
            it.isShown && it.isClickable && it.contentDescription?.toString() == activity.getString(label)
        }
        assertTrue(target.performClick())
    }

    private fun awaitCondition(message: String, condition: () -> Boolean) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10)
        val main = org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper())
        val pause = CountDownLatch(1)
        while (!condition() && System.nanoTime() < deadline) {
            main.idle()
            if (!condition()) pause.await(10, TimeUnit.MILLISECONDS)
        }
        main.idle()
        assertTrue(message, condition())
    }

    private fun field(panel: View, name: String): EditText = checkNotNull(panel.findViewWithTag("vesc.drivetrain.$name"))

    private fun awaitEnabled(panel: View) {
        val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(10)
        val main = org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper())
        val pause = CountDownLatch(1)
        while (!panel.findViewWithTag<Button>("vesc.drivetrain.save").isEnabled && System.nanoTime() < deadline) {
            main.idle()
            if (!panel.findViewWithTag<Button>("vesc.drivetrain.save").isEnabled) pause.await(10, TimeUnit.MILLISECONDS)
        }
        main.idle()
        assertTrue("Drivetrain callback did not complete", panel.findViewWithTag<Button>("vesc.drivetrain.save").isEnabled)
    }

    private fun <T> awaitResult(start: ((Result<T>) -> Unit) -> Unit): Result<T> {
        val complete = CountDownLatch(1)
        var result: Result<T>? = null
        start { result = it; complete.countDown() }
        check(awaitWithMainLooper(complete, 10, TimeUnit.SECONDS))
        return checkNotNull(result)
    }

    private fun standardVesc(state: CutoutSessionStateHandle, id: String): MobileConnectionAttemptTokenDto {
        var clock = 1UL
        val token = checkNotNull(state.beginConnectionAttempt(id, 0UL).token)
        state.connectionLinkEstablished(token)
        val outputs = mutableListOf<MobileSessionOutputDto>()
        val session = PevSession(state, token, { clock }, { outputs += it }) { _, _, _ -> }
        session.notification(byteArrayOf(), byteArrayOf(2, 20, 157.toByte(), 7, 1, 2, 97, 98, 99, 49, 50, 51, 0,
            117, 115, 101, 114, 104, 97, 115, 104, 0, 38, 208.toByte(), 3))
        val query = outputs.first { it.applicationQuery != null }
        session.writeReceipt(query, NativeReceipt.SUBMITTED)
        val awaiting = state.deviceSessionSnapshot().identity?.vescApplicationQuery as MobileVescApplicationQueryStateDto.AwaitingResponse
        clock = awaiting.responseDeadlineMs
        session.tick()
        check(state.deviceSessionSnapshot().identity?.vescApplication == MobileVescApplicationDiscoveryDto.ABSENT)
        return token
    }
}
