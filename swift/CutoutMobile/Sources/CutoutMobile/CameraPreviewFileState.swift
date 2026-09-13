import Foundation

/// Tracks when a local preview destination has received usable video bytes.
///
/// A destination is not presented as a saved capture until the first frame is
/// written successfully. Subsequent frames keep the file live without
/// repeatedly publishing the same URL.
struct CameraPreviewFileState: Equatable, Sendable {
    let destination: URL?
    private(set) var frameCount = 0

    mutating func recordFrame() -> URL? {
        guard destination != nil else { return nil }
        frameCount += 1
        return frameCount == 1 ? destination : nil
    }
}
