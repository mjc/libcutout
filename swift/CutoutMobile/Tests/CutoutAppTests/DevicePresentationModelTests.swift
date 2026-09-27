import Foundation
import XCTest

@testable import CutoutApp
@testable import CutoutMobile

final class DevicePresentationModelTests: XCTestCase {
    func testConnectionStateOwnsNavigationIntent() {
        let selection = ConnectionSelection(
            platformIdentifier: "vesc-1234",
            title: "VESC",
            route: .vescOnewheel
        )

        XCTAssertEqual(
            ConnectionState.connected(selection).navigationIntent(isRecordOnlyCapture: false),
            .openRide(.vescOnewheel)
        )
        XCTAssertEqual(
            ConnectionState.retrying(
                selection,
                retry: SessionConnectionRetry(
                    platformIdentifier: selection.platformIdentifier,
                    attempt: 1,
                    deadline: MonotonicMilliseconds(0),
                    failure: .connectFailed("timed out")
                )
            ).navigationIntent(isRecordOnlyCapture: false),
            .stay
        )
        XCTAssertEqual(
            ConnectionState.failed(selection, .connectFailed("timed out")).navigationIntent(isRecordOnlyCapture: false),
            .returnToPicker
        )
        XCTAssertEqual(
            ConnectionState.picker.navigationIntent(isRecordOnlyCapture: false),
            .returnToPicker
        )
        XCTAssertEqual(
            ConnectionState.connected(selection).navigationIntent(isRecordOnlyCapture: true),
            .openCapture
        )
    }

    func testConnectionStateOwnsSelectedDeviceStatusText() {
        let selection = ConnectionSelection(
            platformIdentifier: "vesc-1234",
            title: "VESC",
            route: .vescOnewheel
        )
        let failure = SessionConnectionFailure.connectFailed("timed out")

        XCTAssertEqual(
            ConnectionState.connecting(selection, phase: .discoveringServices).statusText,
            SessionConnectionPhase.discoveringServices.displayText
        )
        XCTAssertEqual(
            ConnectionState.retrying(
                selection,
                retry: SessionConnectionRetry(
                    platformIdentifier: selection.platformIdentifier,
                    attempt: 1,
                    deadline: MonotonicMilliseconds(0),
                    failure: failure
                )
            ).statusText,
            localizedAppText("picker.status.retrying")
        )
        XCTAssertEqual(
            ConnectionState.connected(selection).statusText,
            SessionConnectionPhase.live.displayText
        )
        XCTAssertEqual(
            ConnectionState.failed(selection, failure).statusText,
            SessionConnectionPhase.failed(failure).displayText
        )
        XCTAssertNil(ConnectionState.picker.statusText)
    }

    @MainActor
    func testOwnsPersistedVehicleNames() throws {
        let suiteName = "DevicePresentationModelTests.deviceNames.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let selectionStore = DevicePickerSelectionStore(defaults: defaults)
        selectionStore.save(platformIdentifier: "wheel-1", displayName: "Saved wheel")
        let model = DevicePresentationModel(selectedDeviceStore: selectionStore)

        XCTAssertEqual(model.rideMapVehicleIdentity, "wheel-1")
        XCTAssertEqual(model.rideMapVehicleName, "Saved wheel")

        model.rememberVehicleName("NF2557", for: "wheel-1")
        XCTAssertEqual(model.rideMapVehicleName, "NF2557")
    }

    @MainActor
    func testCommitsPairSelectionOnlyAfterAcceptedAction() throws {
        let suiteName = "DevicePresentationModelTests.devicePair.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = DevicePickerSelectionStore(defaults: defaults)
        let model = DevicePresentationModel(selectedDeviceStore: store)
        let row = DevicePickerRow(
            id: "wheel-1",
            title: "Saved wheel",
            subtitle: "Electric unicycle",
            detail: "Supported device",
            state: .supported(action: "Connect"),
            symbolName: "circle.hexagongrid.circle",
            connectionRoute: .electricUnicycle
        )
        model.scanState = DevicePickerScanState(status: .scanning, rows: [row])

        let refused = model.pair(
            platformIdentifier: row.id,
            mayRetryCurrentSelection: false
        ) { _ in false }
        guard case .refused = refused else {
            return XCTFail("expected a refused pair action")
        }
        XCTAssertNil(store.platformIdentifier)
        XCTAssertEqual(model.connectionState, .picker)

        let accepted = model.pair(
            platformIdentifier: row.id,
            mayRetryCurrentSelection: false
        ) { $0 == row }
        guard case let .accepted(acceptedRow) = accepted else {
            return XCTFail("expected the selected row to be accepted")
        }
        XCTAssertEqual(acceptedRow, row)
        XCTAssertEqual(store.platformIdentifier, row.id)
        XCTAssertEqual(store.displayName(for: row.id), row.title)
        XCTAssertTrue(model.hasSavedDevice)
    }
}
