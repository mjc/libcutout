import XCTest
@testable import CutoutMobile
import CutoutMobileFFI

final class CameraLocalNetworkAdapterTests: XCTestCase {
    func testCameraPathRequiresWiFiBeforeCameraConfiguration() {
        XCTAssertEqual(
            cameraConnectionPresentation(pathStatus: .unavailable, usesWiFi: false),
            .wifiRequired
        )
        XCTAssertEqual(
            cameraConnectionPresentation(pathStatus: .satisfied, usesWiFi: false),
            .wifiRequired
        )
    }

    func testSatisfiedWiFiPathDoesNotPretendTheCameraIsConnected() {
        XCTAssertEqual(
            cameraConnectionPresentation(pathStatus: .satisfied, usesWiFi: true),
            .notConfigured
        )
        XCTAssertEqual(
            cameraConnectionPresentation(
                pathStatus: .satisfied,
                usesWiFi: true,
                hasReadOnlyEvidence: true
            ),
            .connected
        )
    }

    @MainActor
    func testAdapterStartAndStopRemainNonOptimistic() {
        let adapter = CameraLocalNetworkAdapter()

        adapter.start()
        adapter.startPreview()
        adapter.stop()

        XCTAssertEqual(adapter.presentation, .initial)
    }

    @MainActor
    func testAdapterStopClearsRustCameraLifecycleTruth() {
        let session = CutoutSessionStateHandle()
        let adapter = CameraLocalNetworkAdapter(sessionState: session)

        adapter.observePreview(.live)
        adapter.observeOnboardRecording(.recording)
        adapter.stop()

        XCTAssertEqual(session.cameraSnapshot().preview, .stopped)
        XCTAssertEqual(session.cameraSnapshot().onboardRecording, .unknown)
    }

    @MainActor
    func testReadOnlyEvidenceMarksConnectionAndStorageWithoutClaimingPreviewOrRecording() {
        let snapshot = MobileNovatekReadOnlySnapshotDto(
            firmwareVersion: "R3V1.1_20240411",
            movieRtspUri: "rtsp://192.168.1.254/xxx.mov",
            photoRtspUri: "rtsp://192.168.1.254/xxx.mov",
            configuration: [MobileNovatekCommandStatusDto(commandId: 2016, status: 0)],
            storagePresent: true,
            media: []
        )
        let adapter = CameraLocalNetworkAdapter()

        adapter.apply(readOnlyEvidence: CameraReadOnlyEvidence(snapshot))

        XCTAssertEqual(adapter.presentation.connection, .connected)
        XCTAssertEqual(adapter.presentation.profileName, "FreedConn R3 Pro · Novatek")
        XCTAssertEqual(adapter.presentation.storage, .present)
        XCTAssertEqual(adapter.presentation.preview, .stopped)
        XCTAssertEqual(adapter.presentation.recording, .unknown)
        XCTAssertEqual(adapter.readOnlyEvidence?.movieRTSPURI, "rtsp://192.168.1.254/xxx.mov")

        adapter.stop()
        XCTAssertNil(adapter.readOnlyEvidence)
    }

    @MainActor
    func testUnsupportedEvidenceInvalidatesExistingCameraLifecycleTruth() {
        let session = CutoutSessionStateHandle()
        let adapter = CameraLocalNetworkAdapter(sessionState: session)
        adapter.observePreview(.live)
        adapter.observeOnboardRecording(.recording)

        let evidence = CameraReadOnlyEvidence(MobileNovatekReadOnlySnapshotDto(
            firmwareVersion: "R4V2.0_20250101",
            movieRtspUri: "rtsp://192.168.1.254/xxx.mov",
            photoRtspUri: "rtsp://192.168.1.254/xxx.mov",
            configuration: [],
            storagePresent: true,
            media: []
        ))
        adapter.apply(readOnlyEvidence: evidence)

        XCTAssertEqual(adapter.presentation.connection, .unsupported)
        XCTAssertEqual(session.cameraSnapshot().preview, .stopped)
        XCTAssertEqual(session.cameraSnapshot().onboardRecording, .unknown)
    }

