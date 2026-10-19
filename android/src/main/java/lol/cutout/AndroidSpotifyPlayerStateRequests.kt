package lol.cutout

import uniffi.cutout_mobile_ffi.MobileMusicObservationRevision
import uniffi.cutout_mobile_ffi.MobileMusicPlayerStateRequestId
import uniffi.cutout_mobile_ffi.MobileMusicProviderLifecycle
import uniffi.cutout_mobile_ffi.MobileMusicRequestCompletion
import uniffi.cutout_mobile_ffi.MobileMusicRequestExpiration

/** Native scheduling identity; request admission, expiry, and observation revision stay in Rust. */
internal data class SpotifyPlayerStateTicket(
    val id: MobileMusicPlayerStateRequestId,
    val observationRevision: MobileMusicObservationRevision,
)

internal class AndroidSpotifyPlayerStateRequests(
    private val lifecycle: MobileMusicProviderLifecycle,
) {
    private var active: SpotifyPlayerStateTicket? = null

    fun begin(nowMs: ULong): SpotifyPlayerStateTicket? {
        val revision = lifecycle.playerStateObservationRevision()
        val id = lifecycle.beginPlayerStateRequest(nowMs) ?: return null
        return SpotifyPlayerStateTicket(id, revision).also { active = it }
    }

    fun isCurrent(ticket: SpotifyPlayerStateTicket): Boolean = active == ticket

    fun complete(ticket: SpotifyPlayerStateTicket, nowMs: ULong): MobileMusicRequestCompletion {
        val completion = lifecycle.completePlayerStateRequestIfCurrent(ticket.id, ticket.observationRevision, nowMs)
        if (active == ticket) active = null
        return completion
    }

    fun expire(ticket: SpotifyPlayerStateTicket, nowMs: ULong): MobileMusicRequestExpiration {
        val expiration = lifecycle.expirePlayerStateRequest(ticket.id, nowMs)
        if (expiration != MobileMusicRequestExpiration.PENDING && active == ticket) active = null
        return expiration
    }

    /** Retires a canceled/replaced platform effect without treating it as a new observation. */
    fun retire(ticket: SpotifyPlayerStateTicket, nowMs: ULong) {
        lifecycle.completePlayerStateRequest(ticket.id, nowMs)
        if (active == ticket) active = null
    }

    fun markObserved(nowMs: ULong) = lifecycle.markPlayerStateObserved(nowMs)
}
