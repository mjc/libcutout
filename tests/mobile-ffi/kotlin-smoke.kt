import java.io.File
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.MobileCameraClockUncertaintyDto
import uniffi.cutout_mobile_ffi.MobileCameraMediaProvenanceInput
import uniffi.cutout_mobile_ffi.MobileCameraPreviewStateDto
import uniffi.cutout_mobile_ffi.MobileCameraPreviewEventDto
import uniffi.cutout_mobile_ffi.MobileCameraPreviewFileSink
import uniffi.cutout_mobile_ffi.MobileCameraSourceKindDto
import uniffi.cutout_mobile_ffi.MobileCameraVideoFrameDto
import uniffi.cutout_mobile_ffi.MobileNovatekMediaPathException
import uniffi.cutout_mobile_ffi.MobileNovatekCommandStatusDto
import uniffi.cutout_mobile_ffi.MobileNovatekCommandOutcomeDto
import uniffi.cutout_mobile_ffi.MobileNovatekHttpOriginDto
import uniffi.cutout_mobile_ffi.MobileNovatekRecordingCommandDto
import uniffi.cutout_mobile_ffi.MobileNovatekReadOnlySnapshotDto
import uniffi.cutout_mobile_ffi.mobileParseNovatekReadOnlySnapshot
import uniffi.cutout_mobile_ffi.mobileParseNovatekCommandOutcome
import uniffi.cutout_mobile_ffi.mobileNovatekMediaDownloadTarget
import uniffi.cutout_mobile_ffi.MobileGattFingerprintDto
import uniffi.cutout_mobile_ffi.MobileGattRoleDto
import uniffi.cutout_mobile_ffi.MobileMonotonicMillisDto
import uniffi.cutout_mobile_ffi.MobileSessionInputDto
import uniffi.cutout_mobile_ffi.MobileSessionInputKindDto
import uniffi.cutout_mobile_ffi.MobileTransportWriteLimitDto
import uniffi.cutout_mobile_ffi.MobilePevcapCaptureCursorDto
import uniffi.cutout_mobile_ffi.MobileRideDatabaseException
import uniffi.cutout_mobile_ffi.openRideDatabase
import java.nio.file.Files

fun main() {
    checkCaptureHistoryBoundary()
    CutoutSessionStateHandle().use { state ->
        val attempt = state.beginConnectionAttempt("kotlin-smoke", 0UL)
        val token = checkNotNull(attempt.token)
        state.connectionLinkEstablished(token)
        val reply = listOf(
            2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115,
            104, 0, 38, 208, 3,
        ).map { it.toByte() }.toByteArray()
        state.observeConnectionNotification(token, reply)
        state.resolveDeviceSession(token, false, 1UL)

        val step = checkNotNull(
            state.ingestDeviceSession(
                token,
                MobileSessionInputDto(
                    MobileSessionInputKindDto.LINK_UP,
                    MobileMonotonicMillisDto(1UL),
                    MobileTransportWriteLimitDto(185U),
                    ByteArray(0),
                    ByteArray(0),
                ),
            ),
        )
        check(step.session.connection.token == token)
        check(state.settingsDescriptors().connection.token == token)
        check(state.settingsSnapshot().connection.token == token)
        check(state.settings().connection.token == token)
    }

    val cameraFile = File.createTempFile("cutout-camera-ffi-smoke", ".h264")
    check(cameraFile.delete())
    MobileCameraPreviewFileSink.create(cameraFile.path).use { sink ->
        sink.writeFrame(
            MobileCameraVideoFrameDto(
                data = byteArrayOf(0, 0, 0, 2, 0x65, 0x88.toByte()),
                parameterSets = emptyList(),
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

    CutoutSessionStateHandle().use { session ->
        session.configureNovatekReadOnlySession(
            origin = MobileNovatekHttpOriginDto(
                address = "192.168.1.254",
                port = 80u.toUShort(),
            ),
            snapshot = MobileNovatekReadOnlySnapshotDto(
                firmwareVersion = "R3V1.1_20240411",
                movieRtspUri = "rtsp://192.168.1.254/movie",
                photoRtspUri = "rtsp://192.168.1.254/photo",
                configuration = listOf(
                    MobileNovatekCommandStatusDto(
                        commandId = 2001u.toUShort(),
                        status = 0u.toUShort(),
                    ),
                    MobileNovatekCommandStatusDto(
                        commandId = 1001u.toUShort(),
                        status = 0u.toUShort(),
                    ),
                ),
                storagePresent = true,
                media = emptyList(),
            ),
        )
        check(
            session.novatekRecordingCommandTarget(
                MobileNovatekRecordingCommandDto.START,
            ) ==
                "/?custom=1&cmd=2001&str=1",
        )
        check(
            session.novatekRecordingCommandTarget(
                MobileNovatekRecordingCommandDto.STOP,
            ) ==
                "/?custom=1&cmd=2001&str=0",
        )
        check(session.novatekStillCaptureCommandTarget() == "/?custom=1&cmd=1001")
        session.invalidateCameraLifecycle()
        check(session.novatekSessionOrigin() == null)
    }
    check(
        mobileParseNovatekCommandOutcome(
            response = "<Function><Cmd>2001</Cmd><Status>0</Status></Function>"
                .encodeToByteArray(),
            expectedCommandId = 2001u.toUShort(),
        ) == MobileNovatekCommandOutcomeDto.ACKNOWLEDGED,
    )
    check(
        mobileParseNovatekCommandOutcome(
            response = "<Function><Cmd>2001</Cmd><Status>7</Status></Function>"
                .encodeToByteArray(),
            expectedCommandId = 2001u.toUShort(),
        ) == MobileNovatekCommandOutcomeDto.REFUSED,
    )
    check(
        mobileParseNovatekCommandOutcome(
            response = "<Function><Cmd>2001</Cmd></Function>".encodeToByteArray(),
            expectedCommandId = 2001u.toUShort(),
        ) == MobileNovatekCommandOutcomeDto.UNKNOWN,
    )
}

private fun checkCaptureHistoryBoundary() {
    val directory = Files.createTempDirectory("cutout-capture-history-smoke-")
    try {
        openRideDatabase(directory.resolve("ride.sqlite").toString()).use { database ->
            try {
                val page = database.listPevcapCaptures(null, 1U)
                check(page.captures.isEmpty())
                check(page.nextCursor == null)
                try {
                    database.listPevcapCaptures(null, 0U)
                    error("An unbounded capture query was accepted")
                } catch (_: MobileRideDatabaseException.InvalidQueryLimit) {
                    // Rust rejected the query before dispatching it.
                }
                try {
                    database.listPevcapCaptures(MobilePevcapCaptureCursorDto(0UL, "invalid"), 1U)
                    error("An invalid capture cursor was accepted")
                } catch (_: MobileRideDatabaseException.InvalidPevcapCaptureCursor) {
                    // Generated bindings preserve the typed cursor error.
                }
            } finally {
                database.shutdown()
            }
        }
    } finally {
        check(directory.toFile().deleteRecursively())
    }
}
