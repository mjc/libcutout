package lol.cutout

import android.content.Context
import com.sun.jna.JNIEnv
import com.sun.jna.Library
import com.sun.jna.Native

internal interface RustTlsBootstrapLibrary : Library {
    fun Java_lol_cutout_RustTlsPlatformVerifier_initialize(env: JNIEnv, owner: Any, context: Context)
}

/** Supplies the process Application context before Rust HTTPS clients are used. */
internal object RustTlsPlatformVerifier {
    private val library: RustTlsBootstrapLibrary by lazy {
        Native.load(
            "cutout_mobile_ffi",
            RustTlsBootstrapLibrary::class.java,
            mapOf(Library.OPTION_ALLOW_OBJECTS to true),
        )
    }

    fun initialize(context: Context) {
        // JNA supplies this thread's JNI environment and retains the live object arguments
        // for the synchronous call, using the same library loader as the UniFFI bindings.
        library.Java_lol_cutout_RustTlsPlatformVerifier_initialize(JNIEnv.CURRENT, this, context)
    }
}
