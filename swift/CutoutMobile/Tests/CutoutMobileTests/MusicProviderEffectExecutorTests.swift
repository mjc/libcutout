import CutoutMobileFFI
import XCTest

@testable import CutoutMobile

@MainActor
final class MusicProviderEffectExecutorTests: XCTestCase {
    func testCancelledRustIdentityCannotRemoveItsReplacementTask() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        lifecycle.requestMonitor(request: .observe)
        let first = try! XCTUnwrap(lifecycle.beginMonitor())
        let firstStarted = expectation(description: "first started")
        let firstFinished = expectation(description: "first finished")
        effects.run(.monitor(first.generation)) {
            firstStarted.fulfill()
            do { try await Task.sleep(for: .seconds(30)) } catch {}
            firstFinished.fulfill()
        }
        await fulfillment(of: [firstStarted], timeout: 1)

        let replacement = try! XCTUnwrap(lifecycle.beginMonitor())
        let replacementStarted = expectation(description: "replacement started")
        effects.run(.monitor(replacement.generation)) {
            replacementStarted.fulfill()
            do { try await Task.sleep(for: .seconds(30)) } catch {}
        }
        effects.cancel(.monitor(first.generation))

        await fulfillment(of: [firstFinished, replacementStarted], timeout: 1)
        XCTAssertFalse(effects.isRunning(.monitor(first.generation)))
        XCTAssertTrue(effects.isRunning(.monitor(replacement.generation)))
        effects.cancelAll()
    }

    func testNamespaceCancellationStopsOnlyMatchingRustIssuedEffects() async {
        let lifecycle = MobileMusicProviderLifecycle()
        let effects = MusicProviderEffectExecutor()
        let provider = try! XCTUnwrap(lifecycle.beginProviderSession())
        let playerState = try! XCTUnwrap(lifecycle.beginPlayerStateRequest(nowMs: 0))
        effects.run(.provider(provider)) { try? await Task.sleep(for: .seconds(30)) }
        effects.run(.playerState(playerState)) { try? await Task.sleep(for: .seconds(30)) }

        effects.cancelAll(in: .playerState)

        XCTAssertTrue(effects.isRunning(.provider(provider)))
        XCTAssertFalse(effects.isRunning(.playerState(playerState)))
        effects.cancelAll()
    }
}
