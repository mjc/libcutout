import CutoutMobile
import CutoutMobileFFI
import Foundation

#if canImport(CoreBluetooth)
import CoreBluetooth

@main
struct MelkLightingLiveValidator {
    static func main() {
        let timeout = CommandLine.arguments.dropFirst().first.flatMap(Double.init) ?? 60
        let session = MelkLightingPeripheralSession()
        let startedAt = Date()
        var ready = false
        var finished = false

        session.onIdentity = { identity in
            print("identity name=\(identity.name ?? "unknown") id=\(identity.platformIdentifier) rssi=\(identity.rssi.map(String.init) ?? "unknown")")
        }
        session.onStateChange = { state in
            print("state=\(state)")
            if case .ready = state { ready = true }
            if case .failed = state { finished = true }
        }
        session.onNotification = { data in
            print("notification=\(data.map { String(format: "%02x", $0) }.joined(separator: " "))")
        }
        session.onRecord = { record in print("record=\(record)") }

        print("Scanning for MELK-OC21 on macOS. Press Ctrl-C to stop.")
        session.start()
        while !ready, !finished, Date().timeIntervalSince(startedAt) < timeout {
            RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.1))
        }
        if ready {
            print("ready: enter commands (help for list, quit to disconnect)")
            while !finished {
                guard let line = readLine(strippingNewline: true) else { break }
                if line == "quit" || line == "exit" { break }
                handle(line, session: session)
            }
        } else {
            print("validation=timeout")
        }
        session.stop()
    }

    private static func handle(_ line: String, session: MelkLightingPeripheralSession) {
        let parts = line.split(separator: " ").map(String.init)
        guard let command = parts.first else { return }
        switch command {
        case "help":
            print("power on|off; color R G B; brightness 0-100; speed 0-255; effect 1|16|22|75 [speed]; confirm; unconfirm; quit")
        case "power" where parts.count == 2:
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
        default:
            print("invalid command; use help")
        }
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
            guard let pattern = UInt8(parts[1]), [1, 16, 22, 75].contains(pattern) else { print("unverified effect; use one of 1,16,22,75"); return }
            let speed = parts.count == 3 ? UInt8(parts[2]) ?? 50 : 50
            let state = MobileMelkLightingRestoreStateDto(powerOn: true, red: 255, green: 255, blue: 255, brightness: 100, playback: .effect(pattern: pattern, speed: speed))
            do { print("requested=\(try session.applyState(state))") } catch { print("error=\(error)") }
        default: break
        }
    }
}
#else
@main
struct MelkLightingLiveValidator {
    static func main() { print("CoreBluetooth is unavailable on this platform"); exit(EXIT_FAILURE) }
}
#endif
