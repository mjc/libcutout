import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation
import Security

enum ChargingCommunitySyncDebugEnablement {
    static func isSyncEnabled(
        environmentFixtureValue: String?,
        persistedFixtureValue: String?,
        arguments: [String],
        chargingFixtureEnabled: Bool
    ) -> Bool {
        guard environmentFixtureValue == nil,
            persistedFixtureValue == nil,
            !chargingFixtureEnabled
        else { return false }
        return !arguments.contains(where: isFixtureArgument)
    }

    private static func isFixtureArgument(_ argument: String) -> Bool {
        argument.hasPrefix("-CUTOUT_UI_TEST_")
            || argument.hasPrefix("--ui-test-")
            || argument.hasPrefix("--seed-ui-test-")
            || argument.hasPrefix("--fail-ui-test-")
    }
}

#if DEBUG
    private struct ChargingCommunityAccountDeletionUITestFixture {
        let baseURL: URL
        let credential: ChargingCommunityCredential
        let keychainID: String

        static var isRequested: Bool {
            CommandLine.arguments.contains("--ui-test-account-deletion")
        }

        static var supportsFixture: Bool {
            #if targetEnvironment(simulator)
                true
            #else
                false
            #endif
        }

        static var current: Self? {
            guard isRequested, supportsFixture else { return nil }
            let environment = ProcessInfo.processInfo.environment
            guard
                let baseURLString = environment["CUTOUT_UI_TEST_ACCOUNT_BASE_URL"],
                let baseURL = URL(string: baseURLString),
                baseURL.scheme == "http",
                ["127.0.0.1", "::1"].contains(baseURL.host?.lowercased() ?? ""),
                baseURL.port != nil,
                let identityID = environment["CUTOUT_UI_TEST_ACCOUNT_ID"], !identityID.isEmpty,
                let token = environment["CUTOUT_UI_TEST_ACCOUNT_TOKEN"], !token.isEmpty,
                let keychainID = environment["CUTOUT_UI_TEST_ACCOUNT_KEYCHAIN_ID"], !keychainID.isEmpty
            else { return nil }
            return Self(
                baseURL: baseURL,
                credential: ChargingCommunityCredential(identityID: identityID, token: token),
                keychainID: keychainID
            )
        }
    }
#endif

struct ChargingCommunityCredential: Codable, Equatable, Sendable {
    let identityID: String
    let token: String
}

enum ChargingCredentialRecord: Equatable, Sendable {
    case registrationStarted
    case ready(ChargingCommunityCredential)
    case deletionPending(ChargingCommunityCredential)
    case deleted
}

protocol ChargingCredentialStore: Sendable {
    func load() async throws -> ChargingCredentialRecord?
    func beginRegistration() async throws -> Bool
    func save(_ credential: ChargingCommunityCredential) async throws
    func markDeletionPending(_ credential: ChargingCommunityCredential) async throws
    func finishDeletion(token: String) async throws
}

enum ChargingCredentialStoreError: Error, Equatable {
    case locked
    case unavailable
}

struct KeychainChargingCredentialStore: ChargingCredentialStore {
    static let productionAccount = "charging-community-identity-v1"
    private let keychainAccount: String

    init(account: String = Self.productionAccount) {
        keychainAccount = account
    }

    func load() async throws -> ChargingCredentialRecord? {
        let account = keychainAccount
        return try await Task.detached(priority: .userInitiated) {
            try Self.loadSynchronously(account: account)
        }.value
    }

    func beginRegistration() async throws -> Bool {
        let account = keychainAccount
        return try await Task.detached(priority: .userInitiated) {
            try Self.beginRegistrationSynchronously(account: account)
        }.value
    }

    func save(_ credential: ChargingCommunityCredential) async throws {
        let account = keychainAccount
        try await Task.detached(priority: .userInitiated) {
            try Self.saveSynchronously(credential, account: account)
        }.value
    }

    func markDeletionPending(_ credential: ChargingCommunityCredential) async throws {
        let account = keychainAccount
        try await Task.detached(priority: .userInitiated) {
            try Self.markDeletionPendingSynchronously(credential, account: account)
        }.value
    }

    func finishDeletion(token: String) async throws {
        let account = keychainAccount
        try await Task.detached(priority: .userInitiated) {
            try Self.finishDeletionSynchronously(token: token, account: account)
        }.value
    }