    @MainActor
    func testReadOnlyLoaderUsesFixedTargetsAndRustParser() async throws {
        let responses: [String: Data] = [
            "3012": Data("<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>".utf8),
            "2019": Data("<LIST><MovieLiveViewLink>rtsp://192.168.1.254/xxx.mov</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink></LIST>".utf8),
            "3014": Data("<Function><Cmd>2016</Cmd><Status>0</Status></Function>".utf8),
            "3024": Data("<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>".utf8),
            "3015": Data("<LIST><File><NAME>clip.TS</NAME><FPATH>A:\\Novatek\\Movie\\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File></LIST>".utf8),
        ]
        let adapter = CameraLocalNetworkAdapter()

        let evidence = try await adapter.loadReadOnlyEvidence(
            address: "192.168.1.254",
            port: 80
        ) { url in
            let command = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first {
                $0.name == "cmd"
            }?.value
            XCTAssertNotNil(command)
            return responses[command!]!
        }

        XCTAssertEqual(evidence.firmwareVersion, "R3V1.1_20240411")
        XCTAssertEqual(evidence.media.first?.name, "clip.TS")
        XCTAssertEqual(adapter.presentation.connection, .connected)
        XCTAssertEqual(adapter.presentation.storage, .present)
        XCTAssertEqual(adapter.presentation.preview, .stopped)
        XCTAssertEqual(adapter.presentation.recording, .unknown)
    }

    @MainActor
    func testReadOnlyLoaderDoesNotProbeWhenWiFiPathIsUnavailable() async {
        let adapter = CameraLocalNetworkAdapter()
        adapter.apply(pathStatus: .unavailable, usesWiFi: false)

        do {
            _ = try await adapter.loadReadOnlyEvidence(
                address: "192.168.1.254",
                port: 80
            ) { _ in
                XCTFail("a known-unavailable Wi-Fi path must not reach the fetcher")
                return Data()
            }
            XCTFail("camera evidence must require the Wi-Fi path")
        } catch let error as CameraReadOnlyRequestError {
            XCTAssertEqual(error, .pathUnavailable)
        } catch {
            XCTFail("unexpected error: \(error)")
        }
        XCTAssertEqual(adapter.presentation.connection, .wifiRequired)
    }

    @MainActor
    func testReadOnlyLoaderAcceptsAnEmptyMediaListing() async throws {
        let responses: [String: Data] = [
            "3012": Data("<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>".utf8),
            "2019": Data("<LIST><MovieLiveViewLink>rtsp://192.168.1.254/xxx.mov</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink></LIST>".utf8),
            "3014": Data("<Function><Cmd>2016</Cmd><Status>0</Status></Function>".utf8),
            "3024": Data("<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>".utf8),
            "3015": Data("<LIST></LIST>".utf8),
        ]

        let evidence = try await CameraLocalNetworkAdapter().loadReadOnlyEvidence(
            address: "192.168.1.254",
            port: 80
        ) { url in
            let command = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first {
                $0.name == "cmd"
            }?.value
            return responses[command!]!
        }

        XCTAssertTrue(evidence.media.isEmpty)
    }

    @MainActor
    func testReadOnlyLoaderRejectsNonR3FirmwareBeforeClaimingProfile() async {
        let responses: [String: Data] = [
            "3012": Data("<Function><Cmd>3012</Cmd><Status>0</Status><String>R4V2.0_20250101</String></Function>".utf8),
            "2019": Data("<LIST><MovieLiveViewLink>rtsp://192.168.1.254/xxx.mov</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink></LIST>".utf8),
            "3014": Data("<Function><Cmd>2016</Cmd><Status>0</Status></Function>".utf8),
            "3024": Data("<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>".utf8),
            "3015": Data("<LIST></LIST>".utf8),
        ]
        let adapter = CameraLocalNetworkAdapter()

        do {
            _ = try await adapter.loadReadOnlyEvidence(
                address: "192.168.1.254",
                port: 80
            ) { url in
                let command = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first {
                    $0.name == "cmd"
                }?.value
                return responses[command!]!
            }
            XCTFail("non-R3 firmware must not be labeled as the R3 Pro profile")
        } catch let error as CameraReadOnlyRequestError {
            XCTAssertEqual(error, .unsupportedProfile)
        } catch {
            XCTFail("unexpected error: \(error)")
        }
        XCTAssertNil(adapter.readOnlyEvidence)
        XCTAssertEqual(adapter.presentation.connection, .unsupported)
        XCTAssertEqual(adapter.presentation.preview, .stopped)
        XCTAssertEqual(adapter.presentation.recording, .unknown)
        XCTAssertEqual(adapter.presentation.storage, .unknown)
    }

