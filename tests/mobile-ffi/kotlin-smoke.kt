import uniffi.cutout_mobile_ffi.AeroReadOnlySession
import uniffi.cutout_mobile_ffi.FalconReadOnlySession
import uniffi.cutout_mobile_ffi.MobileCommandDto
import uniffi.cutout_mobile_ffi.MobileFalconProfileDto
import uniffi.cutout_mobile_ffi.MobileSessionConstructorException
import uniffi.cutout_mobile_ffi.MobileSessionInputDto
import uniffi.cutout_mobile_ffi.MobileSessionInputKindDto
import uniffi.cutout_mobile_ffi.MobileSessionOutputKindDto
import uniffi.cutout_mobile_ffi.MobileSessionStepErrorKindDto

fun main() {
    AeroReadOnlySession().use { aero ->
        val link = MobileSessionInputDto(
            kind = MobileSessionInputKindDto.LINK_UP,
            monotonicMs = 1UL,
            maxWriteLen = 185U,
            channel = ByteArray(0),
            bytes = ByteArray(0),
            command = null,
        )
        val result = aero.ingestChecked(link)
        check(result.error == null)
        val channel = result.outputs.firstOrNull { output ->
            output.kind == MobileSessionOutputKindDto.SUBSCRIBE && output.channel.isNotEmpty()
        }?.channel
        check(channel != null)
        val notification = MobileSessionInputDto(
            kind = MobileSessionInputKindDto.NOTIFICATION,
            monotonicMs = 2UL,
            maxWriteLen = null,
            channel = channel,
            bytes = hexBytes(
                """
                dc5a5c532a7c000000000000ab41001700000cff
                000000000226021ca8f607801afa000080c80000
                808080808080022880803080800e310e310e2f0e
                2f0e300e2a0e320e2e0e300e310e300e2d0e2f0e
                310e2e9e05e3ad
                """,
            ),
            command = null,
        )
        check(aero.ingestChecked(notification).error == null)
        check(aero.currentSnapshot().voltageMv == 108_760)
        check(aero.diagnostics().malformedFrames == 0UL)
    }

    FalconReadOnlySession().use { falcon ->
        val horn = MobileSessionInputDto(
            kind = MobileSessionInputKindDto.COMMAND,
            monotonicMs = 2UL,
            maxWriteLen = null,
            channel = ByteArray(0),
            bytes = ByteArray(0),
            command = MobileCommandDto.SOUND_HORN,
        )
        val result = falcon.ingestChecked(horn)
        check(result.error?.kind == MobileSessionStepErrorKindDto.COMMAND_REFUSED)
        check(result.error?.command == MobileCommandDto.SOUND_HORN)
    }

    try {
        FalconReadOnlySession.withProfile(MobileFalconProfileDto.UNSUPPORTED)
        error("unsupported Falcon profile should throw")
    } catch (_: MobileSessionConstructorException.UnsupportedFalconProfile) {
    }
}

fun hexBytes(text: String): ByteArray {
    val digits = text.filterNot { it.isWhitespace() }
    require(digits.length % 2 == 0)
    return digits.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
}
