import CutoutMobile
import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutApp

/// Opt-in integration coverage for the Swift sync coordinator and Rust HTTP transport.
/// Set CUTOUT_UI_TEST_CHARGING_SERVICE_BASE_URL to the disposable loopback proxy origin.
@MainActor
final class ChargingServiceSyncTests: XCTestCase {
    func testIndependentClientsConvergeAcrossOfflineRestartEditsVotesAndWithdrawal() async throws {
        guard
            let rawURL = ProcessInfo.processInfo.environment[
                "CUTOUT_UI_TEST_CHARGING_SERVICE_BASE_URL"
            ], let baseURL = URL(string: rawURL)
        else {
            throw XCTSkip("Set CUTOUT_UI_TEST_CHARGING_SERVICE_BASE_URL to run the local service integration test.")
        }

        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("ChargingServiceSyncTests.\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let pathA = directory.appendingPathComponent("client-a.sqlite")
        let pathB = directory.appendingPathComponent("client-b.sqlite")
        let credentialsA = IntegrationCredentialStore()
        let credentialsB = IntegrationCredentialStore()
        let initialA = try await withClient(path: pathA, credentials: credentialsA, baseURL: baseURL) {
            _, sync in await sync.run()
        }
        XCTAssertEqual(initialA, .synced)
        let initialB = try await withClient(path: pathB, credentials: credentialsB, baseURL: baseURL) {
            _, sync in await sync.run()
        }
        XCTAssertEqual(initialB, .synced)

        try await setOffline(true, at: baseURL)
        let note = "Charging sync integration \(UUID().uuidString)"
        let localID = try await withClient(path: pathA, credentials: credentialsA, baseURL: baseURL) {
            database, sync in
            let receipt = try await database.saveChargingSpot(fields: self.input(note: note))
            let result = await sync.run()
            XCTAssertEqual(result, .offline)
            let pending = try await database.chargingSpot(id: receipt.id)?.pendingOperations
            XCTAssertEqual(pending, 1)
            return receipt.id
        }

        try await setOffline(false, at: baseURL)
        let remoteID = try await withClient(path: pathA, credentials: credentialsA, baseURL: baseURL) {
            database, sync in
            let result = await sync.run()
            XCTAssertEqual(result, .synced)
            let spotValue = try await database.chargingSpot(id: localID)
            return try XCTUnwrap(spotValue?.remoteId)
        }

        let afterCreateB = try await withClient(path: pathB, credentials: credentialsB, baseURL: baseURL) {
            database, sync in
            let result = await sync.run()
            XCTAssertEqual(result, .synced)
            let spotValue = try await database.chargingSpot(id: remoteID)
            let spot = try XCTUnwrap(spotValue)
            XCTAssertEqual(spot.input?.accessNote, note)
            XCTAssertEqual(spot.pendingOperations, 0)
            return spot
        }

        let upVoteResult = try await withClient(path: pathB, credentials: credentialsB, baseURL: baseURL) {
            database, sync in
            _ = try await database.voteChargingSpot(
                id: remoteID, expectedRevision: afterCreateB.localRevision, vote: .up
            )
            return await sync.run()
        }
        XCTAssertEqual(upVoteResult, .synced)
        let afterUpVoteA = try await withClient(path: pathA, credentials: credentialsA, baseURL: baseURL) {
            database, sync in
            let result = await sync.run()
            XCTAssertEqual(result, .synced)
            let spotValue = try await database.chargingSpot(id: localID)
            return try XCTUnwrap(spotValue)
        }
        XCTAssertEqual(afterUpVoteA.votesUp, 1)

        let afterUpVoteB = try await withClient(path: pathB, credentials: credentialsB, baseURL: baseURL) {
            database, _ in
            let value = try await database.chargingSpot(id: remoteID)
            return try XCTUnwrap(value)
        }
        let downVoteResult = try await withClient(path: pathB, credentials: credentialsB, baseURL: baseURL) {
            database, sync in
            _ = try await database.voteChargingSpot(
                id: remoteID, expectedRevision: afterUpVoteB.localRevision, vote: .down
            )
            return await sync.run()
        }
        XCTAssertEqual(downVoteResult, .synced)
        let afterDownVoteA = try await withClient(path: pathA, credentials: credentialsA, baseURL: baseURL) {
            database, sync in
            let result = await sync.run()
            XCTAssertEqual(result, .synced)
            let spotValue = try await database.chargingSpot(id: localID)
            return try XCTUnwrap(spotValue)
        }
        XCTAssertEqual(afterDownVoteA.votesUp, 0)
        XCTAssertEqual(afterDownVoteA.votesDown, 1)

        let afterDownVoteB = try await withClient(path: pathB, credentials: credentialsB, baseURL: baseURL) {
            database, _ in
            let value = try await database.chargingSpot(id: remoteID)
            return try XCTUnwrap(value)
        }
        let clearVoteResult = try await withClient(path: pathB, credentials: credentialsB, baseURL: baseURL) {
            database, sync in
            _ = try await database.voteChargingSpot(
                id: remoteID, expectedRevision: afterDownVoteB.localRevision, vote: nil
            )
            return await sync.run()
        }
        XCTAssertEqual(clearVoteResult, .synced)
        let afterClearA = try await withClient(path: pathA, credentials: credentialsA, baseURL: baseURL) {
            database, sync in
            let result = await sync.run()
            XCTAssertEqual(result, .synced)
            let spotValue = try await database.chargingSpot(id: localID)
            return try XCTUnwrap(spotValue)
        }
        XCTAssertEqual(afterClearA.votesUp, 0)
        XCTAssertEqual(afterClearA.votesDown, 0)

        let editedNote = "Edited \(note)"
        let editResult = try await withClient(path: pathA, credentials: credentialsA, baseURL: baseURL) {
            database, sync in
            _ = try await database.editChargingSpot(
                id: localID, expectedRevision: afterClearA.localRevision,
                fields: self.input(note: editedNote)
            )
            return await sync.run()
        }
        XCTAssertEqual(editResult, .synced)
        let editedB = try await withClient(path: pathB, credentials: credentialsB, baseURL: baseURL) {
            database, sync in
            let result = await sync.run()
            XCTAssertEqual(result, .synced)
            let spotValue = try await database.chargingSpot(id: remoteID)
            return try XCTUnwrap(spotValue)
        }
        XCTAssertEqual(editedB.input?.accessNote, editedNote)

        let withdrawalResult = try await withClient(path: pathA, credentials: credentialsA, baseURL: baseURL) {
            database, sync in
            let latestValue = try await database.chargingSpot(id: localID)
            let latest = try XCTUnwrap(latestValue)
            _ = try await database.withdrawChargingSpot(
                id: localID, expectedRevision: latest.localRevision
            )
            return await sync.run()
        }
        XCTAssertEqual(withdrawalResult, .synced)
        let withdrawnB = try await withClient(path: pathB, credentials: credentialsB, baseURL: baseURL) {
            database, sync in
            let result = await sync.run()
            XCTAssertEqual(result, .synced)
            let spotValue = try await database.chargingSpot(id: remoteID)
            return try XCTUnwrap(spotValue)
        }
        XCTAssertEqual(withdrawnB.status, .withdrawn)
    }

    private func makeSync(
        database: RideDatabaseHandle,
        credentials: IntegrationCredentialStore,
        baseURL: URL
    ) -> ChargingCommunitySync {
        ChargingCommunitySync(
            credentialStore: credentials,
            registerIdentity: {
                let identity = try await mobileRegisterChargingIdentityAtLoopback(
                    baseUrl: baseURL.absoluteString
                )
                return ChargingCommunityCredential(identityID: identity.identityId, token: identity.token)
            },
            synchronize: { token in
                try await database.synchronizeChargingSpotsAtLoopback(
                    token: token, baseUrl: baseURL.absoluteString
                )
            },
            refreshDirectory: {
                try await database.refreshChargingDirectoryAtLoopback(
                    baseUrl: baseURL.absoluteString
                )
            },
            readLastUpdated: { try await database.chargingDirectoryLastUpdatedMs() },
            sleep: { _ in },
            enabled: { true }
        )
    }

    private func withClient<Result>(
        path: URL,
        credentials: IntegrationCredentialStore,
        baseURL: URL,
        operation: @MainActor (RideDatabaseHandle, ChargingCommunitySync) async throws -> Result
    ) async throws -> Result {
        let database = try openRideDatabase(path: path.path)
        let sync = makeSync(
            database: database, credentials: credentials, baseURL: baseURL
        )
        do {
            let result = try await operation(database, sync)
            try database.shutdown()
            return result
        } catch {
            try? database.shutdown()
            throw error
        }
    }

    private func input(note: String) -> MobileChargingInputDto {
        MobileChargingInputDto(
            coordinate: MobileMapCoordinateDto(latitudeDegrees: 39.7392, longitudeDegrees: -104.9903),
            accessNote: note, cost: .free, name: nil, venue: nil, hours: nil,
            terms: nil, price: nil, supplies: []
        )
    }

    private func setOffline(_ offline: Bool, at baseURL: URL) async throws {
        var url = baseURL
        url.append(path: "__test/offline")
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: ["offline": offline])
        let (_, response) = try await URLSession.shared.data(for: request)
        guard let response = response as? HTTPURLResponse, (200..<300).contains(response.statusCode) else {
            throw URLError(.badServerResponse)
        }
    }
}

private actor IntegrationCredentialStore: ChargingCredentialStore {
    private var record: ChargingCredentialRecord?

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
        record = .deleted
    }
}