    #if DEBUG
        func seedIfMissing(_ credential: ChargingCommunityCredential) async throws -> Bool {
            let account = keychainAccount
            return try await Task.detached(priority: .userInitiated) {
                try Self.seedSynchronously(credential, account: account)
            }.value
        }
    #endif

    private static func query(account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Bundle.main.bundleIdentifier ?? "io.cutout.mobile",
            kSecAttrAccount as String: account,
        ]
    }

    private static func loadSynchronously(account: String) throws -> ChargingCredentialRecord? {
        var request = query(account: account)
        request[kSecReturnData as String] = true
        request[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(request as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data else {
            throw storeError(status)
        }
        do {
            let stored = try JSONDecoder().decode(StoredCredential.self, from: data)
            switch stored.phase {
            case .registrationStarted:
                return .registrationStarted
            case .ready:
                guard let identityID = stored.identityID, let token = stored.token else {
                    throw ChargingCredentialStoreError.unavailable
                }
                return .ready(ChargingCommunityCredential(identityID: identityID, token: token))
            case .deletionPending:
                guard let identityID = stored.identityID, let token = stored.token else {
                    throw ChargingCredentialStoreError.unavailable
                }
                return .deletionPending(ChargingCommunityCredential(identityID: identityID, token: token))
            case .deleted:
                return .deleted
            }
        } catch let error as ChargingCredentialStoreError {
            throw error
        } catch {
            throw ChargingCredentialStoreError.unavailable
        }
    }

    private static func beginRegistrationSynchronously(account: String) throws -> Bool {
        let data = try JSONEncoder().encode(StoredCredential(phase: .registrationStarted))
        var request = query(account: account)
        request[kSecValueData as String] = data
        request[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(request as CFDictionary, nil)
        if status == errSecSuccess { return true }
        if status == errSecDuplicateItem { return false }
        throw storeError(status)
    }

    private static func saveSynchronously(_ credential: ChargingCommunityCredential, account: String) throws {
        guard try loadSynchronously(account: account) == .registrationStarted else {
            throw ChargingCredentialStoreError.unavailable
        }
        let data = try JSONEncoder().encode(
            StoredCredential(
                phase: .ready, identityID: credential.identityID, token: credential.token
            )
        )
        let status = SecItemUpdate(
            query(account: account) as CFDictionary,
            [kSecValueData as String: data] as CFDictionary
        )
        guard status == errSecSuccess else { throw storeError(status) }
    }

    private static func markDeletionPendingSynchronously(
        _ credential: ChargingCommunityCredential, account: String
    ) throws {
        let current = try loadSynchronously(account: account)
        guard current == .ready(credential) || current == .deletionPending(credential) else {
            throw ChargingCredentialStoreError.unavailable
        }
        let data = try JSONEncoder().encode(
            StoredCredential(phase: .deletionPending, identityID: credential.identityID, token: credential.token)
        )
        let status = SecItemUpdate(
            query(account: account) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        guard status == errSecSuccess else { throw storeError(status) }
    }

    private static func finishDeletionSynchronously(token: String, account: String) throws {
        guard case .deletionPending(let credential) = try loadSynchronously(account: account), credential.token == token
        else {
            throw ChargingCredentialStoreError.unavailable
        }
        // Preserve the item as a tombstone so account admission cannot recreate an identity.
        let data = try JSONEncoder().encode(StoredCredential(phase: .deleted))
        let status = SecItemUpdate(
            query(account: account) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        guard status == errSecSuccess else { throw storeError(status) }
    }

    #if DEBUG
        private static func seedSynchronously(
            _ credential: ChargingCommunityCredential, account: String
        ) throws -> Bool {
            let data = try JSONEncoder().encode(
                StoredCredential(
                    phase: .ready, identityID: credential.identityID, token: credential.token
                )
            )
            var request = query(account: account)
            request[kSecValueData as String] = data
            request[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            let status = SecItemAdd(request as CFDictionary, nil)
            if status == errSecSuccess { return true }
            if status == errSecDuplicateItem { return false }
            throw storeError(status)
        }
    #endif

    private static func storeError(_ status: OSStatus) -> ChargingCredentialStoreError {
        status == errSecInteractionNotAllowed || status == errSecAuthFailed ? .locked : .unavailable
    }

    private struct StoredCredential: Codable {
        enum Phase: String, Codable {
            case registrationStarted
            case ready
            case deletionPending
            case deleted
        }

        let phase: Phase
        var identityID: String?
        var token: String?

        init(phase: Phase, identityID: String? = nil, token: String? = nil) {
            self.phase = phase
            self.identityID = identityID
            self.token = token
        }
    }
}

typealias ChargingCommunitySyncCounts = MobileChargingSyncSummaryDto

enum ChargingCommunitySyncStatus: Equatable, Sendable {
    case notStarted
    case syncing
    case synced
    case pending
    case offline
    case authenticationRequired
    case conflict
    case rejected
    case throttled
    case storageUnavailable
    case registrationUncertain
    case deletionPending
    case deleted
}

@MainActor
@Observable
final class ChargingCommunitySync {
    typealias RegisterIdentity = @MainActor () async throws -> ChargingCommunityCredential
    typealias Synchronize = @MainActor (String) async throws -> ChargingCommunitySyncCounts
    typealias RefreshDirectory = @MainActor () async throws -> ChargingCommunitySyncCounts
    typealias ReadLastUpdated = @MainActor () async throws -> UInt64?
    typealias Sleep = @MainActor (Duration) async throws -> Void

    static let shared = makeShared()

    private static func makeShared() -> ChargingCommunitySync {
        ChargingCommunitySync(
            credentialStore: defaultCredentialStore(),
            registerIdentity: {
                let identity = try await mobileRegisterChargingIdentity()
                return ChargingCommunityCredential(
                    identityID: identity.identityId, token: identity.token
                )
            },
            synchronize: { token in
                let database = try await RustPersistenceStore.open()
                return try await database.synchronizeChargingSpots(token: token)
            },
            refreshDirectory: {
                let database = try await RustPersistenceStore.open()
                return try await database.refreshChargingDirectory()
            },
            readLastUpdated: {
                let database = try await RustPersistenceStore.open()
                return try await database.chargingDirectoryLastUpdatedMs()
            },
            sleep: waitForRetry,
            enabled: {
                #if DEBUG
                    ChargingCommunitySyncDebugEnablement.isSyncEnabled(
                        environmentFixtureValue: ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_FIXTURE"],
                        persistedFixtureValue: UserDefaults.standard.string(forKey: "CUTOUT_UI_TEST_FIXTURE"),
                        arguments: CommandLine.arguments,
                        chargingFixtureEnabled: CutoutUITestChargingFixture.isEnabled
                    )
                #else
                    true
                #endif
            }
        )
    }

    private static func defaultCredentialStore() -> KeychainChargingCredentialStore {
        #if DEBUG
            if ChargingCommunityAccountDeletionUITestFixture.isRequested {
                return KeychainChargingCredentialStore(
                    account: ChargingCommunityAccountDeletionUITestFixture.current?.keychainID
                        ?? "charging-community-ui-test-unconfigured"
                )
            }
        #endif
        return KeychainChargingCredentialStore()
    }

    private static func waitForRetry(_ duration: Duration) async throws {
        try await Task.sleep(for: duration)
    }

    private let credentialStore: any ChargingCredentialStore
    private let registerIdentity: RegisterIdentity
    private let synchronize: Synchronize
    private let refreshDirectory: RefreshDirectory?
    private let readLastUpdated: ReadLastUpdated?
    private let sleep: Sleep
    private let enabled: @MainActor () -> Bool
    private var activeTask: Task<ChargingCommunitySyncStatus, Never>?
    private var accountDeletionInProgress = false
    private var runGeneration: UInt64 = 0
    private var syncRunState = MobileChargingSyncRunState(
        round: 0, attempt: 0, pendingCount: 0, downloaded: 0, uploaded: 0
    )

    private(set) var status: ChargingCommunitySyncStatus = .notStarted
    var pendingCount: UInt32 { syncRunState.pendingCount }
    var downloaded: UInt32 { syncRunState.downloaded }
    var uploaded: UInt32 { syncRunState.uploaded }
    private(set) var lastUpdated: Date?

    init(
        credentialStore: any ChargingCredentialStore,
        registerIdentity: @escaping RegisterIdentity,
        synchronize: @escaping Synchronize,
        refreshDirectory: RefreshDirectory? = nil,
        readLastUpdated: ReadLastUpdated? = nil,
        sleep: @escaping Sleep,
        enabled: @escaping @MainActor () -> Bool
    ) {
        self.credentialStore = credentialStore
        self.registerIdentity = registerIdentity
        self.synchronize = synchronize
        self.refreshDirectory = refreshDirectory
        self.readLastUpdated = readLastUpdated
        self.sleep = sleep
        self.enabled = enabled
    }

    @discardableResult
    func run(explicitRetry: Bool = false) async -> ChargingCommunitySyncStatus {
        guard enabled(), !accountDeletionInProgress else { return status }
        if let activeTask { return await activeTask.value }

        runGeneration &+= 1
        let generation = runGeneration
        let task = Task { await self.perform(explicitRetry: explicitRetry) }
        activeTask = task
        let result = await task.value
        guard generation == runGeneration else { return result }
        activeTask = nil
        status = result
        return result
    }

    func cancel() {
        activeTask?.cancel()
        activeTask = nil
        runGeneration &+= 1
    }

    func cancelAndWait() async {
        let task = activeTask
        cancel()
        if let task { _ = await task.value }
    }

    func pauseForAccountDeletion() async {
        accountDeletionInProgress = true
        await cancelAndWait()
    }

    func resumeAfterUnmarkedDeletionFailure() {
        accountDeletionInProgress = false
    }

    func allowPublicRefreshAfterDeletion() {
        accountDeletionInProgress = false
        syncRunState = MobileChargingSyncRunState(
            round: 0, attempt: 0, pendingCount: 0, downloaded: 0, uploaded: 0
        )
        status = .deleted
    }

    func allowPublicRefreshWhileDeletionPending() {
        accountDeletionInProgress = false
        status = .deletionPending
    }

    private func perform(explicitRetry: Bool) async -> ChargingCommunitySyncStatus {
        status = .syncing
        syncRunState = MobileChargingSyncRunState(
            round: 0, attempt: 0, pendingCount: pendingCount, downloaded: 0, uploaded: 0
        )
        await refreshPersistedLastUpdated()
        guard !Task.isCancelled else { return .notStarted }
        let credential: ChargingCommunityCredential
        do {
            let stored = try await credentialStore.load()
            try Task.checkCancellation()
            let admission = mobileChargingRegistrationAdmission(
                state: credentialState(stored), explicitRetry: explicitRetry
            )
            switch admission {
            case .synchronize:
                guard case .ready(let saved) = stored else { return .storageUnavailable }
                credential = saved
            case .registerIdentity:
                credential = try await registerAndPersist()
            case .beginRegistration:
                let beganRegistration = try await credentialStore.beginRegistration()
                try Task.checkCancellation()
                if beganRegistration {
                    credential = try await registerAndPersist()
                } else {
                    let currentRecord = try await credentialStore.load()
                    guard case .ready(let saved) = currentRecord else {
                        await refreshUnauthenticatedDirectory()
                        return .registrationUncertain
                    }
                    credential = saved
                }
            case .publicRefresh:
                await refreshUnauthenticatedDirectory()
                if case .deleted = stored { return .deleted }
                if case .deletionPending = stored { return .deletionPending }
                return .storageUnavailable
            case .requireExplicitRetry:
                await refreshUnauthenticatedDirectory()
                return .registrationUncertain
            }
        } catch {
            await refreshUnauthenticatedDirectory()
            return status(for: error)
        }

        while !Task.isCancelled {
            let step: MobileChargingSyncStep
            do {
                step = try await synchronizeWithPolicy(token: credential.token)
            } catch {
                return status(for: error)
            }
            syncRunState = step.state
            if step.action == .nextRound { continue }
            if case .finished(result: .authenticationRequired) = step.action {
                await refreshUnauthenticatedDirectory()
            }
            if case .finished(result: .synced) = step.action {
                lastUpdated = Date()
            }
            if step.action == .finished(result: .synced) || step.state.downloaded > 0 {
                await refreshPersistedLastUpdated()
            }
            return status(for: step.action)
        }
        return .notStarted
    }

    private func refreshUnauthenticatedDirectory() async {
        guard let refreshDirectory else { return }
        while !Task.isCancelled {
            do {
                let summary = try await refreshDirectory()
                try Task.checkCancellation()
                let step = mobileChargingSyncObserveSummary(state: syncRunState, summary: summary)
                syncRunState = step.state
                if summary.downloaded > 0 || step.action != .nextRound {
                    await refreshPersistedLastUpdated()
                }
                if case let .retry(afterSeconds: delay) = step.action {
                    do { try await sleep(.seconds(Int64(delay))) } catch { return }
                    continue
                }
                guard step.action == .nextRound else { return }
            } catch {
                guard !(error is CancellationError) else { return }
                let failure = (error as? MobileChargingError) ?? .StorageFailure
                let step = mobileChargingSyncObserveError(state: syncRunState, error: failure)
                syncRunState = step.state
                guard case let .retry(afterSeconds: delay) = step.action else { return }
                do { try await sleep(.seconds(Int64(delay))) } catch { return }
            }
        }
    }

    private func refreshPersistedLastUpdated() async {
        guard let readLastUpdated else { return }
        do {
            let timestamp = try await readLastUpdated()
            try Task.checkCancellation()
            guard let timestamp else { return }
            lastUpdated = Date(timeIntervalSince1970: Double(timestamp) / 1_000)
        } catch {
            // Keep the last known cache date when the local database cannot be read.
        }
    }

    private func registerAndPersist() async throws -> ChargingCommunityCredential {
        let credential = try await registerIdentity()
        try await credentialStore.save(credential)
        try Task.checkCancellation()
        return credential
    }

    private func synchronizeWithPolicy(token: String) async throws -> MobileChargingSyncStep {
        var state = syncRunState
        while true {
            try Task.checkCancellation()
            do {
                let summary = try await synchronize(token)
                try Task.checkCancellation()
                let step = mobileChargingSyncObserveSummary(state: state, summary: summary)
                state = step.state
                if case let .retry(afterSeconds: delay) = step.action {
                    try await sleep(.seconds(Int64(delay)))
                    continue
                }
                return step
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                let failure = (error as? MobileChargingError) ?? .StorageFailure
                let step = mobileChargingSyncObserveError(state: state, error: failure)
                state = step.state
                guard case let .retry(afterSeconds: delay) = step.action else { return step }
                try await sleep(.seconds(Int64(delay)))
            }
        }
    }

    private func credentialState(
        _ record: ChargingCredentialRecord?
    ) -> MobileChargingSyncCredentialState {
        switch record {
        case .ready: .ready
        case .registrationStarted: .registrationStarted
        case .deletionPending: .deletionPending
        case .deleted: .deleted
        case nil: .missing
        }
    }

    private func status(for action: MobileChargingSyncAction) -> ChargingCommunitySyncStatus {
        switch action {
        case .finished(let result): status(for: result)
        case .nextRound, .retry(_): .pending
        }
    }

    private func status(for result: MobileChargingSyncResult) -> ChargingCommunitySyncStatus {
        switch result {
        case .synced: .synced
        case .pending: .pending
        case .offline: .offline
        case .authenticationRequired: .authenticationRequired
        case .conflict: .conflict
        case .rejected: .rejected
        case .throttled: .throttled
        case .storageUnavailable: .storageUnavailable
        }
    }

    private func status(for error: Error) -> ChargingCommunitySyncStatus {
        if error is CancellationError { return .notStarted }
        if error is ChargingCredentialStoreError {
            return .storageUnavailable
        }
        guard let error = error as? MobileChargingError else { return .storageUnavailable }
        return status(for: mobileChargingSyncResult(error: error))
    }
}

enum ChargingCommunityAccountDeletionState: Equatable, Sendable {
    case loading
    case noAccount
    case active
    case registrationUncertain
    case deleting
    case deletionPending
    case deleted
    case failed
}

@MainActor
@Observable
final class ChargingCommunityAccountDeletion {
    typealias DeleteAccount = @MainActor (String) async throws -> UInt64
    typealias SeedCredentialIfMissing = @MainActor (ChargingCommunityCredential) async throws -> Bool

    static let shared = makeShared()

    private static func makeShared() -> ChargingCommunityAccountDeletion {
        #if DEBUG
            if ChargingCommunityAccountDeletionUITestFixture.isRequested {
                guard let fixture = ChargingCommunityAccountDeletionUITestFixture.current else {
                    return ChargingCommunityAccountDeletion(
                        credentialStore: KeychainChargingCredentialStore(
                            account: "charging-community-ui-test-unconfigured"
                        ),
                        sync: .shared,
                        deleteAccount: { _ in throw ChargingCredentialStoreError.unavailable }
                    )
                }
                let store = KeychainChargingCredentialStore(account: fixture.keychainID)
                return ChargingCommunityAccountDeletion(
                    credentialStore: store,
                    sync: .shared,
                    deleteAccount: { token in
                        guard token == fixture.credential.token else {
                            throw MobileChargingError.Unauthorized
                        }
                        let database = try await RustPersistenceStore.open()
                        let receipt = try await database.deleteCommunityAccountAtLoopback(
                            token: token, baseUrl: fixture.baseURL.absoluteString
                        )
                        return receipt.completedAtMilliseconds
                    },
                    seedCredential: fixture.credential,
                    seedCredentialIfMissing: { credential in
                        try await store.seedIfMissing(credential)
                    }
                )
            }
        #endif

        let store = KeychainChargingCredentialStore()
        return ChargingCommunityAccountDeletion(
            credentialStore: store,
            sync: .shared,
            deleteAccount: { token in
                let database = try await RustPersistenceStore.open()
                let receipt = try await database.deleteCommunityAccount(token: token)
                return receipt.completedAtMilliseconds
            }
        )
    }

    private let credentialStore: any ChargingCredentialStore
    private let sync: ChargingCommunitySync
    private let submitDeletion: DeleteAccount
    private let seedCredential: ChargingCommunityCredential?
    private let seedCredentialIfMissing: SeedCredentialIfMissing?
    private var generation: UInt64 = 0
    private(set) var state: ChargingCommunityAccountDeletionState = .loading
    private(set) var lastError: String?
    private(set) var completedAtMilliseconds: UInt64?

    init(
        credentialStore: any ChargingCredentialStore,
        sync: ChargingCommunitySync,
        deleteAccount: @escaping DeleteAccount,
        seedCredential: ChargingCommunityCredential? = nil,
        seedCredentialIfMissing: SeedCredentialIfMissing? = nil
    ) {
        self.credentialStore = credentialStore
        self.sync = sync
        self.submitDeletion = deleteAccount
        self.seedCredential = seedCredential
        self.seedCredentialIfMissing = seedCredentialIfMissing
    }

    func refresh() async {
        guard state != .deleting else { return }
        generation &+= 1
        let currentGeneration = generation
        state = .loading
        lastError = nil
        do {
            var record = try await credentialStore.load()
            if record == nil, let seedCredential, let seedCredentialIfMissing {
                _ = try await seedCredentialIfMissing(seedCredential)
                record = try await credentialStore.load()
            }
            guard currentGeneration == generation else { return }
            switch record {
            case .ready: state = .active
            case .deletionPending: state = .deletionPending
            case .deleted: state = .deleted
            case .registrationStarted: state = .registrationUncertain
            case nil: state = .noAccount
            }
        } catch {
            guard currentGeneration == generation else { return }
            state = .failed
            lastError = error.localizedDescription
        }
    }

    func deleteAccount() async {
        guard state == .active || state == .deletionPending else { return }
        var deletionWasMarked = state == .deletionPending
        generation &+= 1
        let currentGeneration = generation
        state = .deleting
        lastError = nil
        do {
            let record = try await credentialStore.load()
            let credential: ChargingCommunityCredential
            switch record {
            case .ready(let saved), .deletionPending(let saved):
                credential = saved
            case .deleted:
                state = .deleted
                return
            case .registrationStarted, nil:
                state = .noAccount
                return
            }

            await sync.pauseForAccountDeletion()
            guard currentGeneration == generation else { return }
            if record == .ready(credential) {
                try await credentialStore.markDeletionPending(credential)
            }
            deletionWasMarked = true
            guard currentGeneration == generation else { return }
            let completedAt = try await submitDeletion(credential.token)
            guard currentGeneration == generation else { return }
            completedAtMilliseconds = completedAt
            try await credentialStore.finishDeletion(token: credential.token)
            guard currentGeneration == generation else { return }
            state = .deleted
            sync.allowPublicRefreshAfterDeletion()
        } catch {
            guard currentGeneration == generation else { return }
            if deletionWasMarked {
                state = .deletionPending
                sync.allowPublicRefreshWhileDeletionPending()
            } else {
                state = .active
                sync.resumeAfterUnmarkedDeletionFailure()
            }
            lastError = error.localizedDescription
        }
    }
}
