import CutoutMobile
import CutoutMobileFFI
import Foundation
import Observation
import Security

struct ChargingCommunityCredential: Codable, Equatable, Sendable {
    let identityID: String
    let token: String
}

enum ChargingCredentialRecord: Equatable, Sendable {
    case registrationStarted
    case ready(ChargingCommunityCredential)
}

protocol ChargingCredentialStore: Sendable {
    func load() async throws -> ChargingCredentialRecord?
    func beginRegistration() async throws -> Bool
    func save(_ credential: ChargingCommunityCredential) async throws
}

enum ChargingCredentialStoreError: Error, Equatable {
    case locked
    case unavailable
}

struct KeychainChargingCredentialStore: ChargingCredentialStore {
    private static let account = "charging-community-identity-v1"

    func load() async throws -> ChargingCredentialRecord? {
        try await Task.detached(priority: .userInitiated) {
            try Self.loadSynchronously()
        }.value
    }

    func beginRegistration() async throws -> Bool {
        try await Task.detached(priority: .userInitiated) {
            try Self.beginRegistrationSynchronously()
        }.value
    }

    func save(_ credential: ChargingCommunityCredential) async throws {
        try await Task.detached(priority: .userInitiated) {
            try Self.saveSynchronously(credential)
        }.value
    }

