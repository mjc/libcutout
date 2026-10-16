import CutoutMobileFFI
import XCTest

@testable import CutoutApp

@MainActor
final class ChargingCommunitySyncTests: XCTestCase {
    func testRoutineSyncAndDeletedAccountHaveNoStatusMessage() {
        for status in [ChargingCommunitySyncStatus.notStarted, .syncing, .synced, .deleted] {
            XCTAssertNil(status.messageKey)
        }
        XCTAssertFalse(ChargingCommunitySyncStatus.deleted.canRetry)
        XCTAssertNotNil(ChargingCommunitySyncStatus.deletionPending.messageKey)
        XCTAssertFalse(ChargingCommunitySyncStatus.deletionPending.canRetry)
        XCTAssertNotNil(ChargingCommunitySyncStatus.offline.messageKey)
        XCTAssertNotNil(ChargingCommunitySyncStatus.conflict.messageKey)
    }

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
                    pendingCount: 2, downloaded: 3, uploaded: 1, outcome: .finished(result: .pending)
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
                    pendingCount: 0, downloaded: 4, uploaded: 0, outcome: .finished(result: .synced)
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
                    pendingCount: 1, downloaded: 0, uploaded: 0, outcome: .moreWork
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
                    pendingCount: 0, downloaded: 0, uploaded: 0, outcome: .finished(result: .synced)
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

