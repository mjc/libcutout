package lol.cutout.local

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import lol.cutout.AndroidPhoneAlarmClient
import lol.cutout.CutoutApplication
import lol.cutout.BlePresentation
import lol.cutout.ffitest.productionReplayPresentation
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalPhoneAlarmVehicleNameTest {
    @Test
    fun savedVehicleNameOverridesProtocolFallbackAndStaleLookupCannotReplaceCurrentDevice() {
        val context = ApplicationProvider.getApplicationContext<Context>().applicationContext
        val app = context as CutoutApplication
        val runtime = app.rides
        val database = await { done -> runtime.withDatabase({ it }, done) }
        val oldIdentity = "phone-alarm-name-${UUID.randomUUID()}"
        val currentIdentity = "phone-alarm-name-${UUID.randomUUID()}"
        val oldName = "Stale saved name"
        val currentName = "Current saved wheel"
        await { done -> runtime.withDatabase({ db ->
            db.saveDeviceName(oldIdentity, oldName, 1UL)
            db.saveDeviceName(currentIdentity, currentName, 2UL)
        }, done) }

        val replay = productionReplayPresentation()
        val state = CutoutSessionStateHandle.withDatabase(database)
        val client = AndroidPhoneAlarmClient(context, runtime)
        val currentActivated = CountDownLatch(1)
        val currentNamed = CountDownLatch(1)
        val subscription = client.addObserver { value ->
            if (value.preferences?.deviceIdentity == currentIdentity) currentActivated.countDown()
            if (value.deviceLabel == currentName) currentNamed.countDown()
        }

        val enteredDatabase = CountDownLatch(1)
        val releaseDatabase = CountDownLatch(1)
        try {
            runtime.withDatabase({
                enteredDatabase.countDown()
                check(releaseDatabase.await(10, TimeUnit.SECONDS)) { "Name lookup blocker was not released" }
                Unit
            }) { }
            check(enteredDatabase.await(10, TimeUnit.SECONDS)) { "Could not hold the shared Rust database worker" }

            val originalSession = checkNotNull(replay.session)
            fun withIdentity(identity: String): BlePresentation = replay.copy(
                session = originalSession.copy(
                    connection = originalSession.connection.copy(
                        token = checkNotNull(originalSession.connection.token).copy(platformIdentifier = identity),
                    ),
                ),
            )

            client.onSessionObservation(withIdentity(oldIdentity), state)
            client.onSessionObservation(withIdentity(currentIdentity), state)
            check(awaitWithMainLooper(currentActivated, 10, TimeUnit.SECONDS)) {
                "The current phone-alarm identity was not activated"
            }

            // Ensure both per-identity database reads are queued behind the blocker before release.
            val readsQueued = CountDownLatch(1)
            runtime.withDatabase({ Unit }) { readsQueued.countDown() }
            check(readsQueued.count == 1L) { "Name lookups unexpectedly bypassed the blocked database worker" }
            releaseDatabase.countDown()

            check(awaitWithMainLooper(currentNamed, 10, TimeUnit.SECONDS)) {
                "The Rust-saved custom vehicle name was not published; latest=${client.currentPresentation()}"
            }
            check(client.currentPresentation().deviceLabel == currentName) {
                "A stale name lookup replaced the current vehicle label: ${client.currentPresentation()}"
            }

            val renamed = "Renamed saved wheel"
            await { done -> runtime.withDatabase({ db ->
                db.saveDeviceName(currentIdentity, renamed, 3UL)
            }, done) }
            val reopenedName = CountDownLatch(1)
            val reopenedObserver = client.addObserver { value ->
                if (value.deviceLabel == renamed) reopenedName.countDown()
            }
            try {
                check(awaitWithMainLooper(reopenedName, 10, TimeUnit.SECONDS)) {
                    "Reopening phone-alarm settings did not refresh the saved vehicle name"
                }
            } finally {
                reopenedObserver.close()
            }
        } finally {
            releaseDatabase.countDown()
            subscription.close()
            state.close()
            client.close()
        }
    }

    private fun <T> await(start: ((Result<T>) -> Unit) -> Unit): T {
        val result = AtomicReference<Result<T>>()
        val completed = CountDownLatch(1)
        start { result.set(it); completed.countDown() }
        check(awaitWithMainLooper(completed, 10, TimeUnit.SECONDS)) { "Rust database operation did not complete" }
        return checkNotNull(result.get()).getOrThrow()
    }
}
