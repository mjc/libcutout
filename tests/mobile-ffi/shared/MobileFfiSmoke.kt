package lol.cutout.smoke

import java.io.File
import lol.cutout.NativeReceipt
import lol.cutout.PevSession
import lol.cutout.wireBytes
import uniffi.cutout_mobile_ffi.MobileConnectionAttemptTokenDto
import uniffi.cutout_mobile_ffi.MobileSessionOutputDto
import uniffi.cutout_mobile_ffi.MobileVehicleKindDto
import uniffi.cutout_mobile_ffi.MobileVescApplicationDiscoveryDto
import uniffi.cutout_mobile_ffi.MobileVescDrivetrainDraft
import uniffi.cutout_mobile_ffi.MobileVescDrivetrainException
import uniffi.cutout_mobile_ffi.MobileVescDrivetrainSourceDto
import uniffi.cutout_mobile_ffi.MobileVescDrivetrainTargetDto
import uniffi.cutout_mobile_ffi.CutoutSessionStateHandle
import uniffi.cutout_mobile_ffi.MobileCameraClockUncertaintyDto
import uniffi.cutout_mobile_ffi.MobileCameraMediaProvenanceInput
import uniffi.cutout_mobile_ffi.MobileCaptureOriginDto
import uniffi.cutout_mobile_ffi.MobileCameraPreviewStateDto
import uniffi.cutout_mobile_ffi.MobileCameraPreviewEventDto
import uniffi.cutout_mobile_ffi.MobileCameraPreviewFileSink
import uniffi.cutout_mobile_ffi.MobileCameraSourceKindDto
import uniffi.cutout_mobile_ffi.MobileCameraVideoFrameDto
import uniffi.cutout_mobile_ffi.MobileNovatekMediaPathException
import uniffi.cutout_mobile_ffi.MobileNovatekCommandOutcomeDto
import uniffi.cutout_mobile_ffi.MobileNovatekHttpOriginDto
import uniffi.cutout_mobile_ffi.MobileNovatekCommandDto
import uniffi.cutout_mobile_ffi.MobileNovatekMediaDownloadAssociationInput
import uniffi.cutout_mobile_ffi.MobileNovatekMediaDownloadCompletionInput
import uniffi.cutout_mobile_ffi.mobileParseNovatekReadOnlySnapshot
import uniffi.cutout_mobile_ffi.mobileNovatekMediaDownloadTarget
import uniffi.cutout_mobile_ffi.MobileGattFingerprintDto
import uniffi.cutout_mobile_ffi.MobileGattRoleDto
import uniffi.cutout_mobile_ffi.MobileMonotonicMillisDto
import uniffi.cutout_mobile_ffi.MobileSessionInputDto
import uniffi.cutout_mobile_ffi.MobileSessionInputKindDto
import uniffi.cutout_mobile_ffi.MobileSessionOutputKindDto
import uniffi.cutout_mobile_ffi.MobileTransportWriteLimitDto
import uniffi.cutout_mobile_ffi.MobilePevcapCaptureCursorDto
import uniffi.cutout_mobile_ffi.MobileRideDatabaseException
import uniffi.cutout_mobile_ffi.openRideDatabase
import java.nio.file.Files

