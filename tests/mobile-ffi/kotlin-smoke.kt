import java.io.File
import uniffi.cutout_mobile_ffi.AeroBenignControlSession
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.FalconBenignControlSession
import uniffi.cutout_mobile_ffi.MobileCameraClockUncertaintyDto
import uniffi.cutout_mobile_ffi.MobileCameraMediaProvenanceInput
import uniffi.cutout_mobile_ffi.MobileCameraPreviewStateDto
import uniffi.cutout_mobile_ffi.MobileCommandDto
import uniffi.cutout_mobile_ffi.MobileCameraPreviewEventDto
import uniffi.cutout_mobile_ffi.MobileCameraPreviewFileSink
import uniffi.cutout_mobile_ffi.MobileCameraSourceKindDto
import uniffi.cutout_mobile_ffi.MobileCameraVideoFrameDto
import uniffi.cutout_mobile_ffi.MobileNovatekMediaPathException
import uniffi.cutout_mobile_ffi.mobileParseNovatekReadOnlySnapshot
import uniffi.cutout_mobile_ffi.mobileNovatekMediaDownloadTarget
import uniffi.cutout_mobile_ffi.MobileNovatekRecordingCommandDto
import uniffi.cutout_mobile_ffi.mobileNovatekRecordingCommandTarget
import uniffi.cutout_mobile_ffi.MobileFalconProfileDto
import uniffi.cutout_mobile_ffi.MobileGattFingerprintDto
import uniffi.cutout_mobile_ffi.MobileGattRoleDto
import uniffi.cutout_mobile_ffi.MobileMonotonicMillisDto
import uniffi.cutout_mobile_ffi.MobilePevcapCaptureBuilder
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
    AeroBenignControlSession().use { aero ->
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
        check(aero.currentSnapshot().voltage?.value?.value == 108_760)
        check(aero.diagnostics().malformedFrames.count == 0UL)
    }

    FalconBenignControlSession().use { falcon ->
        val horn = MobileSessionInputDto(
            kind = MobileSessionInputKindDto.COMMAND,
            monotonicMs = MobileMonotonicMillisDto(2UL),
            maxWriteLen = null,
            channel = ByteArray(0),
            bytes = ByteArray(0),
            command = MobileCommandDto.SoundHorn,
        )
        val result = falcon.ingestChecked(horn)
        check(result.error?.kind == MobileSessionStepErrorKindDto.COMMAND_REFUSED)
        check(result.error?.command == MobileCommandDto.SoundHorn)
    }

    try {
        FalconBenignControlSession.withProfile(MobileFalconProfileDto.UNSUPPORTED)
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
        val captureFile = File.createTempFile("cutout-mobile-ffi-smoke", ".jsonl")
        check(capture.startWriter(captureFile.path))
        check(capture.recordLinkUp(
            monotonicMs = MobileMonotonicMillisDto(1UL),
            maxWriteLen = MobileTransportWriteLimitDto(185U),
        ))
        check(capture.recordNotification(
            monotonicMs = MobileMonotonicMillisDto(2UL),
            characteristic = ByteArray(16) { 0x11 },
            service = ByteArray(16) { 0x22 },
            bytes = byteArrayOf(0xde.toByte(), 0xad.toByte(), 0xbe.toByte(), 0xef.toByte()),
        ))
        check(capture.finishWriter())
        val exported = captureFile.readText()
        check(exported.contains("capture_label=powered_on_stationary"))
        check(exported.contains("\"protocol_family\":\"BegodeGotway\""))
        check(exported.contains("\"model\":{\"value\":\"Begode Falcon\",\"verification\":\"Inferred\"}"))
        check(exported.contains("\"roles\":[\"Read\",\"WriteWithoutResponse\",\"Notify\"]"))
        check(exported.contains("\"bytes\":[222,173,190,239]"))
        check(captureFile.delete())
    }

    val cameraFile = File.createTempFile("cutout-camera-ffi-smoke", ".h264")
    MobileCameraPreviewFileSink.create(cameraFile.path).use { sink ->
        sink.writeFrame(
            MobileCameraVideoFrameDto(
                data = byteArrayOf(0, 0, 0, 2, 0x65, 0x88.toByte()),
                loss = 0U,
                isRandomAccessPoint = true,
                timestamp = 90_000L,
                clockRateHz = 90_000U,
            ),
        )
        sink.finish()
    }
    check(cameraFile.readBytes().contentEquals(byteArrayOf(0, 0, 0, 1, 0x65, 0x88.toByte())))
    check(cameraFile.delete())

    val novatekSnapshot = mobileParseNovatekReadOnlySnapshot(
        firmwareResponse = "<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>"
            .encodeToByteArray(),
        liveViewResponse = "<LIST><MovieLiveViewLink>rtsp://192.168.1.254/xxx.mov</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink></LIST>"
            .encodeToByteArray(),
        configurationResponse = "<Function><Cmd>2001</Cmd><Status>0</Status></Function>"
            .encodeToByteArray(),
        storageResponse = "<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>"
            .encodeToByteArray(),
        mediaResponse = "<LIST><File><NAME>clip.TS</NAME><FPATH>A:\\Novatek\\Movie\\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File></LIST>"
            .encodeToByteArray(),
    )
    check(novatekSnapshot.firmwareVersion == "R3V1.1_20240411")
    check(novatekSnapshot.media.single().path == "A:\\Novatek\\Movie\\clip.TS")

    CutoutSessionStateHandle().use { cameraState ->
        cameraState.reduceCameraPreview(MobileCameraPreviewEventDto.STARTED)
        cameraState.reduceCameraPreview(MobileCameraPreviewEventDto.FRAME_RECEIVED)
        cameraState.recordCameraMediaProvenance(
            MobileCameraMediaProvenanceInput(
                source = MobileCameraSourceKindDto.NOVATEK_R3_PRO,
                cameraPath = "A:\\Novatek\\Movie\\clip.TS",
                sizeBytes = 42UL,
                cameraTimecode = 7UL,
                cameraTime = "2025/01/01 00:00:00",
                rideCaptureFileName = "ride.pevcap",
                capturedAtMonotonicMs = 100UL,
                capturedAtWallClockMs = 200UL,
                clockUncertainty = MobileCameraClockUncertaintyDto.Milliseconds(500UL),
            ),
        )
        check(cameraState.cameraSnapshot().preview == MobileCameraPreviewStateDto.LIVE)
        val provenance = cameraState.cameraMediaProvenance().single()
        check(provenance.source == MobileCameraSourceKindDto.NOVATEK_R3_PRO)
        check(provenance.cameraPath == "A:\\Novatek\\Movie\\clip.TS")
        check(provenance.sizeBytes == 42UL)
        check(provenance.cameraTimecode == 7UL)
        check(provenance.cameraTime == "2025/01/01 00:00:00")
        check(provenance.rideCaptureFileName == "ride.pevcap")
        check(provenance.capturedAtMonotonicMs == 100UL)
        check(provenance.capturedAtWallClockMs == 200UL)
        check(
            provenance.clockUncertainty ==
                MobileCameraClockUncertaintyDto.Milliseconds(500UL),
        )
    }

    check(
        mobileNovatekMediaDownloadTarget("A:\\Novatek\\Movie\\clip.TS") ==
            "/Novatek/Movie/clip.TS",
    )
    try {
        mobileNovatekMediaDownloadTarget("A:\\Novatek\\Movie\\..\\clip.TS")
        error("unsafe Novatek media paths should throw")
    } catch (_: MobileNovatekMediaPathException.InvalidPath) {
    }

    check(
        mobileNovatekRecordingCommandTarget(MobileNovatekRecordingCommandDto.START) ==
            "/?custom=1&cmd=2001&str=1",
    )
    check(
        mobileNovatekRecordingCommandTarget(MobileNovatekRecordingCommandDto.STOP) ==
            "/?custom=1&cmd=2001&str=0",
    )
}

fun hexBytes(text: String): ByteArray {
    val digits = text.filterNot { it.isWhitespace() }
    require(digits.length % 2 == 0)
    return digits.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
}
