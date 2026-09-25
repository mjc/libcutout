import XCTest
@testable import CutoutApp
@testable import CutoutMobile
import CutoutMobileFFI

@MainActor
final class MusicFeatureModelTests: XCTestCase {
    func testDeletingHistoryInvalidatesDetailBeforeRustDeleteAndClearsCaptureBeforeSelection() throws {
        let state = MobileRideMapState()
        _ = try state.startGpsOnly(atMs: 100)
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let policyStore = MusicHistoryPolicyStore(defaults: suite.defaults)
        let selectedRideID = state.currentSnapshot()?.rideID
        var order = [String]()
        let model = makeModel(
            state: state,
            defaults: suite.defaults,
            historyPolicyStore: policyStore,
            selectedRideID: { selectedRideID },
            invalidateHistory: {
                XCTAssertFalse(state.currentMusicHistory()?.events.isEmpty ?? true)
                order.append("invalidate")
            },
            updateCaptureObservation: { observation in
                if observation == nil { order.append("capture-clear") }
            },
            clearSelectedHistoryMusic: { order.append("selection-clear") }
        )
        XCTAssertTrue(model.setHistoryPolicy(.humanReadable))
        XCTAssertTrue(model.ingestObservation(observation(atMs: 200), wallClockAtMs: 1_700_000_000_000))
        let rideID = try XCTUnwrap(state.currentSnapshot()?.rideID)

        XCTAssertTrue(model.forgetHistory(for: rideID))

        XCTAssertEqual(order, ["invalidate", "capture-clear", "selection-clear"])
        XCTAssertEqual(model.historyPolicy, .disabled)
        XCTAssertTrue(model.timelineEvents.isEmpty)
        XCTAssertEqual(policyStore.policy, .humanReadable)
        XCTAssertTrue(state.currentMusicEvents().isEmpty)
    }

    func testHiddenPlayerKeepsSettingsProjectionAndMonitoringIntentIndependent() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let visibility = MusicPlayerVisibilityStore()
        let wasHidden = visibility.isHidden
        visibility.setHidden(false)
        defer { visibility.setHidden(wasHidden) }
        let monitoring = MusicMonitoringPreferenceStore(defaults: suite.defaults)
        monitoring.setEnabled(false)
        let model = makeModel(defaults: suite.defaults, monitoringPreferenceStore: monitoring)
        XCTAssertTrue(model.ingestObservation(observation(atMs: 10)))
        let settingsProjection = model.settingsNowPlaying

        model.dismissPlayer()

        XCTAssertTrue(model.isPlayerHidden)
        XCTAssertNil(model.nowPlaying)
        XCTAssertEqual(model.settingsNowPlaying, settingsProjection)
        XCTAssertFalse(monitoring.isEnabled)
    }

    func testLateCommandFeedbackCannotOverwriteAfterProviderSwitch() throws {
        let suite = try makeDefaults()
        defer { suite.defaults.removePersistentDomain(forName: suite.name) }
        let model = makeModel(defaults: suite.defaults)
        let requestID = try XCTUnwrap(model.beginCommandFeedback())
        XCTAssertNotNil(model.commandFeedback)

        model.selectProvider(.spotify)
        _ = model.finishCommand(.failed, provider: .appleMusic, requestID: requestID)

        XCTAssertNil(model.commandFeedback)
    }

    private func makeModel(
        state: MobileRideMapState? = nil,
        defaults: UserDefaults,
        historyPolicyStore: MusicHistoryPolicyStore? = nil,
        monitoringPreferenceStore: MusicMonitoringPreferenceStore? = nil,
        selectedRideID: @escaping @MainActor () -> String? = { nil },
        invalidateHistory: @escaping @MainActor () -> Void = {},
        updateCaptureObservation: @escaping @MainActor (MobilePevcapMusicEventDto?) -> Void = { _ in },
        clearSelectedHistoryMusic: @escaping @MainActor () -> Void = {},
        setRideHistoryError: @escaping @MainActor (MobileRideMapError) -> Void = { _ in }
    ) -> MusicFeatureModel {
        MusicFeatureModel(
            providerSelectionStore: MusicProviderSelectionStore(defaults: defaults),
            historyPolicyStore: historyPolicyStore ?? MusicHistoryPolicyStore(defaults: defaults),
            monitoringPreferenceStore: monitoringPreferenceStore ?? MusicMonitoringPreferenceStore(defaults: defaults),
            rideMapState: state,
            monotonicNow: { 1_000 },
            updateCapturePolicy: { _ in },
            updateCaptureObservation: updateCaptureObservation,
            invalidateHistoryForDeletion: invalidateHistory,
            selectedHistoryRideID: selectedRideID,
            clearSelectedHistoryMusic: clearSelectedHistoryMusic,
            setRideHistoryError: setRideHistoryError
        )
    }

    private func makeDefaults() throws -> (name: String, defaults: UserDefaults) {
        let name = "MusicFeatureModelTests-\(UUID().uuidString)"
        return (name, try XCTUnwrap(UserDefaults(suiteName: name)))
    }

    private func observation(atMs: UInt64) -> MusicProviderObservation {
        MusicProviderObservation(snapshot: MobileMusicSnapshotDto(
            provider: .appleMusic,
            sessionId: "feature-test",
            state: .playing,
            item: MobileMusicItemDto(identifier: "track-1", title: "Track", artist: "Artist"),
            positionMilliseconds: nil,
            durationMilliseconds: nil,
            observedAtMs: atMs,
            capabilities: MobileMusicCapabilitiesDto(
                previous: false,
                play: true,
                pause: true,
                next: false,
                openProvider: true
            )
        ))
    }
}