    @MainActor
    func testFailedReadOnlyRefreshDoesNotRetainStaleCameraSource() async {
        let snapshot = MobileNovatekReadOnlySnapshotDto(
            firmwareVersion: "R3V1.1_20240411",
            movieRtspUri: "rtsp://192.168.1.254/xxx.mov",
            photoRtspUri: "rtsp://192.168.1.254/xxx.mov",
            configuration: [],
            storagePresent: true,
            media: []
        )
        let adapter = CameraLocalNetworkAdapter()
        adapter.apply(readOnlyEvidence: CameraReadOnlyEvidence(snapshot))

        do {
            _ = try await adapter.loadReadOnlyEvidence(
                address: "8.8.8.8",
                port: 80
            ) { _ in
                XCTFail("an invalid origin must not reach the fetcher")
                return Data()
            }
            XCTFail("public origins should be rejected")
        } catch {
            XCTAssertNil(adapter.readOnlyEvidence)
            XCTAssertEqual(adapter.presentation, .initial)
        }
    }

    @MainActor
    func testFailedReadOnlyRefreshStopsAStalePreview() async {
        let session = CutoutSessionStateHandle()
        let adapter = CameraLocalNetworkAdapter(sessionState: session)
        adapter.observePreview(.live)

        do {
            _ = try await adapter.loadReadOnlyEvidence(
                address: "8.8.8.8",
                port: 80
            ) { _ in
                XCTFail("an invalid origin must not reach the fetcher")
                return Data()
            }
            XCTFail("public origins should be rejected")
        } catch {
            XCTAssertEqual(session.cameraSnapshot().preview, .stopped)
            XCTAssertEqual(adapter.presentation.preview, .stopped)
        }
    }

    @MainActor
    func testWiFiLossInvalidatesCameraEvidenceAndPreview() {
        let session = CutoutSessionStateHandle()
        let adapter = CameraLocalNetworkAdapter(sessionState: session)
        let evidence = CameraReadOnlyEvidence(MobileNovatekReadOnlySnapshotDto(
            firmwareVersion: "R3V1.1_20240411",
            movieRtspUri: "rtsp://192.168.1.254/xxx.mov",
            photoRtspUri: "rtsp://192.168.1.254/xxx.mov",
            configuration: [],
            storagePresent: true,
            media: []
        ))
        adapter.apply(readOnlyEvidence: evidence)
        adapter.observePreview(.live)
        adapter.observeOnboardRecording(.recording)

        adapter.apply(pathStatus: .unavailable, usesWiFi: false)

        XCTAssertNil(adapter.readOnlyEvidence)
        XCTAssertEqual(adapter.presentation.connection, .wifiRequired)
        XCTAssertEqual(session.cameraSnapshot().preview, .stopped)
        XCTAssertEqual(session.cameraSnapshot().onboardRecording, .unknown)
    }

    @MainActor
    func testSatisfiedWiFiPathPreservesConnectedCameraEvidence() {
        let adapter = CameraLocalNetworkAdapter()
        let evidence = CameraReadOnlyEvidence(MobileNovatekReadOnlySnapshotDto(
            firmwareVersion: "R3V1.1_20240411",
            movieRtspUri: "rtsp://192.168.1.254/xxx.mov",
            photoRtspUri: "rtsp://192.168.1.254/xxx.mov",
            configuration: [],
            storagePresent: true,
            media: []
        ))
        adapter.apply(readOnlyEvidence: evidence)

        adapter.apply(pathStatus: .satisfied, usesWiFi: true)

        XCTAssertEqual(adapter.presentation.connection, .connected)
        XCTAssertEqual(adapter.readOnlyEvidence, evidence)
    }

    @MainActor
    func testReadOnlyRefreshCannotRestoreEvidenceAfterWiFiLoss() async {
        let adapter = CameraLocalNetworkAdapter()
        let responses: [String: Data] = [
            "3012": Data("<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>".utf8),
            "2019": Data("<LIST><MovieLiveViewLink>rtsp://192.168.1.254/xxx.mov</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink></LIST>".utf8),
            "3014": Data("<Function><Cmd>2016</Cmd><Status>0</Status></Function>".utf8),
            "3024": Data("<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>".utf8),
            "3015": Data("<LIST></LIST>".utf8),
        ]
        do {
            _ = try await adapter.loadReadOnlyEvidence(
                address: "192.168.1.254",
                port: 80
            ) { url in
                let command = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first {
                    $0.name == "cmd"
                }?.value
                if command == "3012" {
                    await adapter.apply(pathStatus: .unavailable, usesWiFi: false)
                }
                return responses[command!]!
            }
            XCTFail("a refresh invalidated by Wi-Fi loss must not publish evidence")
        } catch let error as CameraReadOnlyRequestError {
            XCTAssertEqual(error, .pathUnavailable)
            XCTAssertNil(adapter.readOnlyEvidence)
            XCTAssertEqual(adapter.presentation.connection, .wifiRequired)
        } catch {
            XCTFail("unexpected error: \(error)")
            XCTAssertNil(adapter.readOnlyEvidence)
            XCTAssertEqual(adapter.presentation.connection, .wifiRequired)
        }
    }

