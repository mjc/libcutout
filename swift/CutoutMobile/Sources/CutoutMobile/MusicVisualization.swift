import SwiftUI

/// Bounded provider-supplied analysis data for visualization.
///
/// This is intentionally not an audio buffer. A provider adapter may populate
/// it when it has an approved analysis surface; the app never captures or
/// forwards the system audio stream.
public struct MusicAnalysisFrame: Equatable, Sendable {
    public let bass: Double
    public let mid: Double
    public let treble: Double
    public let energy: Double
    public let beat: Double

    public init?(bass: Double, mid: Double, treble: Double, energy: Double, beat: Double) {
        guard Self.isValid(bass), Self.isValid(mid), Self.isValid(treble),
              Self.isValid(energy), Self.isValid(beat) else { return nil }
        self.bass = bass
        self.mid = mid
        self.treble = treble
        self.energy = energy
        self.beat = beat
    }

    private static func isValid(_ value: Double) -> Bool {
        value.isFinite && (0...1).contains(value)
    }
}

/// One normalized RGB frame suitable for a hardware or UI renderer.
public struct MusicRGBFrame: Equatable, Sendable {
    public let red: Double
    public let green: Double
    public let blue: Double
    public let brightness: Double

    public static let silent = MusicRGBFrame(red: 0, green: 0, blue: 0, brightness: 0)

    fileprivate init(red: Double, green: Double, blue: Double, brightness: Double) {
        self.red = red
        self.green = green
        self.blue = blue
        self.brightness = brightness
    }
}

/// Maps optional provider analysis into a stable RGB signal.
public enum MusicVisualizer {
    public static func rgb(from analysis: MusicAnalysisFrame?) -> MusicRGBFrame {
        guard let analysis else { return .silent }
        return MusicRGBFrame(
            red: max(analysis.bass, analysis.beat),
            green: max(analysis.mid, analysis.beat * 0.7),
            blue: max(analysis.treble, analysis.beat * 0.4),
            brightness: max(analysis.energy, analysis.beat)
        )
    }
}

/// A small visualizer preview backed only by normalized provider analysis.
public struct MusicVisualizationView: View {
    public let nowPlaying: MusicNowPlaying

    public init(nowPlaying: MusicNowPlaying) {
        self.nowPlaying = nowPlaying
    }

    public var body: some View {
        if let analysis = nowPlaying.analysis {
            let rgb = MusicVisualizer.rgb(from: analysis)
            HStack(alignment: .bottom, spacing: 6) {
                bar(analysis.bass, color: Color(red: rgb.red, green: 0.18, blue: 0.18))
                bar(analysis.mid, color: Color(red: 0.18, green: rgb.green, blue: 0.18))
                bar(analysis.treble, color: Color(red: 0.18, green: 0.18, blue: rgb.blue))
                bar(analysis.energy, color: Color(red: rgb.red, green: rgb.green, blue: rgb.blue))
                bar(analysis.beat, color: Color(red: rgb.red, green: rgb.green, blue: rgb.blue))
            }
            .frame(maxWidth: .infinity, minHeight: 64, maxHeight: 64, alignment: .bottom)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(pevLocalizedText("music.visualization.title"))
        } else {
            Label(
                pevLocalizedText("music.visualization.unavailable"),
                systemImage: "waveform"
            )
            .foregroundStyle(.secondary)
        }
    }

    private func bar(_ value: Double, color: Color) -> some View {
        Capsule()
            .fill(color)
            .frame(maxWidth: .infinity)
            .frame(height: max(8, 56 * value))
    }
}