    func testLostRegistrationClaimDoesNotRetryIdentityCreationOnExplicitRetry() async {
        let store = MemoryChargingCredentialStore(
            loseRegistrationClaim: true,
            recordAfterLostClaim: .registrationStarted
        )
        let recorder = SyncRecorder()
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                recorder.registrations += 1
                return ChargingCommunityCredential(identityID: "identity", token: "secret")
            },
            synchronize: { _ in
                recorder.syncCalls += 1
                return ChargingCommunitySyncCounts(
                    pendingCount: 0, downloaded: 0, uploaded: 0, outcome: .finished(result: .synced)
                )
            },
            refreshDirectory: {
                recorder.directoryRefreshes += 1
                return ChargingCommunitySyncCounts(
                    pendingCount: 0, downloaded: 0, uploaded: 0, outcome: .finished(result: .synced)
                )
            },
            sleep: { _ in },
            enabled: { true }
        )

        let result = await sync.run(explicitRetry: true)

        XCTAssertEqual(result, .registrationUncertain)
        XCTAssertEqual(recorder.registrations, 0)
        XCTAssertEqual(recorder.syncCalls, 0)
        XCTAssertEqual(recorder.directoryRefreshes, 1)
    }

    func testLostRegistrationClaimUsesAReadyCredentialWithoutRegistering() async {
        let credential = ChargingCommunityCredential(identityID: "identity", token: "secret")
        let store = MemoryChargingCredentialStore(
            loseRegistrationClaim: true,
            recordAfterLostClaim: .ready(credential)
        )
        let recorder = SyncRecorder()
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                recorder.registrations += 1
                return ChargingCommunityCredential(identityID: "new", token: "new-token")
            },
            synchronize: { token in
                recorder.tokens.append(token)
                return ChargingCommunitySyncCounts(
                    pendingCount: 0, downloaded: 0, uploaded: 0,
                    outcome: .finished(result: .synced)
                )
            },
            sleep: { _ in },
            enabled: { true }
        )

        let result = await sync.run(explicitRetry: true)

        XCTAssertEqual(result, .synced)
        XCTAssertEqual(recorder.registrations, 0)
        XCTAssertEqual(recorder.tokens, ["secret"])
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

    func testFirstErrorOfNewRunKeepsLastKnownPendingCount() async {
        let store = MemoryChargingCredentialStore(
            initial: .ready(ChargingCommunityCredential(identityID: "identity", token: "secret"))
        )
        var syncCalls = 0
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                XCTFail("Existing identity must be reused")
                return .init(identityID: "", token: "")
            },
            synchronize: { _ in
                syncCalls += 1
                if syncCalls == 1 {
                    return ChargingCommunitySyncCounts(
                        pendingCount: 6, downloaded: 0, uploaded: 0, outcome: .finished(result: .pending)
                    )
                }
                throw MobileChargingError.Network
            },
            sleep: { _ in },
            enabled: { true }
        )

        let firstResult = await sync.run()
        XCTAssertEqual(firstResult, .pending)
        XCTAssertEqual(sync.pendingCount, 6)
        let secondResult = await sync.run()
        XCTAssertEqual(secondResult, .offline)
        XCTAssertEqual(sync.pendingCount, 6)
    }

    func testConflictOutcomePublishesProgressFromTheTerminalRound() async {
        let store = MemoryChargingCredentialStore(
            initial: .ready(ChargingCommunityCredential(identityID: "identity", token: "secret"))
        )
        let cachedDate = Date(timeIntervalSince1970: 1_700_000_456)
        var dateReads = 0
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                XCTFail("Existing identity must be reused")
                return .init(identityID: "", token: "")
            },
            synchronize: { _ in
                ChargingCommunitySyncCounts(
                    pendingCount: 2,
                    downloaded: 4,
                    uploaded: 3,
                    outcome: .finished(result: .conflict)
                )
            },
            readLastUpdated: {
                dateReads += 1
                return UInt64(cachedDate.timeIntervalSince1970 * 1_000)
            },
            sleep: { _ in },
            enabled: { true }
        )

        let result = await sync.run()

        XCTAssertEqual(result, .conflict)
        XCTAssertEqual(sync.pendingCount, 2)
        XCTAssertEqual(sync.downloaded, 4)
        XCTAssertEqual(sync.uploaded, 3)
        XCTAssertEqual(dateReads, 2)
        XCTAssertEqual(sync.lastUpdated, cachedDate)
    }

    func testNoIdentityStaysUnregisteredAndCredentialRemainsUntilConfirmation() async throws {
        let store = MemoryDeletionCredentialStore()
        var deletionCalls = 0
        let deletion = ChargingCommunityAccountDeletion(
            credentialStore: store,
            sync: makeDeletionTestSync(store: store),
            deleteAccount: { _ in
                deletionCalls += 1
                return 1
            }
        )

        await deletion.refresh()
        XCTAssertEqual(deletion.state, .noAccount)
        let emptyRecord = try await store.load()
        XCTAssertNil(emptyRecord)
        await deletion.deleteAccount()
        XCTAssertEqual(deletionCalls, 0)
        let stillEmptyRecord = try await store.load()
        XCTAssertNil(stillEmptyRecord)

        let credential = ChargingCommunityCredential(identityID: "account", token: "secret")
        let activeStore = MemoryDeletionCredentialStore(initial: .ready(credential))
        let cancelledFlow = ChargingCommunityAccountDeletion(
            credentialStore: activeStore,
            sync: makeDeletionTestSync(store: activeStore),
            deleteAccount: { _ in
                XCTFail("Confirmation was cancelled, so deletion must not be submitted")
                return 1
            }
        )
        await cancelledFlow.refresh()
        XCTAssertEqual(cancelledFlow.state, .active)
        let activeRecord = try await activeStore.load()
        XCTAssertEqual(activeRecord, .ready(credential))
    }

    func testConfirmedDeletionKeepsCredentialUntilSuccessThenStoresTombstone() async throws {
        let credential = ChargingCommunityCredential(identityID: "account", token: "secret")
        let store = MemoryDeletionCredentialStore(initial: .ready(credential))
        var submittedTokens = [String]()
        let deletion = ChargingCommunityAccountDeletion(
            credentialStore: store,
            sync: makeDeletionTestSync(store: store),
            deleteAccount: { token in
                let recordDuringRequest = try await store.load()
                XCTAssertEqual(recordDuringRequest, .deletionPending(credential))
                submittedTokens.append(token)
                return 42
            }
        )

        await deletion.refresh()
        await deletion.deleteAccount()

        XCTAssertEqual(submittedTokens, ["secret"])
        XCTAssertEqual(deletion.state, .deleted)
        XCTAssertEqual(deletion.completedAtMilliseconds, 42)
        let recordAfterSuccess = try await store.load()
        XCTAssertEqual(recordAfterSuccess, .deleted)
    }

    func testAmbiguousFailureSurvivesRelaunchAndRetriesWithTheOriginalCredential() async throws {
        let credential = ChargingCommunityCredential(identityID: "account", token: "secret")
        let store = MemoryDeletionCredentialStore(initial: .ready(credential))
        var firstAttempt = true
        let firstLaunch = ChargingCommunityAccountDeletion(
            credentialStore: store,
            sync: makeDeletionTestSync(store: store),
            deleteAccount: { _ in
                if firstAttempt {
                    firstAttempt = false
                    throw MobileChargingError.Network
                }
                return 84
            }
        )

        await firstLaunch.refresh()
        await firstLaunch.deleteAccount()
        XCTAssertEqual(firstLaunch.state, .deletionPending)
        let recordAfterAmbiguousFailure = try await store.load()
        XCTAssertEqual(recordAfterAmbiguousFailure, .deletionPending(credential))

        let relaunched = ChargingCommunityAccountDeletion(
            credentialStore: store,
            sync: makeDeletionTestSync(store: store),
            deleteAccount: { token in
                XCTAssertEqual(token, credential.token)
                return 85
            }
        )
        await relaunched.refresh()
        XCTAssertEqual(relaunched.state, .deletionPending)
        await relaunched.deleteAccount()
        XCTAssertEqual(relaunched.state, .deleted)
        let recordAfterRetry = try await store.load()
        XCTAssertEqual(recordAfterRetry, .deleted)
    }

    func testKeychainCleanupFailureRemainsRetryable() async throws {
        let credential = ChargingCommunityCredential(identityID: "account", token: "secret")
        let store = MemoryDeletionCredentialStore(initial: .ready(credential), failFinishDeletionCount: 1)
        var deletionCalls = 0
        let deletion = ChargingCommunityAccountDeletion(
            credentialStore: store,
            sync: makeDeletionTestSync(store: store),
            deleteAccount: { _ in
                deletionCalls += 1
                return UInt64(deletionCalls)
            }
        )

        await deletion.refresh()
        await deletion.deleteAccount()
        XCTAssertEqual(deletion.state, .deletionPending)
        let recordAfterCleanupFailure = try await store.load()
        XCTAssertEqual(recordAfterCleanupFailure, .deletionPending(credential))
        await deletion.deleteAccount()
        XCTAssertEqual(deletion.state, .deleted)
        XCTAssertEqual(deletionCalls, 2)
        let recordAfterCleanupRetry = try await store.load()
        XCTAssertEqual(recordAfterCleanupRetry, .deleted)
    }

    func testConfirmedDeletionClearsObsoleteSyncCounts() async throws {
        let credential = ChargingCommunityCredential(identityID: "account", token: "secret")
        let store = MemoryDeletionCredentialStore(initial: .ready(credential))
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                XCTFail("Deletion must not register an account")
                throw MobileChargingError.Unauthorized
            },
            synchronize: { _ in
                ChargingCommunitySyncCounts(
                    pendingCount: 2, downloaded: 3, uploaded: 1,
                    outcome: .finished(result: .pending)
                )
            },
            sleep: { _ in }, enabled: { true }
        )
        _ = await sync.run()
        XCTAssertEqual(sync.pendingCount, 2)
        let deletion = ChargingCommunityAccountDeletion(
            credentialStore: store, sync: sync, deleteAccount: { _ in 42 }
        )
        await deletion.refresh()
        await deletion.deleteAccount()
        XCTAssertEqual(deletion.state, .deleted)
        XCTAssertEqual(sync.status, .deleted)
        XCTAssertEqual(sync.pendingCount, 0)
        XCTAssertEqual(sync.downloaded, 0)
        XCTAssertEqual(sync.uploaded, 0)
    }

    func testPendingDeletionFailureResumesOnlyPublicRefreshInTheSameProcess() async throws {
        for failsCleanup in [false, true] {
            let credential = ChargingCommunityCredential(identityID: "account", token: "secret")
            let store = MemoryDeletionCredentialStore(
                initial: .ready(credential), failFinishDeletionCount: failsCleanup ? 1 : 0
            )
            var publicRefreshes = 0
            let sync = ChargingCommunitySync(
                credentialStore: store,
                registerIdentity: {
                    XCTFail("Pending deletion must not register a replacement account")
                    throw MobileChargingError.Unauthorized
                },
                synchronize: { _ in
                    XCTFail("Pending deletion must not synchronize with the old credential")
                    throw MobileChargingError.Unauthorized
                },
                refreshDirectory: {
                    publicRefreshes += 1
                    return ChargingCommunitySyncCounts(
                        pendingCount: 0, downloaded: 0, uploaded: 0,
                        outcome: .finished(result: .synced)
                    )
                },
                sleep: { _ in }, enabled: { true }
            )
            let deletion = ChargingCommunityAccountDeletion(
                credentialStore: store, sync: sync,
                deleteAccount: { _ in
                    if !failsCleanup { throw MobileChargingError.Network }
                    return 42
                }
            )
            await deletion.refresh()
            await deletion.deleteAccount()
            XCTAssertEqual(deletion.state, .deletionPending)
            XCTAssertEqual(sync.status, .deletionPending)
            let status = await sync.run(explicitRetry: true)
            XCTAssertEqual(status, .deletionPending)
            XCTAssertEqual(publicRefreshes, 1)
        }
    }

    func testDeletedTombstonePreventsRegistrationAndOldIdentitySync() async throws {
        let store = MemoryDeletionCredentialStore(initial: .deleted)
        var registrations = 0
        var syncCalls = 0
        let sync = ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                registrations += 1
                return ChargingCommunityCredential(identityID: "new", token: "new-token")
            },
            synchronize: { _ in
                syncCalls += 1
                return ChargingCommunitySyncCounts(
                    pendingCount: 0, downloaded: 0, uploaded: 0,
                    outcome: .finished(result: .synced)
                )
            },
            sleep: { _ in },
            enabled: { true }
        )

        let result = await sync.run()
        XCTAssertEqual(result, .deleted)
        XCTAssertEqual(registrations, 0)
        XCTAssertEqual(syncCalls, 0)
    }

    private func makeDeletionTestSync(store: any ChargingCredentialStore) -> ChargingCommunitySync {
        ChargingCommunitySync(
            credentialStore: store,
            registerIdentity: {
                XCTFail("Deletion must not register an account")
                return .init(identityID: "", token: "")
            },
            synchronize: { _ in
                ChargingCommunitySyncCounts(
                    pendingCount: 0, downloaded: 0, uploaded: 0,
                    outcome: .finished(result: .synced)
                )
            },
            sleep: { _ in },
            enabled: { true }
        )
    }
}

