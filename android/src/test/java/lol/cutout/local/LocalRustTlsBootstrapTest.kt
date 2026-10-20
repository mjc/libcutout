package lol.cutout.local

import lol.cutout.CutoutApplication
import lol.cutout.RustTlsPlatformVerifier
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** Checks the real host JNI bridge; Android certificate trust still requires Android proof. */
abstract class RustTlsBootstrapContract {
    @Test
    fun applicationStartupAndRepeatedCallsResolveTheNativeBootstrap() {
        // Robolectric already called the production Application.onCreate hook.
        val application = RuntimeEnvironment.getApplication()
        RustTlsPlatformVerifier.initialize(application.applicationContext)
    }
}

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = CutoutApplication::class)
@GraphicsMode(GraphicsMode.Mode.LEGACY)
class LocalRustTlsBootstrapTest : RustTlsBootstrapContract()

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = CutoutApplication::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class LocalNativeRustTlsBootstrapTest : RustTlsBootstrapContract()