    @MainActor
    func testPreviewAndRecordingObservationsShareRustStateWithoutInference() {
        let session = CutoutSessionStateHandle()
        let adapter = CameraLocalNetworkAdapter(sessionState: session)

        adapter.observeOnboardRecording(.recording)
        adapter.observePreview(.live)
        adapter.observePreview(.stopped)

        XCTAssertEqual(session.cameraSnapshot().preview, .stopped)
        XCTAssertEqual(session.cameraSnapshot().onboardRecording, .recording)
        XCTAssertEqual(adapter.presentation.preview, .stopped)
        XCTAssertEqual(adapter.presentation.recording, .recording)
    }

    @MainActor
    func testPreviewLifecycleForwardsTransportEventsToRust() {
        let session = CutoutSessionStateHandle()
        let adapter = CameraLocalNetworkAdapter(sessionState: session)

        adapter.startPreview()
        XCTAssertEqual(adapter.presentation.preview, .buffering)
        adapter.recordPreviewFrame()
        XCTAssertEqual(adapter.presentation.preview, .live)
        adapter.interruptPreview()
        XCTAssertEqual(adapter.presentation.preview, .interrupted)
        adapter.stopPreview()
        XCTAssertEqual(adapter.presentation.preview, .stopped)
    }

    @MainActor
    func testPreviewStartRejectsInvalidURIWithoutClaimingBuffering() async {
        let adapter = CameraLocalNetworkAdapter()

        do {
            try await adapter.startPreview(uri: "http://192.168.1.254/xxx.mov")
            XCTFail("non-RTSP URI should be rejected")
        } catch {
            XCTAssertEqual(adapter.presentation.preview, .stopped)
        }
    }

    @MainActor
    func testPreviewStartRejectsRTSPUriFromDifferentSelectedOriginBeforeNetwork() async {
        let adapter = CameraLocalNetworkAdapter()

        do {
            try await adapter.startPreview(
                uri: "rtsp://192.168.1.253/xxx.mov",
                expectedAddress: "192.168.1.254"
            )
            XCTFail("RTSP host mismatch must be rejected before network I/O")
        } catch {
            XCTAssertEqual(adapter.presentation.preview, .stopped)
        }
    }

    @MainActor
    func testPreviewStartRemainsBoundToTheReadOnlyOriginWhenAddressIsOverridden() async throws {
        let adapter = CameraLocalNetworkAdapter()
        let evidence = CameraReadOnlyEvidence(MobileNovatekReadOnlySnapshotDto(
            firmwareVersion: "R3V1.1_20240411",
            movieRtspUri: "rtsp://192.168.1.253/xxx.mov",
            photoRtspUri: "rtsp://192.168.1.253/xxx.mov",
            configuration: [],
            storagePresent: true,
            media: []
        ))
        let origin = try mobileValidateNovatekHttpOrigin(address: "192.168.1.254", port: 80)
        adapter.apply(readOnlyEvidence: evidence, origin: origin)

        do {
            try await adapter.startPreview(
                uri: evidence.movieRTSPURI,
                expectedAddress: "192.168.1.253"
            )
            XCTFail("an explicit preview address must not override the evidence origin")
        } catch let error as CameraReadOnlyRequestError {
            XCTAssertEqual(error, .originMismatch)
        }
        XCTAssertEqual(adapter.presentation.preview, .stopped)
    }

    @MainActor
    func testPreviewFileDestinationIsCreatedOnlyAfterRTSPNegotiation() async {
        let adapter = CameraLocalNetworkAdapter()
        let path = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cutout-camera-(UUID().uuidString).h264")

        do {
            try await adapter.startPreview(
                uri: "http://192.168.1.254/xxx.mov",
                saveTo: path
            )
            XCTFail("non-RTSP URI should be rejected")
        } catch {
            XCTAssertFalse(FileManager.default.fileExists(atPath: path.path))
            XCTAssertEqual(adapter.presentation.preview, .stopped)
        }
    }