private actor MemoryDeletionCredentialStore: ChargingCredentialStore {
    private var record: ChargingCredentialRecord?
    private var finishDeletionFailures: Int

    init(initial: ChargingCredentialRecord? = nil, failFinishDeletionCount: Int = 0) {
        record = initial
        finishDeletionFailures = failFinishDeletionCount
    }

    func load() async throws -> ChargingCredentialRecord? { record }

    func beginRegistration() async throws -> Bool {
        guard record == nil else { return false }
        record = .registrationStarted
        return true
    }

    func save(_ credential: ChargingCommunityCredential) async throws {
        guard record == .registrationStarted else { throw ChargingCredentialStoreError.unavailable }
        record = .ready(credential)
    }

    func markDeletionPending(_ credential: ChargingCommunityCredential) async throws {
        guard record == .ready(credential) || record == .deletionPending(credential) else {
            throw ChargingCredentialStoreError.unavailable
        }
        record = .deletionPending(credential)
    }

    func finishDeletion(token: String) async throws {
        guard case .deletionPending(let credential) = record, credential.token == token else {
            throw ChargingCredentialStoreError.unavailable
        }
        if finishDeletionFailures > 0 {
            finishDeletionFailures -= 1
            throw ChargingCredentialStoreError.unavailable
        }
        record = .deleted
    }
}

