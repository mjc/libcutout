#if os(iOS)
    import CutoutMobileFFI
    import Foundation
    import XCTest

    final class MobileReplacementVectorTests: XCTestCase {
        func testNovatekSnapshotDecodesFixedResponsesThroughRustFFI() throws {
            let snapshot = try novatekSnapshot()
            XCTAssertEqual(snapshot.firmwareVersion, "R3V1.1_20240411")
            XCTAssertEqual(snapshot.movieRtspUri, "rtsp://192.168.1.254/xxx.mov")
            XCTAssertEqual(snapshot.photoRtspUri, "rtsp://192.168.1.254/xxx.mov")
            XCTAssertEqual(snapshot.configuration.map(\.commandId), [2016, 1001])
            XCTAssertEqual(snapshot.configuration.map(\.status), [0, 7])
            XCTAssertTrue(snapshot.storagePresent)
            let media = try XCTUnwrap(snapshot.media?.first)
            XCTAssertEqual(snapshot.media?.count, 1)
            XCTAssertEqual(media.name, "clip.TS")
            XCTAssertEqual(media.path, "A:\\Novatek\\Movie\\clip.TS")
            XCTAssertEqual(media.sizeBytes, 42)
            XCTAssertEqual(media.timecode, 7)
            XCTAssertEqual(media.time, "2025/01/01 00:00:00")
            XCTAssertTrue(try XCTUnwrap(novatekSnapshot(media: "<LIST></LIST>").media).isEmpty)
        }

        func testNovatekSnapshotRejectsMalformedUnsupportedAndOversizedXmlThroughRustFFI() {
            let rejectedFirmware = [
                "<Function><Cmd>3012</Cmd><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>",
                "<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function><unfinished>",
                "<Function source=\"camera\"><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>",
                "<Function><!-- unsupported --><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>",
                String(repeating: " ", count: 4_097),
            ]
            for firmware in rejectedFirmware {
                XCTAssertThrowsError(try novatekSnapshot(firmware: firmware)) { error in
                    guard case MobileNovatekParseError.InvalidResponse = error else {
                        return XCTFail("Unexpected FFI parser error: \(error)")
                    }
                }
            }
        }

        func testVescXmodemFrameAcceptsFixedChecksumRejectsCorruptionAndRecoversThroughRustFFI() throws {
            // Fixed firmware reply: the 20-byte payload has CRC-16/XMODEM 0x26d0.
            let frame = Data([
                2, 20, 157, 7, 1, 2, 97, 98, 99, 49, 50, 51, 0, 117, 115, 101, 114, 104, 97, 115,
                104, 0, 38, 208, 3,
            ])
            let state = CutoutSessionStateHandle()
            let token = try XCTUnwrap(
                state.beginConnectionAttempt(platformIdentifier: "ios-crc-vector", nowMs: 0).token)
            _ = state.connectionLinkEstablished(token: token)
            var corrupt = frame
            corrupt[22] ^= 1
            let rejected = try XCTUnwrap(state.observeConnectionNotification(token: token, bytes: corrupt))
            XCTAssertNil(rejected.protocolFamily)
            XCTAssertNil(state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 1).identity)
            let partial = try XCTUnwrap(state.observeConnectionNotification(token: token, bytes: frame.prefix(12)))
            XCTAssertNil(partial.protocolFamily)
            let accepted = try XCTUnwrap(state.observeConnectionNotification(token: token, bytes: frame.dropFirst(12)))
            XCTAssertEqual(accepted.protocolFamily, .vesc)
            let identity = try XCTUnwrap(
                state.resolveDeviceSession(token: token, identificationComplete: false, nowMs: 2).identity)
            XCTAssertEqual(identity.protocol, .vesc)
        }

        private func novatekSnapshot(
            firmware: String = "<Function><Cmd>3012</Cmd><Status>0</Status><String>R3V1.1_20240411</String></Function>",
            media: String =
                "<LIST><File><NAME>clip.TS</NAME><FPATH>A:\\Novatek\\Movie\\clip.TS</FPATH><SIZE>42</SIZE><TIMECODE>7</TIMECODE><TIME>2025/01/01 00:00:00</TIME><ATTR>32</ATTR></File></LIST>"
        ) throws -> MobileNovatekReadOnlySnapshotDto {
            try mobileParseNovatekReadOnlySnapshot(
                firmwareResponse: Data(firmware.utf8),
                liveViewResponse: Data(
                    "<LIST><MovieLiveViewLink>rtsp://192.168.1.254/xxx.mov</MovieLiveViewLink><PhotoLiveViewLink>rtsp://192.168.1.254/xxx.mov</PhotoLiveViewLink></LIST>"
                        .utf8),
                configurationResponse: Data(
                    "<Function><Cmd>2016</Cmd><Status>0</Status><Cmd>1001</Cmd><Status>7</Status></Function>".utf8),
                storageResponse: Data("<Function><Cmd>3024</Cmd><Status>0</Status><Value>1</Value></Function>".utf8),
                mediaResponse: Data(media.utf8)
            )
        }
    }
#endif
