import CutoutMobileFFI
import XCTest
@testable import CutoutMobile

@MainActor
final class MusicProviderTransportExecutorTests: XCTestCase {
    func testOverlappingAppleCommandsAdmitOnlyOneRustTransport() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let provider = try! XCTUnwrap(lifecycle.beginProviderSession())
        let gate = TransportOperationGate()
        let transport = MusicProviderTransportExecutor(
            lifecycle: lifecycle,
            effects: MusicProviderEffectExecutor(),
            nowMs: { 0 }
        )

        let first = Task { @MainActor in
            await transport.perform(providerGeneration: provider, command: .play) { _, completion in
                Task { @MainActor in completion(await gate.wait()) }
            }
        }
        let started = await gate.waitUntilOperationStarted()
        XCTAssertTrue(started)

        let overlapping = await transport.perform(providerGeneration: provider, command: .play) { _, completion in completion(true) }
        XCTAssertEqual(overlapping, .refused)

        await gate.resume(returning: true)
        let firstOutcome = await first.value
        XCTAssertEqual(firstOutcome, .accepted)
    }

    func testProviderChangeRejectsLateAppleCommandCompletion() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let apple = try! XCTUnwrap(lifecycle.beginProviderSession())
        let gate = TransportOperationGate()
        let transport = MusicProviderTransportExecutor(
            lifecycle: lifecycle,
            effects: MusicProviderEffectExecutor(),
            nowMs: { 0 }
        )
        let command = Task { @MainActor in
            await transport.perform(providerGeneration: apple, command: .play) { _, completion in
                Task { @MainActor in completion(await gate.wait()) }
            }
        }
        let started = await gate.waitUntilOperationStarted()
        XCTAssertTrue(started)

        transport.apply(lifecycle.retireProviderSession(id: apple))
        _ = lifecycle.beginProviderSession()
        await gate.resume(returning: true)

        let outcome = await command.value
        XCTAssertEqual(outcome, .unavailable)
    }

    func testAppleBackgroundSuspensionCompletesPendingCommand() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let provider = try! XCTUnwrap(lifecycle.beginProviderSession())
        let gate = TransportOperationGate()
        let transport = MusicProviderTransportExecutor(
            lifecycle: lifecycle,
            effects: MusicProviderEffectExecutor(),
            nowMs: { 0 }
        )
        let command = Task { @MainActor in
            await transport.perform(providerGeneration: provider, command: .play) { _, completion in
                Task { @MainActor in completion(await gate.wait()) }
            }
        }
        let started = await gate.waitUntilOperationStarted()
        XCTAssertTrue(started)

        transport.apply(lifecycle.suspend())

        let outcome = await command.value
        XCTAssertEqual(outcome, .unavailable)
        await gate.resume(returning: true)
    }

    func testAlreadyCancelledCommandDoesNotDispatchProviderEffect() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let provider = try! XCTUnwrap(lifecycle.beginProviderSession())
        let transport = MusicProviderTransportExecutor(
            lifecycle: lifecycle,
            effects: MusicProviderEffectExecutor(),
            nowMs: { 0 }
        )
        var dispatched = false
        let command = Task { @MainActor in
            await transport.perform(providerGeneration: provider, command: .play) { _, completion in
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

    func waitUntilOperationStarted(timeout: Duration = .seconds(1)) async -> Bool {
        await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                while !Task.isCancelled {
                    if await self.isOperationStarted() { return true }
                    await Task.yield()
                }
                return false
            }
            group.addTask {
                do {
                    try await Task.sleep(for: timeout)
                    return false
                } catch {
                    return true
                }
            }
            let result = await group.next() ?? false
            group.cancelAll()
            return result
        }
    }

    func isOperationStarted() -> Bool {
        continuation != nil
    }
}