    func testPreviewFileStatePublishesDestinationOnlyAfterTheFirstWrittenFrame() {
        let destination = URL(fileURLWithPath: "/tmp/camera-preview.h264")
        var state = CameraPreviewFileState(destination: destination)

        XCTAssertEqual(state.recordFrame(), destination)
        XCTAssertNil(state.recordFrame())
        XCTAssertEqual(state.frameCount, 2)

        var filelessState = CameraPreviewFileState(destination: nil)
        XCTAssertNil(filelessState.recordFrame())
        XCTAssertEqual(filelessState.frameCount, 0)
    }

    @MainActor
    func testMediaDownloadMapsCameraPathAndMovesFetchedFile() async throws {
        let media = CameraMediaEvidence(
            name: "clip.TS",
            path: #"A:\Novatek\Movie\clip.TS"#,
            sizeBytes: 4,
            timecode: 7,
            time: "2025/01/01 00:00:00",
            attributes: 32
        )
        let source = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cutout-camera-source-\(UUID().uuidString).tmp")
        let destination = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cutout-camera-download-\(UUID().uuidString).TS")
        try Data([1, 2, 3, 4]).write(to: source)
        defer {
            try? FileManager.default.removeItem(at: source)
            try? FileManager.default.removeItem(at: destination)
        }

        let requestedURL = DownloadURLCapture()
        try await CameraLocalNetworkAdapter().downloadMedia(
            address: "192.168.1.254",
            port: 80,
            media: media,
            to: destination
        ) { url in
            await requestedURL.record(url)
            return source
        }

        let actualURL = await requestedURL.value()
        XCTAssertEqual(actualURL?.path, "/Novatek/Movie/clip.TS")
        XCTAssertEqual(try Data(contentsOf: destination), Data([1, 2, 3, 4]))
    }

