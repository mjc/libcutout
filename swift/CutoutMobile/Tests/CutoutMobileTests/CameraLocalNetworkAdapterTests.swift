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
        XCTAssertEqual(adapter.presentation.profileName, "Novatek R3 Pro")
        XCTAssertEqual(adapter.presentation.storage, .present)
        XCTAssertEqual(adapter.presentation.preview, .stopped)
        XCTAssertEqual(adapter.presentation.recording, .unknown)
        XCTAssertEqual(adapter.readOnlyEvidence?.movieRTSPURI, "rtsp://192.168.1.254/xxx.mov")

        adapter.stop()
        XCTAssertNil(adapter.readOnlyEvidence)
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
}
