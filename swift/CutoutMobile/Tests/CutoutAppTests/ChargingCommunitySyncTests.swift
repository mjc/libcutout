import CutoutMobileFFI
import XCTest

@testable import CutoutApp

@MainActor
final class ChargingCommunitySyncTests: XCTestCase {
    func testDebugEnablementDisablesSyncForEveryUIFixtureSource() {
        XCTAssertFalse(
            ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                environmentFixtureValue: "bluetooth-unavailable",
                persistedFixtureValue: nil,
                arguments: [],
                chargingFixtureEnabled: false
            ))
        XCTAssertFalse(
            ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                environmentFixtureValue: nil,
                persistedFixtureValue: "euc",
                arguments: [],
                chargingFixtureEnabled: false
            ))
        XCTAssertFalse(
            ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                environmentFixtureValue: nil,
                persistedFixtureValue: nil,
                arguments: ["-CUTOUT_UI_TEST_FIXTURE", "vesc"],
                chargingFixtureEnabled: false
            ))
        XCTAssertFalse(
            ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                environmentFixtureValue: nil,
                persistedFixtureValue: nil,
                arguments: ["--seed-ui-test-charging-spots"],
                chargingFixtureEnabled: false
            ))
        XCTAssertFalse(
            ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                environmentFixtureValue: nil,
                persistedFixtureValue: nil,
                arguments: ["--seed-ui-test-ride-history"],
                chargingFixtureEnabled: false
            ))
        XCTAssertFalse(
            ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                environmentFixtureValue: nil,
                persistedFixtureValue: nil,
                arguments: ["--seed-ui-test-live-route"],
                chargingFixtureEnabled: false
            ))
        XCTAssertFalse(
            ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                environmentFixtureValue: nil,
                persistedFixtureValue: nil,
                arguments: ["--ui-test-ride-secondary-readings", "--fail-ui-test-history-refresh-once"],
                chargingFixtureEnabled: false
            ))
        XCTAssertFalse(
            ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                environmentFixtureValue: nil,
                persistedFixtureValue: nil,
                arguments: [],
                chargingFixtureEnabled: true
            ))
        XCTAssertTrue(
            ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                environmentFixtureValue: nil,
                persistedFixtureValue: nil,
                arguments: ["--ordinary-launch-option"],
                chargingFixtureEnabled: false
            ))
    }

    func testRegistrationIsPersistedBeforeSyncAndQueuedWorkIsReported() async throws {
        let store = MemoryChargingCredentialStore()
        let recorder = SyncRecorder()
        let credential = ChargingCommunityCredential(identityID: "identity", token: "secret")
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                recorder.registrations += 1
                return credential
            },
            synchronize: { token in
                recorder.tokens.append(token)
                recorder.sawPersistedCredential = try await store.load() == .ready(credential)
                return ChargingCommunitySyncCounts(
                    pendingCount: 2, downloaded: 3, uploaded: 1, moreWork: false
                )
            },
            sleep: { _ in },
            enabled: { true }
        )

        let result = await sync.run()

        XCTAssertEqual(result, .pending)
        XCTAssertEqual(recorder.registrations, 1)
        XCTAssertEqual(recorder.tokens, ["secret"])
        XCTAssertTrue(recorder.sawPersistedCredential)
        XCTAssertEqual(sync.pendingCount, 2)
        XCTAssertEqual(sync.downloaded, 3)
        XCTAssertEqual(sync.uploaded, 1)
        let stored = try await store.load()
        XCTAssertEqual(stored, .ready(credential))
    }

    func testKeychainFailureDoesNotRegisterAnotherIdentity() async {
        let store = MemoryChargingCredentialStore(failLoad: .locked)
        let recorder = SyncRecorder()
        let cachedDate = Date(timeIntervalSince1970: 1_700_000_000)
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                recorder.registrations += 1
                return ChargingCommunityCredential(identityID: "new", token: "new-token")
            },
            synchronize: { _ in
                XCTFail("Must not sync without a readable credential")
                throw MobileChargingError.Network
            },
            refreshDirectory: {
                recorder.directoryRefreshes += 1
                return ChargingCommunitySyncCounts(
                    pendingCount: 0, downloaded: 4, uploaded: 0, moreWork: false
                )
            },
            readLastUpdated: { UInt64(cachedDate.timeIntervalSince1970 * 1_000) },
            sleep: { _ in },
            enabled: { true }
        )

        let result = await sync.run()
        XCTAssertEqual(result, .storageUnavailable)
        XCTAssertEqual(recorder.registrations, 0)
        XCTAssertEqual(recorder.syncCalls, 0)
        XCTAssertEqual(recorder.directoryRefreshes, 1)
        XCTAssertEqual(sync.downloaded, 4)
        XCTAssertEqual(sync.lastUpdated, cachedDate)
    }

    func testSkippedPublicRefreshKeepsPersistedCacheDate() async {
        let store = MemoryChargingCredentialStore(failLoad: .locked)
        let recorder = SyncRecorder()
        let cachedDate = Date(timeIntervalSince1970: 1_700_000_123)
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                XCTFail("A locked credential must not register")
                return .init(identityID: "", token: "")
            },
            synchronize: { _ in
                XCTFail("A locked credential must not run authenticated sync")
                throw MobileChargingError.Network
            },
            refreshDirectory: {
                recorder.directoryRefreshes += 1
                return ChargingCommunitySyncCounts(
                    pendingCount: 1, downloaded: 0, uploaded: 0, moreWork: true
                )
            },
            readLastUpdated: { UInt64(cachedDate.timeIntervalSince1970 * 1_000) },
            sleep: { _ in },
            enabled: { true }
        )

        let result = await sync.run()

        XCTAssertEqual(result, .storageUnavailable)
        XCTAssertEqual(recorder.directoryRefreshes, 3)
        XCTAssertEqual(sync.downloaded, 0)
        XCTAssertEqual(sync.lastUpdated, cachedDate)
    }

    func testUncertainRegistrationRequiresExplicitRetry() async {
        let store = MemoryChargingCredentialStore(initial: .registrationStarted)
        let recorder = SyncRecorder()
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                recorder.registrations += 1
                return ChargingCommunityCredential(identityID: "identity", token: "secret")
            },
            synchronize: { _ in
                ChargingCommunitySyncCounts(
                    pendingCount: 0, downloaded: 0, uploaded: 0, moreWork: false
                )
            },
            sleep: { _ in },
            enabled: { true }
        )

        let firstResult = await sync.run()
        XCTAssertEqual(firstResult, .registrationUncertain)
        XCTAssertEqual(recorder.registrations, 0)
        let retryResult = await sync.run(explicitRetry: true)
        XCTAssertEqual(retryResult, .synced)
        XCTAssertEqual(recorder.registrations, 1)
    }

    func testConcurrentRequestsShareOneSyncAndUnauthorizedKeepsCredential() async throws {
        let credential = ChargingCommunityCredential(identityID: "identity", token: "secret")
        let store = MemoryChargingCredentialStore(initial: .ready(credential))
        let recorder = SyncRecorder()
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                recorder.registrations += 1
                return ChargingCommunityCredential(identityID: "replacement", token: "replacement")
            },
            synchronize: { _ in
                recorder.syncCalls += 1
                try await Task.sleep(for: .milliseconds(20))
                throw MobileChargingError.Unauthorized
            },
            sleep: { _ in },
            enabled: { true }
        )

        async let first = sync.run()
        async let second = sync.run()
        let results = await (first, second)

        XCTAssertEqual(results.0, .authenticationRequired)
        XCTAssertEqual(results.1, .authenticationRequired)
        XCTAssertEqual(recorder.syncCalls, 1)
        XCTAssertEqual(recorder.registrations, 0)
        let stored = try await store.load()
        XCTAssertEqual(stored, .ready(credential))
    }

    func testNetworkRetriesAreBounded() async {
        let store = MemoryChargingCredentialStore(
            initial: .ready(ChargingCommunityCredential(identityID: "identity", token: "secret"))
        )
        let recorder = SyncRecorder()
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                XCTFail("Existing identity must be reused")
                return .init(identityID: "", token: "")
            },
            synchronize: { _ in
                recorder.syncCalls += 1
                throw MobileChargingError.Network
            },
            sleep: { _ in recorder.backoffCount += 1 },
            enabled: { true }
        )

        let result = await sync.run()
        XCTAssertEqual(result, .offline)
        XCTAssertEqual(recorder.syncCalls, 3)
        XCTAssertEqual(recorder.backoffCount, 2)
    }
}

private actor MemoryChargingCredentialStore: ChargingCredentialStore {
    private var record: ChargingCredentialRecord?
    private let loadFailure: ChargingCredentialStoreError?

    init(
        initial: ChargingCredentialRecord? = nil,
        failLoad: ChargingCredentialStoreError? = nil
    ) {
        record = initial
        loadFailure = failLoad
    }

    func load() async throws -> ChargingCredentialRecord? {
        if let loadFailure { throw loadFailure }
        return record
    }

    func beginRegistration() async throws -> Bool {
        guard record == nil else { return false }
        record = .registrationStarted
        return true
    }

    func save(_ credential: ChargingCommunityCredential) async throws {
        guard record == .registrationStarted else {
            throw ChargingCredentialStoreError.unavailable
        }
        record = .ready(credential)
    }
}

@MainActor
private final class SyncRecorder {
    var registrations = 0
    var syncCalls = 0
    var directoryRefreshes = 0
    var backoffCount = 0
    var tokens = [String]()
    var sawPersistedCredential = false
}