    @MainActor
    func testMediaDownloadCannotInstallFileAfterWiFiLoss() async {
        let adapter = CameraLocalNetworkAdapter()
        let media = CameraMediaEvidence(
            name: "clip.TS",
            path: #"A:\Novatek\Movie\clip.TS"#,
            sizeBytes: 4,
            timecode: 7,
            time: "2025/01/01 00:00:00",
            attributes: 32
        )
        let source = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cutout-camera-loss-source-\(UUID().uuidString).tmp")
        let destination = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cutout-camera-loss-destination-\(UUID().uuidString).TS")
        try? Data([1, 2, 3, 4]).write(to: source)
        defer {
            try? FileManager.default.removeItem(at: source)
            try? FileManager.default.removeItem(at: destination)
        }

        do {
            try await adapter.downloadMedia(
                address: "192.168.1.254",
                port: 80,
                media: media,
                to: destination
            ) { _ in
                await adapter.apply(pathStatus: .unavailable, usesWiFi: false)
                return source
            }
            XCTFail("a file fetched across Wi-Fi loss must not be installed")
        } catch let error as CameraMediaDownloadError {
            XCTAssertEqual(error, .pathUnavailable)
        } catch {
            XCTFail("unexpected error: \(error)")
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))
    }

    @MainActor
    func testMediaThumbnailUsesSourceBackedTargetWithoutMovingImageTypesAcrossFFI() async throws {
        let media = CameraMediaEvidence(
            name: "clip.TS",
            path: #"A:\Novatek\Movie\clip.TS"#,
            sizeBytes: 4,
            timecode: 7,
            time: "2025/01/01 00:00:00",
            attributes: 32
        )
        let requestedURL = DownloadURLCapture()

        let thumbnail = try await CameraLocalNetworkAdapter().fetchMediaThumbnail(
            address: "192.168.1.254",
            port: 80,
            media: media
        ) { url in
            await requestedURL.record(url)
            return Data([0xff, 0xd8, 0xff])
        }

        let actualURL = await requestedURL.value()
        XCTAssertEqual(actualURL?.path, "/Novatek/Movie/clip.TS")
        XCTAssertEqual(actualURL?.query, "custom=1&cmd=4001")
        XCTAssertEqual(thumbnail, Data([0xff, 0xd8, 0xff]))
    }

    @MainActor
    func testMediaThumbnailCannotPublishAfterWiFiLoss() async {
        let adapter = CameraLocalNetworkAdapter()
        let media = CameraMediaEvidence(
            name: "clip.TS",
            path: #"A:\Novatek\Movie\clip.TS"#,
            sizeBytes: 4,
            timecode: 7,
            time: "2025/01/01 00:00:00",
            attributes: 32
        )

        do {
            _ = try await adapter.fetchMediaThumbnail(
                address: "192.168.1.254",
                port: 80,
                media: media
            ) { _ in
                await adapter.apply(pathStatus: .unavailable, usesWiFi: false)
                return Data([0xff, 0xd8, 0xff])
            }
            XCTFail("a thumbnail fetched across Wi-Fi loss must not be published")
        } catch let error as CameraReadOnlyRequestError {
            XCTAssertEqual(error, .pathUnavailable)
        } catch {
            XCTFail("unexpected error: \(error)")
        }
    }

    @MainActor
    func testMediaThumbnailRejectsAnOversizedResponse() async {
        let adapter = CameraLocalNetworkAdapter()
        let media = CameraMediaEvidence(
            name: "clip.TS",
            path: #"A:\Novatek\Movie\clip.TS"#,
            sizeBytes: 4,
            timecode: 7,
            time: "2025/01/01 00:00:00",
            attributes: 32
        )

        do {
            _ = try await adapter.fetchMediaThumbnail(
                address: "192.168.1.254",
                port: 80,
                media: media
            ) { _ in
                Data(repeating: 0, count: 2 * 1024 * 1024 + 1)
            }
            XCTFail("an oversized thumbnail response must be rejected")
        } catch let error as CameraReadOnlyRequestError {
            XCTAssertEqual(error, .responseTooLarge)
        } catch {
            XCTFail("unexpected error: \(error)")
        }
    }

    @MainActor
    func testMediaDownloadRejectsFetchedSizeThatDisagreesWithCameraEvidence() async {
        let media = CameraMediaEvidence(
            name: "clip.TS",
            path: #"A:\Novatek\Movie\clip.TS"#,
            sizeBytes: 5,
            timecode: 7,
            time: "2025/01/01 00:00:00",
            attributes: 32
        )
        let source = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cutout-camera-size-mismatch-\(UUID().uuidString).tmp")
        let destination = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cutout-camera-size-mismatch-\(UUID().uuidString).TS")
        try? Data([1, 2, 3, 4]).write(to: source)
        defer {
            try? FileManager.default.removeItem(at: source)
            try? FileManager.default.removeItem(at: destination)
        }

        do {
            try await CameraLocalNetworkAdapter().downloadMedia(
                address: "192.168.1.254",
                port: 80,
                media: media,
                to: destination
            ) { _ in source }
            XCTFail("a downloaded file with the wrong size must not be installed")
        } catch let error as CameraMediaDownloadError {
            XCTAssertEqual(error, .sizeMismatch(expected: 5, actual: 4))
        } catch {
            XCTFail("unexpected error: \(error)")
        }

        XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: source.path))
    }

    @MainActor
    func testMediaDownloadRejectsUnsafeCameraPathBeforeFetching() async {
        let media = CameraMediaEvidence(
            name: "clip.TS",
            path: #"A:\Novatek\Movie\..\clip.TS"#,
            sizeBytes: 4,
            timecode: 7,
            time: "2025/01/01 00:00:00",
            attributes: 32
        )
        let destination = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("cutout-camera-download-\(UUID().uuidString).TS")

        do {
            try await CameraLocalNetworkAdapter().downloadMedia(
                address: "192.168.1.254",
                port: 80,
                media: media,
                to: destination
            ) { _ in
                XCTFail("unsafe camera paths must not reach the fetcher")
                return destination
            }
            XCTFail("unsafe camera path should be rejected")
        } catch {
            XCTAssertFalse(FileManager.default.fileExists(atPath: destination.path))
        }
    }

    @MainActor
    func testOnboardRecordingRequestUsesExplicitStartAndStopTargetsWithoutInference() async throws {
        let adapter = CameraLocalNetworkAdapter()
        let capture = RecordingRequestCapture()
        adapter.apply(readOnlyEvidence: cameraEvidence(advertisedCommandIDs: [2001]))

        let startOutcome = try await adapter.requestOnboardRecording(
            address: "192.168.1.254",
            port: 80,
            start: true
        ) { url in
            await capture.record(url)
            return Data("<Function><Cmd>2001</Cmd><Status>0</Status></Function>".utf8)
        }
        let stopOutcome = try await adapter.requestOnboardRecording(
            address: "192.168.1.254",
            port: 80,
            start: false
        ) { url in
            await capture.record(url)
            return Data("<Function><Cmd>2001</Cmd></Function>".utf8)
        }

        let values = await capture.values()
        XCTAssertEqual(values, [
            "/?custom=1&cmd=2001&str=1",
            "/?custom=1&cmd=2001&str=0",
        ])
        XCTAssertEqual(startOutcome, .acknowledged)
        XCTAssertEqual(stopOutcome, .unknown)
        XCTAssertEqual(adapter.presentation.recording, .unknown)
    }

    @MainActor
    func testCameraCommandOutcomesDistinguishRefusalTimeoutAndFailure() async throws {
        let adapter = CameraLocalNetworkAdapter()
        adapter.apply(readOnlyEvidence: cameraEvidence(advertisedCommandIDs: [1001, 2001]))

        let refused = try await adapter.requestStillCapture(
            address: "192.168.1.254",
            port: 80
        ) { _ in
            Data("<Function><Cmd>1001</Cmd><Status>7</Status></Function>".utf8)
        }
        XCTAssertEqual(refused, .refused)

        let timedOut = try await adapter.requestStillCapture(
            address: "192.168.1.254",
            port: 80
        ) { _ in
            throw URLError(.timedOut)
        }
        XCTAssertEqual(timedOut, .timedOut)

        let failed = try await adapter.requestStillCapture(
            address: "192.168.1.254",
            port: 80
        ) { _ in
            throw CameraTestError.transport
        }
        XCTAssertEqual(failed, .failed)
    }

    @MainActor
    func testCameraCommandsRequireReadOnlyCapabilityEvidence() async {
        let adapter = CameraLocalNetworkAdapter()

        do {
            _ = try await adapter.requestStillCapture(
                address: "192.168.1.254",
                port: 80
            ) { _ in
                XCTFail("commands without capability evidence must not reach the fetcher")
                return Data()
            }
            XCTFail("still capture must require read-only capability evidence")
        } catch let error as CameraCommandRequestError {
            XCTAssertEqual(error, .unsupported)
        } catch {
            XCTFail("unexpected error: \(error)")
        }
    }

    @MainActor
    func testCameraCommandsRemainBoundToTheReadOnlyOrigin() async throws {
        let adapter = CameraLocalNetworkAdapter()
        let responses: [String: Data] = [
            "3012": Data("<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>".utf8),
            "2019": Data("<LIST><MovieLiveViewLink>rtsp://192.168.1.254/xxx.mov</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink></LIST>".utf8),
            "3014": Data("<Function><Cmd>1001</Cmd><Status>0</Status></Function>".utf8),
            "3024": Data("<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>".utf8),
            "3015": Data("<LIST></LIST>".utf8),
        ]
        _ = try await adapter.loadReadOnlyEvidence(
            address: "192.168.1.254",
            port: 80
        ) { url in
            let command = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first {
                $0.name == "cmd"
            }?.value
            return responses[command!]!
        }

        do {
            _ = try await adapter.requestStillCapture(
                address: "192.168.1.253",
                port: 80
            ) { _ in
                XCTFail("a command for a different local origin must not reach the fetcher")
                return Data()
            }
            XCTFail("camera commands must remain bound to the evidence origin")
        } catch let error as CameraCommandRequestError {
            XCTAssertEqual(error, .originMismatch)
        }
    }

    func testCameraURLResponsesMustRemainOnTheSelectedOrigin() throws {
        let origin = try mobileValidateNovatekHttpOrigin(address: "192.168.1.254", port: 80)
        let sameOrigin = try XCTUnwrap(URL(string: "http://192.168.1.254/?custom=1&cmd=3012"))
        let redirectedOrigin = try XCTUnwrap(URL(string: "http://192.168.1.253/?custom=1&cmd=3012"))

        XCTAssertTrue(
            cameraResponseMatchesOrigin(
                URLResponse(url: sameOrigin, mimeType: nil, expectedContentLength: 0, textEncodingName: nil),
                origin: origin
            )
        )
        XCTAssertFalse(
            cameraResponseMatchesOrigin(
                URLResponse(url: redirectedOrigin, mimeType: nil, expectedContentLength: 0, textEncodingName: nil),
                origin: origin
            )
        )
    }

    @MainActor
    func testCameraCommandCannotPublishResponseAfterWiFiLoss() async {
        let adapter = CameraLocalNetworkAdapter()
        adapter.apply(readOnlyEvidence: cameraEvidence(advertisedCommandIDs: [1001, 2001]))

        do {
            _ = try await adapter.requestStillCapture(
                address: "192.168.1.254",
                port: 80
            ) { _ in
                await adapter.apply(pathStatus: .unavailable, usesWiFi: false)
                return Data("<Function><Cmd>1001</Cmd><Status>0</Status></Function>".utf8)
            }
            XCTFail("a command response fetched across Wi-Fi loss must not be published")
        } catch let error as CameraCommandRequestError {
            XCTAssertEqual(error, .pathUnavailable)
        } catch {
            XCTFail("unexpected error: \(error)")
        }
    }

    @MainActor
    func testCameraCommandRejectsASecondRequestWhileTheFirstIsInFlight() async throws {
        let adapter = CameraLocalNetworkAdapter()
        let gate = CameraCommandGate()
        adapter.apply(readOnlyEvidence: cameraEvidence(advertisedCommandIDs: [1001, 2001]))
        let first = Task { @MainActor in
            try await adapter.requestStillCapture(
                address: "192.168.1.254",
                port: 80
            ) { _ in
                await gate.wait()
            }
        }

        while await gate.started() == false {
            await Task.yield()
        }

        do {
            _ = try await adapter.requestOnboardRecording(
                address: "192.168.1.254",
                port: 80,
                start: true
            ) { _ in
                XCTFail("a second command must not reach the transport")
                return Data()
            }
            XCTFail("a second command must be rejected while one is in flight")
        } catch let error as CameraCommandRequestError {
            XCTAssertEqual(error, .inFlight)
        }

        await gate.release(
            Data("<Function><Cmd>1001</Cmd><Status>0</Status></Function>".utf8)
        )
        let firstOutcome = try await first.value
        XCTAssertEqual(firstOutcome, .acknowledged)
    }

    @MainActor
    func testOnboardRecordingRequestRejectsPublicOriginBeforeFetching() async {
        do {
            let _ = try await CameraLocalNetworkAdapter().requestOnboardRecording(
                address: "8.8.8.8",
                port: 80,
                start: true
            ) { _ in
                XCTFail("public origins must not reach the recording fetcher")
                return Data()
            }
            XCTFail("public origins should be rejected")
        } catch {
            XCTAssertTrue(error is MobileNovatekOriginError)
        }
    }

    @MainActor
    func testStillCaptureRequestUsesExplicitTargetWithoutClaimingMediaReadback() async throws {
        let capture = RecordingRequestCapture()
        let adapter = CameraLocalNetworkAdapter()
        adapter.apply(readOnlyEvidence: cameraEvidence(advertisedCommandIDs: [1001]))

        let _ = try await adapter.requestStillCapture(
            address: "192.168.1.254",
            port: 80
        ) { url in
            await capture.record(url)
            return Data()
        }

        let values = await capture.values()
        XCTAssertEqual(values, ["/?custom=1&cmd=1001"])
    }
}

