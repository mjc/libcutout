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
            command = null,
        )
        val result = aero.ingestChecked(link)
        check(result.error == null)
        check(result.outputs.any { output ->
            output.kind == MobileSessionOutputKindDto.SUBSCRIBE && output.channel.isNotEmpty()
        })
        check(aero.diagnostics().malformedFrames == 0UL)
        aero.currentSnapshot()
    }

    FalconReadOnlySession().use { falcon ->
        val horn = MobileSessionInputDto(
            kind = MobileSessionInputKindDto.COMMAND,
            monotonicMs = 2UL,
            maxWriteLen = null,
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
