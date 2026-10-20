//! JNI startup boundary for Android's process-owned platform TLS verifier.

use jni::{EnvUnowned, errors::ThrowRuntimeExAndDefault, jni_mangle, objects::JObject};

/// Initializes Android certificate verification before Rust clients can network.
///
/// The host symbol also exists for the normal Application lifecycle in Robolectric.
#[jni_mangle("lol.cutout.RustTlsPlatformVerifier")]
#[expect(
    unreachable_pub,
    reason = "jni_mangle requires pub for this exported JNI symbol in a private Rust module"
)]
pub fn initialize<'caller>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    context: JObject<'caller>,
) {
    unowned_env
        .with_env(|env| {
            #[cfg(target_os = "android")]
            {
                rustls_platform_verifier::android::init_with_env(env, context)
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = (env, context);
                Ok::<(), jni::errors::Error>(())
            }
        })
        .resolve::<ThrowRuntimeExAndDefault>();
}
