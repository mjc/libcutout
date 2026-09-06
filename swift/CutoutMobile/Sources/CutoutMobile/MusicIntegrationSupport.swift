import CutoutMobileFFI
import Foundation

/// Presentation-only artwork retained in Swift and bounded before decoding.
/// Artwork never enters the Rust ride or UniFFI contracts.
public struct MusicArtwork: Equatable, Sendable {
    public static let maxBytes = 512 * 1024

    public let data: Data

    public init?(data: Data) {
        guard data.isEmpty == false, data.count <= Self.maxBytes else { return nil }
        self.data = data
    }
}

/// Validates provider observations before they enter the shared music pipeline.
enum MusicObservationValidator {
    private static let identifierMaxBytes = 256
    private static let displayTextMaxBytes = 512

    static func accepts(_ snapshot: MobileMusicSnapshotDto) -> Bool {
        guard acceptsRequired(snapshot.sessionId, maxBytes: identifierMaxBytes) else { return false }
        if let position = snapshot.positionMilliseconds,
           let duration = snapshot.durationMilliseconds,
           position > duration
        {
            return false
        }
        guard let item = snapshot.item else { return true }
        guard acceptsRequired(item.identifier, maxBytes: identifierMaxBytes) else { return false }
        return acceptsOptional(item.title, maxBytes: displayTextMaxBytes)
            && acceptsOptional(item.artist, maxBytes: displayTextMaxBytes)
    }

    private static func acceptsRequired(_ value: String, maxBytes: Int) -> Bool {
        value.utf8.count <= maxBytes
            && value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
    }

    private static func acceptsOptional(_ value: String?, maxBytes: Int) -> Bool {
        guard let value else { return true }
        return acceptsRequired(value, maxBytes: maxBytes)
    }
}

/// Keeps one positive artwork result so polling does not repeatedly decode the
/// same provider image. The value is already bounded by `MusicArtwork`.
struct MusicArtworkCache: Sendable {
    private var itemIdentifier: String?
    private var cachedArtwork: MusicArtwork?

    mutating func artwork(
        for itemIdentifier: String?,
        load: () -> MusicArtwork?
    ) -> MusicArtwork? {
        guard let itemIdentifier else {
            clear()
            return nil
        }
        if self.itemIdentifier == itemIdentifier, let cachedArtwork {
            return cachedArtwork
        }
        let artwork = load()
        if let artwork {
            self.itemIdentifier = itemIdentifier
            cachedArtwork = artwork
        } else {
            clear()
        }
        return artwork
    }

    private mutating func clear() {
        itemIdentifier = nil
        cachedArtwork = nil
    }
}

/// Converts provider seconds into bounded milliseconds without trapping.
enum MusicTimeConversion {
    static func milliseconds(_ seconds: TimeInterval) -> UInt64? {
        guard seconds.isFinite, seconds >= 0 else { return nil }
        let milliseconds = seconds * 1_000
        guard milliseconds < Double(UInt64.max) else { return nil }
        return UInt64(milliseconds)
    }
}