fun checkSessionAndCameraBoundary(directory: File) {
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
        check(step.result.error == null)
        check(step.result.outputs.any { it.kind == MobileSessionOutputKindDto.SUBSCRIBE })
        check(state.settingsDescriptors().connection.token == token)
        check(state.settingsSnapshot().connection.token == token)
        check(state.settings().connection.token == token)
    }

    val cameraFile = File.createTempFile("cutout-camera-ffi-smoke", ".h264", directory)
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
    check(checkNotNull(novatekSnapshot.media).single().path == "A:\\Novatek\\Movie\\clip.TS")

    CutoutSessionStateHandle().use { cameraState ->
        val captureGeneration =
            cameraState.beginCapture(MobileCaptureOriginDto.MANUAL)
                ?: error("capture start should be admitted")
        check(cameraState.captureWriterStarted(captureGeneration))
        cameraState.reduceCameraPreview(MobileCameraPreviewEventDto.STARTED)
        cameraState.reduceCameraPreview(MobileCameraPreviewEventDto.FRAME_RECEIVED)
        val origin = MobileNovatekHttpOriginDto(
            address = "192.168.1.254",
            port = 80u.toUShort(),
        )
        cameraState.configureNovatekReadOnlySession(
            origin = origin,
            firmwareResponse = "<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>"
                .encodeToByteArray(),
            liveViewResponse = "<LIST><MovieLiveViewLink>rtsp://192.168.1.254/movie</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/photo</PhotoLiveViewLink></LIST>"
                .encodeToByteArray(),
            configurationResponse = "<Function><Cmd>2016</Cmd><Status>0</Status></Function>"
                .encodeToByteArray(),
            storageResponse = "<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>"
                .encodeToByteArray(),
            mediaResponse = "<LIST><File><NAME>clip.TS</NAME><FPATH>A:\\Novatek\\Movie\\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File></LIST>"
                .encodeToByteArray(),
        )
        val authorized = cameraState.authorizeNovatekMediaDownload("A:\\Novatek\\Movie\\clip.TS")
        check(authorized.target == "/Novatek/Movie/clip.TS")
        val completed = cameraState.completeNovatekMediaDownload(
            MobileNovatekMediaDownloadCompletionInput(
                operationId = authorized.operationId,
                actualSize = 42UL,
                association = MobileNovatekMediaDownloadAssociationInput(
                    captureGeneration = captureGeneration,
                    rideCaptureFileName = "ride.pevcap",
                    capturedAtMonotonicMs = 100UL,
                    capturedAtWallClockMs = 200UL,
                    clockUncertainty = MobileCameraClockUncertaintyDto.Unknown,
                ),
            ),
        )
        check(completed.media.timecode == 7UL)
        check(completed.provenance?.cameraTimecode == 7UL)
        cameraState.recordCameraMediaProvenance(
            MobileCameraMediaProvenanceInput(
                captureGeneration = captureGeneration,
                source = MobileCameraSourceKindDto.FIXTURE,
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
        val provenance = cameraState.cameraMediaProvenance().single {
            it.source == MobileCameraSourceKindDto.NOVATEK_R3_PRO
        }
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
                MobileCameraClockUncertaintyDto.Unknown,
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
        val cameraOrigin = MobileNovatekHttpOriginDto(
            address = "192.168.1.254",
            port = 80u.toUShort(),
        )
        session.configureNovatekReadOnlySession(
            origin = cameraOrigin,
            firmwareResponse = "<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>"
                .encodeToByteArray(),
            liveViewResponse = "<LIST><MovieLiveViewLink>rtsp://192.168.1.254/movie</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/photo</PhotoLiveViewLink></LIST>"
                .encodeToByteArray(),
            configurationResponse = "<Function><Cmd>2001</Cmd><Status>0</Status><Cmd>1001</Cmd><Status>0</Status></Function>"
                .encodeToByteArray(),
            storageResponse = "<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>"
                .encodeToByteArray(),
            mediaResponse = "<LIST></LIST>".encodeToByteArray(),
        )
        val startRequest = session.authorizeNovatekCommand(
            cameraOrigin,
            MobileNovatekCommandDto.START_RECORDING,
        )
        check(startRequest.target == "/?custom=1&cmd=2001&str=1")
        check(
            session.completeNovatekCommand(
                startRequest,
                "<Function><Cmd>2001</Cmd><Status>0</Status></Function>".encodeToByteArray(),
            ) == MobileNovatekCommandOutcomeDto.ACKNOWLEDGED,
        )
        val stopRequest = session.authorizeNovatekCommand(
            cameraOrigin,
            MobileNovatekCommandDto.STOP_RECORDING,
        )
        check(stopRequest.target == "/?custom=1&cmd=2001&str=0")
        check(
            session.completeNovatekCommand(
                stopRequest,
                "<Function><Cmd>2001</Cmd><Status>7</Status></Function>".encodeToByteArray(),
            ) == MobileNovatekCommandOutcomeDto.REFUSED,
        )
        val stillRequest = session.authorizeNovatekCommand(
            cameraOrigin,
            MobileNovatekCommandDto.STILL_CAPTURE,
        )
        check(stillRequest.target == "/?custom=1&cmd=1001")
        check(
            session.completeNovatekCommand(
                stillRequest,
                "<Function><Cmd>1001</Cmd><Status>0</Status></Function>".encodeToByteArray(),
            ) == MobileNovatekCommandOutcomeDto.ACKNOWLEDGED,
        )
        val unknownRequest = session.authorizeNovatekCommand(
            cameraOrigin,
            MobileNovatekCommandDto.STILL_CAPTURE,
        )
        check(
            session.completeNovatekCommand(
                unknownRequest,
                "<Function><Cmd>1001</Cmd></Function>".encodeToByteArray(),
            ) == MobileNovatekCommandOutcomeDto.UNKNOWN,
        )
        session.invalidateCameraLifecycle()
        check(session.novatekSessionOrigin() == null)
    }
}

fun checkCaptureHistoryBoundary(storageRoot: File) {
    val directory = Files.createTempDirectory(storageRoot.toPath(), "capture-history-")
    try {
        openRideDatabase(directory.resolve("ride.sqlite").toString()).use { database ->
            try {
                database.saveSelectedDevice("kotlin-persistence-fixture", 1UL)
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
        openRideDatabase(directory.resolve("ride.sqlite").toString()).use { reopened ->
            try {
                check(reopened.selectedDevice() == "kotlin-persistence-fixture")
            } finally {
                reopened.shutdown()
            }
        }
    } finally {
        check(directory.toFile().deleteRecursively())
    }
}

/** Generated Kotlin/Rust contract for controller-scoped configuration; no hardware effects. */
fun checkControllerGeometryBoundary(storageRoot: File) {
    val directory = Files.createTempDirectory(storageRoot.toPath(), "controller-geometry-")
    val path = directory.resolve("ride.sqlite").toString()
    val local = MobileVescDrivetrainDraft("250", "15", "1")
    val secondary = MobileVescDrivetrainDraft("200", "7", "2.5")
    val changed = MobileVescDrivetrainDraft("210", "7", "2.5")
    val can = MobileVescDrivetrainTargetDto.Can(7U, ByteArray(12) { 1 })
    val replacement = MobileVescDrivetrainTargetDto.Can(7U, ByteArray(12) { 2 })
    try {
        openRideDatabase(path).use { database ->
            try {
                CutoutSessionStateHandle.withDatabase(database).use { state ->
                    val token = connectGeometryScooter(state)
                    check(state.saveConnectionVescDrivetrain(token, local, 1UL) == local)
                    val measured = checkNotNull(state.ingestDeviceSession(token, MobileSessionInputDto(
                        MobileSessionInputKindDto.NOTIFICATION, MobileMonotonicMillisDto(3_001UL), null,
                        java.util.UUID.fromString("6e400003-b5a3-f393-e0a9-e50e24dcca9e").wireBytes(),
                        byteArrayOf(2, 9, 50, 0, 0, 0, 128.toByte(), 0, 0, 23, 112, 244.toByte(), 198.toByte(), 3),
                    )))
                    checkNotNull(measured.telemetry.speed)
                    check(state.saveConnectionVescControllerDrivetrain(token, can, secondary, 2UL) == secondary)
                    val before = checkNotNull(state.connectionTelemetrySnapshot(token))
                    check(before.vescDrivetrainContext == measured.telemetry.vescDrivetrainContext)
                    check(before.speed == measured.telemetry.speed)
                    check(before.speedObservedAtMs == measured.telemetry.speedObservedAtMs)
                    check(before.configuredVescDrivetrains.size == 2)
                    val configured = before.configuredVescDrivetrains[1]
                    val target = configured.target
                    check(target is MobileVescDrivetrainTargetDto.Can)
                    check(target.controllerId == 7U.toUByte() && target.controllerUuid.contentEquals(ByteArray(12) { 1 }))
                    check(configured.revision == 1UL && configured.motorToWheelRatioMilli == 2_500U)
                    check(configured.source == MobileVescDrivetrainSourceDto.USER_CONFIGURED)
                    state.saveConnectionVescControllerDrivetrain(token, can, secondary, 3UL)
                    check(checkNotNull(state.connectionTelemetrySnapshot(token)).configuredVescDrivetrains[1].revision == 1UL)
                    state.saveConnectionVescControllerDrivetrain(token, can, changed, 4UL)
                    val after = checkNotNull(state.connectionTelemetrySnapshot(token))
                    check(after.configuredVescDrivetrains[1].revision == 2UL)
                    check(after.vescDrivetrainContext == before.vescDrivetrainContext)
                    check(after.speed == before.speed && after.speedObservedAtMs == before.speedObservedAtMs)
                    check(configured.revision == 1UL && configured.wheelDiameterMicrometres == 200_000U)
                    for (invalid in listOf(
                        MobileVescDrivetrainTargetDto.Can(255U, ByteArray(12) { 1 }),
                        MobileVescDrivetrainTargetDto.Can(7U, ByteArray(12)),
                        MobileVescDrivetrainTargetDto.Can(7U, ByteArray(11) { 1 }),
                    )) {
                        try {
                            state.saveConnectionVescControllerDrivetrain(token, invalid, secondary, 5UL)
                            error("An invalid controller association was accepted")
                        } catch (_: MobileVescDrivetrainException.InvalidControllerTarget) {
                            // Generated bindings preserve the Rust boundary refusal.
                        }
                    }
                    state.disconnectConnectionAttempt()
                }
            } finally {
                database.shutdown()
            }
        }
        openRideDatabase(path).use { database ->
            try {
                CutoutSessionStateHandle.withDatabase(database).use { state ->
                    val token = checkNotNull(state.beginConnectionAttempt("kotlin-controller-geometry", 0UL).token)
                    check(state.restoreConnectionVescControllerDrivetrain(token, replacement) == null)
                    check(state.restoreConnectionVescControllerDrivetrain(token, can) == changed)
                    check(state.restoreConnectionVescDrivetrain(token) == local)
                    state.disconnectConnectionAttempt()
                }
            } finally {
                database.shutdown()
            }
        }
    } finally {
        check(directory.toFile().deleteRecursively())
    }
}

private fun connectGeometryScooter(state: CutoutSessionStateHandle): MobileConnectionAttemptTokenDto {
    var clock = 1UL
    val token = checkNotNull(state.beginConnectionAttempt("kotlin-controller-geometry", 0UL).token)
    state.connectionLinkEstablished(token)
    val outputs = mutableListOf<MobileSessionOutputDto>()
    val session = PevSession(state, token, { clock }, { outputs += it }) { _, _, _ -> }
    session.notification(byteArrayOf(), byteArrayOf(2, 20, 157.toByte(), 7, 1, 2, 97, 98, 99, 49, 50, 51, 0,
        117, 115, 101, 114, 104, 97, 115, 104, 0, 38, 208.toByte(), 3))
    val query = outputs.first { it.applicationQuery != null }
    session.writeReceipt(query, NativeReceipt.SUBMITTED)
    clock = 3_000UL
    session.tick()
    check(state.deviceSessionSnapshot().identity?.vescApplication == MobileVescApplicationDiscoveryDto.ABSENT)
    state.selectConnectionVehicleKind(token, MobileVehicleKindDto.SCOOTER, 1UL)
    return token
}