private func cameraEvidence(advertisedCommandIDs: [UInt16]) -> CameraReadOnlyEvidence {
    CameraReadOnlyEvidence(MobileNovatekReadOnlySnapshotDto(
        firmwareVersion: "R3V1.1_20240411",
        movieRtspUri: "rtsp://192.168.1.254/xxx.mov",
        photoRtspUri: "rtsp://192.168.1.254/xxx.mov",
        configuration: advertisedCommandIDs.map {
            MobileNovatekCommandStatusDto(commandId: $0, status: 0)
        },
        storagePresent: true,
        media: []
    ))
}

private actor DownloadURLCapture {
    private var recordedURL: URL?

    func record(_ url: URL) {
        recordedURL = url
    }

    func value() -> URL? {
        recordedURL
    }
}

private actor RecordingRequestCapture {
    private var urls: [URL] = []

    func record(_ url: URL) {
        urls.append(url)
    }

    func values() -> [String] {
        urls.map(\.pathAndQuery)
    }
}

private actor CameraCommandGate {
    private var didStart = false
    private var continuation: CheckedContinuation<Data, Never>?

    func wait() async -> Data {
        didStart = true
        return await withCheckedContinuation { continuation = $0 }
    }

    func started() -> Bool {
        didStart
    }

    func release(_ response: Data) {
        continuation?.resume(returning: response)
        continuation = nil
    }
}

private enum CameraTestError: Error {
    case transport
}

private extension URL {
    var pathAndQuery: String {
        var components = URLComponents(url: self, resolvingAgainstBaseURL: false)
        components?.scheme = nil
        components?.host = nil
        components?.port = nil
        return components?.string ?? absoluteString
    }
}