private actor MemoryChargingCredentialStore: ChargingCredentialStore {
    private var record: ChargingCredentialRecord?
    private let loadFailure: ChargingCredentialStoreError?
    private let loseRegistrationClaim: Bool
    private let recordAfterLostClaim: ChargingCredentialRecord?

    init(
        initial: ChargingCredentialRecord? = nil,
        failLoad: ChargingCredentialStoreError? = nil,
        loseRegistrationClaim: Bool = false,
        recordAfterLostClaim: ChargingCredentialRecord? = nil
    ) {
        record = initial
        loadFailure = failLoad
        self.loseRegistrationClaim = loseRegistrationClaim
        self.recordAfterLostClaim = recordAfterLostClaim
    }

    func load() async throws -> ChargingCredentialRecord? {
        if let loadFailure { throw loadFailure }
        return record
    }

    func beginRegistration() async throws -> Bool {
        if loseRegistrationClaim {
            record = recordAfterLostClaim
            return false
        }
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

    func markDeletionPending(_ credential: ChargingCommunityCredential) async throws {
        guard record == .ready(credential) || record == .deletionPending(credential) else {
            throw ChargingCredentialStoreError.unavailable
        }
        record = .deletionPending(credential)
    }

    func finishDeletion(token: String) async throws {
        guard case .deletionPending(let credential) = record, credential.token == token else {
            throw ChargingCredentialStoreError.unavailable
        }
        record = .deleted
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
