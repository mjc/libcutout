package lol.cutout

import android.os.SystemClock
import uniffi.cutout_mobile_ffi.MobileChargeEstimateInputDto
import uniffi.cutout_mobile_ffi.MobileChargeEstimateStateDto
import uniffi.cutout_mobile_ffi.MobileChargeEstimator
import uniffi.cutout_mobile_ffi.MobileChargeProfileDto
import uniffi.cutout_mobile_ffi.MobileConnectionReadinessDto
import uniffi.cutout_mobile_ffi.MobileConnectionTransportStateDto
import uniffi.cutout_mobile_ffi.MobileDurationDto
import uniffi.cutout_mobile_ffi.MobileMonotonicMillisDto
import uniffi.cutout_mobile_ffi.MobileSettingsDto
import uniffi.cutout_mobile_ffi.MobileTelemetrySnapshotDto
import uniffi.cutout_mobile_ffi.MobileDeviceSessionSnapshotDto

/** Android presentation adapter for the Rust-owned charge estimator. */
class ChargeEstimateRuntime(
    private val estimator: MobileChargeEstimator = MobileChargeEstimator(),
) : AutoCloseable {
    private var configuredToken: Pair<ULong, String>? = null
    private var configuredProfile: MobileChargeProfileDto? = null
    private var closed = false

    /**
     * Admit the latest session telemetry to Rust and return its typed presentation state.
     * The profile is used only when it belongs to the same connection attempt, as in Swift.
     */
    @Synchronized
    fun observe(
        session: MobileDeviceSessionSnapshotDto?,
        settings: MobileSettingsDto?,
        telemetry: MobileTelemetrySnapshotDto?,
    ): MobileChargeEstimateStateDto? {
        check(!closed) { "Charge estimate runtime is closed" }
        val connection = session?.connection ?: run {
            clear()
            return null
        }
        val token = connection.token
        if (token == null || connection.transport == MobileConnectionTransportStateDto.DISCONNECTED ||
            connection.readiness == MobileConnectionReadinessDto.DISCONNECTED ||
            connection.readiness == MobileConnectionReadinessDto.FAILED ||
            connection.readiness == MobileConnectionReadinessDto.RECORD_ONLY
        ) {
            clear()
            return null
        }

        val tokenKey = token.generation to token.platformIdentifier
        val profile = settings?.takeIf {
            val settingsToken = it.connection.token
            settingsToken != null && settingsToken.generation == token.generation &&
                settingsToken.platformIdentifier == token.platformIdentifier
        }?.defaultChargeProfile
        if (configuredToken != tokenKey || configuredProfile != profile) {
            if (profile == null) estimator.clearProfile() else estimator.configureProfile(profile)
            configuredToken = tokenKey
            configuredProfile = profile
        }

        val snapshot = telemetry ?: return null
        return estimator.update(
            MobileChargeEstimateInputDto(
                at = MobileMonotonicMillisDto(SystemClock.elapsedRealtime().toULong()),
                snapshot = snapshot,
                freshness = MobileDurationDto(30_000UL),
            ),
        )
    }

    @Synchronized
    private fun clear() {
        if (configuredToken != null || configuredProfile != null) estimator.clearProfile()
        configuredToken = null
        configuredProfile = null
    }

    @Synchronized
    override fun close() {
        if (closed) return
        clear()
        estimator.close()
        closed = true
    }
}