    private static var query: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: Bundle.main.bundleIdentifier ?? "io.cutout.mobile",
            kSecAttrAccount as String: account,
        ]
    }

    private static func loadSynchronously() throws -> ChargingCredentialRecord? {
        var request = query
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
            }
        } catch let error as ChargingCredentialStoreError {
            throw error
        } catch {
            throw ChargingCredentialStoreError.unavailable
        }
    }

    private static func beginRegistrationSynchronously() throws -> Bool {
        let data = try JSONEncoder().encode(StoredCredential(phase: .registrationStarted))
        var request = query
        request[kSecValueData as String] = data
        request[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(request as CFDictionary, nil)
        if status == errSecSuccess { return true }
        if status == errSecDuplicateItem { return false }
        throw storeError(status)
    }

    private static func saveSynchronously(_ credential: ChargingCommunityCredential) throws {
        guard try loadSynchronously() == .registrationStarted else {
            throw ChargingCredentialStoreError.unavailable
        }
        let data = try JSONEncoder().encode(
            StoredCredential(
                phase: .ready, identityID: credential.identityID, token: credential.token
            )
        )
        let status = SecItemUpdate(
            query as CFDictionary,
            [kSecValueData as String: data] as CFDictionary
        )
        guard status == errSecSuccess else { throw storeError(status) }
    }

    private static func storeError(_ status: OSStatus) -> ChargingCredentialStoreError {
        status == errSecInteractionNotAllowed || status == errSecAuthFailed ? .locked : .unavailable
    }

    private struct StoredCredential: Codable {
        enum Phase: String, Codable {
            case registrationStarted
            case ready
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

struct ChargingCommunitySyncCounts: Equatable, Sendable {
    let pendingCount: UInt32
    let downloaded: UInt32
    let uploaded: UInt32
    let moreWork: Bool
}

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
}

@MainActor
@Observable
final class ChargingCommunitySync {
    typealias RegisterIdentity = @MainActor () async throws -> ChargingCommunityCredential
    typealias Synchronize = @MainActor (String) async throws -> ChargingCommunitySyncCounts
    typealias RefreshDirectory = @MainActor () async throws -> ChargingCommunitySyncCounts
    typealias ReadLastUpdated = @MainActor () async throws -> UInt64?
    typealias Sleep = @MainActor (Duration) async throws -> Void

    private static let maximumSyncRounds = 3
    private static let maximumAttempts = 3

    static let shared = makeShared()

    private static func makeShared() -> ChargingCommunitySync {
        ChargingCommunitySync(
            credentialStore: KeychainChargingCredentialStore(),
            registerIdentity: {
                let identity = try await mobileRegisterChargingIdentity()
                return ChargingCommunityCredential(
                    identityID: identity.identityId, token: identity.token
                )
            },
            synchronize: { token in
                let database = try await RustPersistenceStore.open()
                let summary = try await database.synchronizeChargingSpots(token: token)
                return ChargingCommunitySyncCounts(
                    pendingCount: summary.pendingCount,
                    downloaded: summary.downloaded,
                    uploaded: summary.uploaded,
                    moreWork: summary.moreWork
                )
            },
            refreshDirectory: {
                let database = try await RustPersistenceStore.open()
                let summary = try await database.refreshChargingDirectory()
                return ChargingCommunitySyncCounts(
                    pendingCount: summary.pendingCount,
                    downloaded: summary.downloaded,
                    uploaded: summary.uploaded,
                    moreWork: summary.moreWork
                )
            },
            readLastUpdated: {
                let database = try await RustPersistenceStore.open()
                return try await database.chargingDirectoryLastUpdatedMs()
            },
            sleep: waitForRetry,
            enabled: {
                #if DEBUG
                    !CutoutUITestChargingFixture.isEnabled
                #else
                    true
                #endif
            }
        )
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
    private var runGeneration: UInt64 = 0

    private(set) var status: ChargingCommunitySyncStatus = .notStarted
    private(set) var pendingCount: UInt32 = 0
    private(set) var downloaded: UInt32 = 0
    private(set) var uploaded: UInt32 = 0
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
        guard enabled() else { return status }
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

    private func perform(explicitRetry: Bool) async -> ChargingCommunitySyncStatus {
        status = .syncing
        downloaded = 0
        uploaded = 0
        await refreshPersistedLastUpdated()
        guard !Task.isCancelled else { return .notStarted }
        let credential: ChargingCommunityCredential
        do {
            let stored = try await credentialStore.load()
            try Task.checkCancellation()
            switch stored {
            case .ready(let saved):
                credential = saved
            case .registrationStarted:
                guard explicitRetry else {
                    await refreshUnauthenticatedDirectory()
                    return .registrationUncertain
                }
                credential = try await registerAndPersist()
            case nil:
                let beganRegistration = try await credentialStore.beginRegistration()
                try Task.checkCancellation()
                if beganRegistration {
                    credential = try await registerAndPersist()
                } else {
                    switch try await credentialStore.load() {
                    case .ready(let saved): credential = saved
                    case .registrationStarted, nil:
                        await refreshUnauthenticatedDirectory()
                        return .registrationUncertain
                    }
                }
            }
        } catch {
            await refreshUnauthenticatedDirectory()
            return status(for: error)
        }

        for round in 0..<Self.maximumSyncRounds {
            let summary: ChargingCommunitySyncCounts
            do {
                try Task.checkCancellation()
                summary = try await synchronizeWithRetry(token: credential.token)
                try Task.checkCancellation()
            } catch {
                if (error as? MobileChargingError) == .Unauthorized {
                    await refreshUnauthenticatedDirectory()
                }
                return status(for: error)
            }
            pendingCount = summary.pendingCount
            downloaded &+= summary.downloaded
            uploaded &+= summary.uploaded
            if !summary.moreWork {
                lastUpdated = Date()
                await refreshPersistedLastUpdated()
                return summary.pendingCount == 0 ? .synced : .pending
            }
            if round == Self.maximumSyncRounds - 1 { return .pending }
        }
        return .pending
    }

    private func refreshUnauthenticatedDirectory() async {
        guard let refreshDirectory, !Task.isCancelled else { return }
        for round in 0..<Self.maximumSyncRounds {
            do {
                let summary = try await refreshDirectory()
                try Task.checkCancellation()
                pendingCount = summary.pendingCount
                downloaded &+= summary.downloaded
                if summary.downloaded > 0 || !summary.moreWork {
                    await refreshPersistedLastUpdated()
                }
                if !summary.moreWork || round == Self.maximumSyncRounds - 1 {
                    return
                }
            } catch {
                return
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

    private func synchronizeWithRetry(token: String) async throws -> ChargingCommunitySyncCounts {
        for attempt in 0..<Self.maximumAttempts {
            do {
                return try await synchronize(token)
            } catch {
                guard attempt < Self.maximumAttempts - 1, isTransient(error) else { throw error }
                try await sleep(.seconds(Int64(1 << attempt)))
            }
        }
        throw MobileChargingError.Network
    }

    private func isTransient(_ error: Error) -> Bool {
        guard let error = error as? MobileChargingError else { return false }
        return switch error {
        case .Network: true
        default: false
        }
    }

    private func status(for error: Error) -> ChargingCommunitySyncStatus {
        if error is CancellationError { return .notStarted }
        if error is ChargingCredentialStoreError {
            return .storageUnavailable
        }
        guard let error = error as? MobileChargingError else { return .storageUnavailable }
        return switch error {
        case .Unauthorized: .authenticationRequired
        case .Conflict: .conflict
        case .Rejected: .rejected
        case .Throttled: .throttled
        case .Network, .Unavailable: .offline
        case .StorageFailure: .storageUnavailable
        default: .storageUnavailable
        }
    }
}
