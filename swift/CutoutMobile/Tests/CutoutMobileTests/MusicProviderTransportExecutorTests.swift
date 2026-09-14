import CutoutMobileFFI
import XCTest
@testable import CutoutMobile

@MainActor
final class MusicProviderTransportExecutorTests: XCTestCase {
    func testOverlappingAppleCommandsAdmitOnlyOneRustTransport() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let provider = lifecycle.beginProviderSession()
        let gate = TransportOperationGate()
        let transport = MusicProviderTransportExecutor(
            lifecycle: lifecycle,
            effects: MusicProviderEffectExecutor(),
            nowMs: { 0 }
        )

        let first = Task { @MainActor in
            await transport.perform(providerGeneration: provider) { completion in
                Task { @MainActor in completion(await gate.wait()) }
            }
        }
        await gate.waitUntilOperationStarted()

        let overlapping = await transport.perform(providerGeneration: provider) { $0(true) }
        XCTAssertEqual(overlapping, .refused)

        await gate.resume(returning: true)
        let firstOutcome = await first.value
        XCTAssertEqual(firstOutcome, .accepted)
    }

    func testProviderChangeRejectsLateAppleCommandCompletion() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let apple = lifecycle.beginProviderSession()
        let gate = TransportOperationGate()
        let transport = MusicProviderTransportExecutor(
            lifecycle: lifecycle,
            effects: MusicProviderEffectExecutor(),
            nowMs: { 0 }
        )
        let command = Task { @MainActor in
            await transport.perform(providerGeneration: apple) { completion in
                Task { @MainActor in completion(await gate.wait()) }
            }
        }
        await gate.waitUntilOperationStarted()

        transport.apply(lifecycle.retireProviderSession(id: apple))
        _ = lifecycle.beginProviderSession()
        await gate.resume(returning: true)

        let outcome = await command.value
        XCTAssertEqual(outcome, .unavailable)
    }

    func testAppleBackgroundSuspensionCompletesPendingCommand() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let provider = lifecycle.beginProviderSession()
        let gate = TransportOperationGate()
        let transport = MusicProviderTransportExecutor(
            lifecycle: lifecycle,
            effects: MusicProviderEffectExecutor(),
            nowMs: { 0 }
        )
        let command = Task { @MainActor in
            await transport.perform(providerGeneration: provider) { completion in
                Task { @MainActor in completion(await gate.wait()) }
            }
        }
        await gate.waitUntilOperationStarted()

        transport.apply(lifecycle.suspend())

        let outcome = await command.value
        XCTAssertEqual(outcome, .unavailable)
        await gate.resume(returning: true)
    }

    func testAlreadyCancelledCommandDoesNotDispatchProviderEffect() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let provider = lifecycle.beginProviderSession()
        let transport = MusicProviderTransportExecutor(
            lifecycle: lifecycle,
            effects: MusicProviderEffectExecutor(),
            nowMs: { 0 }
        )
        var dispatched = false
        let command = Task { @MainActor in
            await transport.perform(providerGeneration: provider) { completion in
                dispatched = true
                completion(true)
            }
        }
        command.cancel()

        let outcome = await command.value
        XCTAssertEqual(outcome, .unavailable)
        XCTAssertFalse(dispatched)
    }
}

private actor TransportOperationGate {
    private var continuation: CheckedContinuation<Bool, Never>?
    private var bufferedValue: Bool?

    func wait() async -> Bool {
        if let bufferedValue {
            self.bufferedValue = nil
            return bufferedValue
        }
        return await withCheckedContinuation { continuation = $0 }
    }

    func resume(returning value: Bool) {
        if let continuation {
            self.continuation = nil
            continuation.resume(returning: value)
        } else {
            bufferedValue = value
        }
    }

    func waitUntilOperationStarted() async {
        while continuation == nil {
            await Task.yield()
        }
    }
}
