import uniffi.cutout_mobile_ffi.AeroReadOnlySession
import uniffi.cutout_mobile_ffi.FalconReadOnlySession
import uniffi.cutout_mobile_ffi.MobileCommandDto
import uniffi.cutout_mobile_ffi.MobileFalconProfileDto
import uniffi.cutout_mobile_ffi.MobileGattFingerprintDto
import uniffi.cutout_mobile_ffi.MobileGattRoleDto
import uniffi.cutout_mobile_ffi.MobileMonotonicMillisDto
import uniffi.cutout_mobile_ffi.MobilePevcapCaptureBuilder
import uniffi.cutout_mobile_ffi.MobilePevcapEncodingDto
import uniffi.cutout_mobile_ffi.MobileProtocolFamilyDto
import uniffi.cutout_mobile_ffi.MobileResolvedIdentityDto
import uniffi.cutout_mobile_ffi.MobileSessionConstructorException
import uniffi.cutout_mobile_ffi.MobileSessionInputDto
import uniffi.cutout_mobile_ffi.MobileSessionInputKindDto
import uniffi.cutout_mobile_ffi.MobileSessionOutputKindDto
import uniffi.cutout_mobile_ffi.MobileSessionStepErrorKindDto
import uniffi.cutout_mobile_ffi.MobileTransportWriteLimitDto
import uniffi.cutout_mobile_ffi.MobileVerificationStatusDto
import uniffi.cutout_mobile_ffi.MobileVerifiedStringDto
import uniffi.cutout_mobile_ffi.MobileWallClockUnixMillisDto

fun main() {
    AeroReadOnlySession().use { aero ->
        val link = MobileSessionInputDto(
            kind = MobileSessionInputKindDto.LINK_UP,
            monotonicMs = MobileMonotonicMillisDto(1UL),
            maxWriteLen = MobileTransportWriteLimitDto(185U),
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
            monotonicMs = MobileMonotonicMillisDto(2UL),
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
        check(aero.currentSnapshot().voltage?.value == 108_760)
        check(aero.diagnostics().malformedFrames.count == 0UL)
    }

    FalconReadOnlySession().use { falcon ->
        val horn = MobileSessionInputDto(
            kind = MobileSessionInputKindDto.COMMAND,
            monotonicMs = MobileMonotonicMillisDto(2UL),
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

    MobilePevcapCaptureBuilder(
        wallClockStartUnixMs = MobileWallClockUnixMillisDto(1_700_000_000_000UL),
        platformId = "ios-corebluetooth",
        writeLimit = MobileTransportWriteLimitDto(185U),
    ).use { capture ->
        capture.addAnnotation("capture_label=powered_on_stationary")
        capture.addAnnotation("capture_privacy=redacted")
        capture.addAnnotation("capture_distribution=redistributable")
        capture.addAnnotation("capture_evidence=hardware_tested")
        val ffe0 = hexBytes("0000ffe000001000800000805f9b34fb")
        val ffe1 = hexBytes("0000ffe100001000800000805f9b34fb")
        capture.addAdvertisedService(ffe0)
        capture.addGattFingerprint(
            MobileGattFingerprintDto(
                service = ffe0,
                characteristic = ffe1,
                roles = listOf(
                    MobileGattRoleDto.READ,
                    MobileGattRoleDto.WRITE_WITHOUT_RESPONSE,
                    MobileGattRoleDto.NOTIFY,
                ),
                verification = MobileVerificationStatusDto.HARDWARE_VERIFIED,
            ),
        )
        capture.setResolvedIdentity(
            MobileResolvedIdentityDto(
                protocolFamily = MobileProtocolFamilyDto.BEGODE_GOTWAY,
                model = MobileVerifiedStringDto(
                    value = "Begode Falcon",
                    verification = MobileVerificationStatusDto.INFERRED,
                ),
                firmware = MobileVerifiedStringDto(
                    value = "GW2015004",
                    verification = MobileVerificationStatusDto.HARDWARE_VERIFIED,
                ),
            ),
        )
        capture.recordLinkUp(
            monotonicMs = MobileMonotonicMillisDto(1UL),
            maxWriteLen = MobileTransportWriteLimitDto(185U),
        )
        capture.recordNotification(
            monotonicMs = MobileMonotonicMillisDto(2UL),
            characteristic = ByteArray(16) { 0x11 },
            service = ByteArray(16) { 0x22 },
            bytes = byteArrayOf(0xde.toByte(), 0xad.toByte(), 0xbe.toByte(), 0xef.toByte()),
        )
        val exported = capture.export(MobilePevcapEncodingDto.JSONL).decodeToString()
        check(exported.contains("capture_label=powered_on_stationary"))
        check(exported.contains("\"protocol_family\":\"BegodeGotway\""))
        check(exported.contains("\"model\":{\"value\":\"Begode Falcon\",\"verification\":\"Inferred\"}"))
        check(exported.contains("\"roles\":[\"Read\",\"WriteWithoutResponse\",\"Notify\"]"))
        check(exported.contains("\"bytes\":[222,173,190,239]"))
    }
}

fun hexBytes(text: String): ByteArray {
    val digits = text.filterNot { it.isWhitespace() }
    require(digits.length % 2 == 0)
    return digits.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
}
