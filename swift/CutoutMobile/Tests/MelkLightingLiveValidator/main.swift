import CutoutMobile
import CutoutMobileFFI
import Foundation

#if canImport(CoreBluetooth)
import CoreBluetooth

@main
struct MelkLightingLiveValidator {
    static func main() {
        let arguments = Array(CommandLine.arguments.dropFirst())
        let timeout = arguments.first.flatMap(Double.init) ?? 60
        let preferredPlatformIdentifier = arguments.dropFirst().first
        let session = MelkLightingPeripheralSession()
        let startedAt = Date()
        var advertisedIdentifiers = Set<String>()
        let input = InputBuffer()
        let validationState = ValidationState()

        session.onCandidate = { candidate in
            print("candidate name=\(candidate.name ?? "unknown") id=\(candidate.id) rssi=\(candidate.rssi); enter `select \(candidate.id)`")
        }

        session.onIdentity = { identity in
            print("identity name=\(identity.name ?? "unknown") id=\(identity.platformIdentifier) rssi=\(identity.rssi.map(String.init) ?? "unknown")")
        }
        session.onAdvertisement = { name, identifier, rssi in
            guard advertisedIdentifiers.insert(identifier).inserted else { return }
            print("advertisement name=\(name ?? "unknown") id=\(identifier) rssi=\(rssi)")
        }
        session.onStateChange = { state in
            print("state=\(state)")
            validationState.apply(state)
        }
        session.onNotification = { data in
            print("notification=\(data.map { String(format: "%02x", $0) }.joined(separator: " "))")
        }
        session.onRecord = { record in print("record=\(record)") }

        Thread {
            while let line = readLine(strippingNewline: true) {
                input.append(line)
            }
            input.append("quit")
        }.start()

        print("Scanning for MELK-OC21 on macOS. Press Ctrl-C to stop.")
        if let preferredPlatformIdentifier {
            print("target=melk id=\(preferredPlatformIdentifier)")
        }
        session.start(preferredPlatformIdentifier: preferredPlatformIdentifier)
        while true {
            let snapshot = validationState.snapshot()
            guard !snapshot.ready, !snapshot.finished,
                  Date().timeIntervalSince(startedAt) < timeout else { break }
            for line in input.drain() {
                if handle(line, session: session) { validationState.finish() }
            }
            RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.1))
        }
        let discoveryResult = validationState.snapshot()
        if discoveryResult.ready, !discoveryResult.failed {
            print("ready: enter commands (help for list, quit to disconnect)")
            while !validationState.snapshot().finished {
                for line in input.drain() {
                    if handle(line, session: session) { validationState.finish() }
                }
                RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.1))
            }
        } else {
            print(discoveryResult.failed ? "validation=failed" : "validation=timeout")
            session.stop()
            exit(EXIT_FAILURE)
        }
        session.stop()
        if validationState.snapshot().failed { exit(EXIT_FAILURE) }
    }

    private static let verifiedEffectIDs = Set(mobileMelkLightingCapabilities().verifiedEffectIds)

    private static func handle(_ line: String, session: MelkLightingPeripheralSession) -> Bool {
        let parts = line.split(separator: " ").map(String.init)
        guard let command = parts.first else { return false }
        switch command {
        case "help":
            print("select <UUID>; power on|off; color R G B; brightness 0-100; speed 0-255; effect <verified-id> [speed]; confirm; unconfirm; quit")
        case "select" where parts.count == 2:
            session.selectCandidate(platformIdentifier: parts[1])
        case "power" where parts.count == 2:
            guard parts[1] == "on" || parts[1] == "off" else {
                print("invalid power; use on or off")
                return false
            }
            print("requested=\(session.setPower(parts[1] == "on"))")
        case "color" where parts.count == 4,
             "brightness" where parts.count == 2,
             "speed" where parts.count == 2,
             "effect" where parts.count == 2 || parts.count == 3:
            handleLighting(parts, session: session)
        case "confirm":
            session.markLastCommandConfirmed()
            print("evidence=confirmed")
        case "unconfirm":
            session.markLastCommandUnconfirmed()
            print("evidence=unconfirmed")
        case "quit", "exit":
            break
        default:
            print("invalid command; use help")
        }
        return command == "quit" || command == "exit"
    }

    private static func handleLighting(_ parts: [String], session: MelkLightingPeripheralSession) {
        switch parts[0] {
        case "color":
            guard let r = UInt8(parts[1]), let g = UInt8(parts[2]), let b = UInt8(parts[3]) else { print("invalid color"); return }
            print("requested=\(session.setSolidColor(red: r, green: g, blue: b))")
        case "brightness":
            guard let value = UInt8(parts[1]) else { print("invalid brightness"); return }
            do { print("requested=\(try session.setBrightness(value))") } catch { print("error=\(error)") }
        case "speed":
            guard let value = UInt8(parts[1]) else { print("invalid speed"); return }
            print("requested=\(session.setEffectSpeed(value))")
        case "effect":
            guard let pattern = UInt8(parts[1]), verifiedEffectIDs.contains(pattern) else { print("unverified effect; use a capture-backed effect ID"); return }
            let speed: UInt8
            if parts.count == 3 {
                guard let suppliedSpeed = UInt8(parts[2]) else {
                    print("invalid speed; use 0-255")
                    return
                }
                speed = suppliedSpeed
            } else {
                speed = 50
            }
            let state = MobileMelkLightingRestoreStateDto(powerOn: true, red: 255, green: 255, blue: 255, brightness: 100, playback: .effect(pattern: pattern, speed: speed))
            do { print("requested=\(try session.applyState(state))") } catch { print("error=\(error)") }
        default: break
        }
    }
}

private final class InputBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private var lines: [String] = []

    func append(_ line: String) {
        lock.lock()
        lines.append(line)
        lock.unlock()
    }

    func drain() -> [String] {
        lock.lock()
        defer { lock.unlock() }
        let drained = lines
        lines.removeAll(keepingCapacity: true)
        return drained
    }
}

private final class ValidationState: @unchecked Sendable {
    private let lock = NSLock()
    private var ready = false
    private var failed = false
    private var finished = false

    func apply(_ state: MelkLightingPeripheralState) {
        lock.lock()
        defer { lock.unlock() }
        if case .ready = state { ready = true }
        if case .failed = state {
            failed = true
            finished = true
        }
    }

    func finish() {
        lock.lock()
        finished = true
        lock.unlock()
    }

    func snapshot() -> (ready: Bool, failed: Bool, finished: Bool) {
        lock.lock()
        defer { lock.unlock() }
        return (ready, failed, finished)
    }
}
#else
@main
struct MelkLightingLiveValidator {
    static func main() { print("CoreBluetooth is unavailable on this platform"); exit(EXIT_FAILURE) }
}
#endif
