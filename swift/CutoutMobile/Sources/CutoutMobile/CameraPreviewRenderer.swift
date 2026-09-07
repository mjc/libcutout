import AVFoundation
import CoreMedia
import Foundation
import SwiftUI
import CutoutMobileFFI

/// Failure while turning one encoded camera frame into a displayable sample.
public enum CameraPreviewRendererError: Error, Equatable, Sendable {
    case invalidTiming
    case missingParameterSets
    case formatDescription
    case sampleBuffer
}

/// Main-actor H.264 renderer shared by iOS and macOS camera routes.
@MainActor
public final class CameraPreviewRenderer {
    public let displayLayer: AVSampleBufferDisplayLayer

    private var formatDescription: CMVideoFormatDescription?
    private var hasStartedTimeline = false
    private let renderSynchronizer: AVSampleBufferRenderSynchronizer
    private let receiver: AVSampleBufferVideoRenderer.Receiver

    public init() {
        let displayLayer = AVSampleBufferDisplayLayer()
        let renderSynchronizer = AVSampleBufferRenderSynchronizer()
        self.displayLayer = displayLayer
        self.renderSynchronizer = renderSynchronizer
        self.receiver = renderSynchronizer.sampleBufferReceiver(
            adding: displayLayer.sampleBufferRenderer
        )
        renderSynchronizer.rate = 1
        displayLayer.videoGravity = .resizeAspect
    }

    /// Enqueues one AVCC H.264 frame for native platform decoding.
    ///
    /// The first access unit carrying SPS and PPS establishes the format
    /// description. Frames received before that point are rejected rather
    /// than displayed with guessed dimensions or codec state.
    public func enqueue(_ frame: MobileCameraVideoFrameDto) async throws {
        let accessUnit = try CameraH264AccessUnit(data: frame.data)
        if accessUnit.parameterSets.count == 2 {
            formatDescription = try makeFormatDescription(parameterSets: accessUnit.parameterSets)
        }
        guard let formatDescription else {
            throw CameraPreviewRendererError.missingParameterSets
        }
        guard frame.timestamp >= 0,
              frame.clockRateHz > 0,
              let timescale = CMTimeScale(exactly: frame.clockRateHz)
        else {
            throw CameraPreviewRendererError.invalidTiming
        }

        let timestamp = CMTime(value: frame.timestamp, timescale: timescale)
        guard let sampleBuffer = makeSampleBuffer(
            data: accessUnit.data,
            timestamp: timestamp,
            formatDescription: formatDescription
        ) else {
            throw CameraPreviewRendererError.sampleBuffer
        }
        if !hasStartedTimeline {
            renderSynchronizer.setRate(1, time: timestamp)
            hasStartedTimeline = true
        }
        _ = try await receiver.enqueue(CMReadySampleBuffer(unsafeBuffer: sampleBuffer))
    }

    /// Flushes queued samples and forgets codec parameter sets.
    public func reset() {
        receiver.flush()
        formatDescription = nil
        hasStartedTimeline = false
    }

    private func makeFormatDescription(parameterSets: [Data]) throws -> CMVideoFormatDescription {
        guard parameterSets.count == 2 else {
            throw CameraPreviewRendererError.missingParameterSets
        }

        var description: CMVideoFormatDescription?
        let status = parameterSets[0].withUnsafeBytes { sps in
            parameterSets[1].withUnsafeBytes { pps in
                let pointers: [UnsafePointer<UInt8>] = [
                    sps.bindMemory(to: UInt8.self).baseAddress!,
                    pps.bindMemory(to: UInt8.self).baseAddress!,
                ]
                var sizes = [parameterSets[0].count, parameterSets[1].count]
                return CMVideoFormatDescriptionCreateFromH264ParameterSets(
                    allocator: kCFAllocatorDefault,
                    parameterSetCount: pointers.count,
                    parameterSetPointers: pointers.withUnsafeBufferPointer { $0.baseAddress! },
                    parameterSetSizes: &sizes,
                    nalUnitHeaderLength: 4,
                    formatDescriptionOut: &description
                )
            }
        }
        guard status == noErr, let description else {
            throw CameraPreviewRendererError.formatDescription
        }
        return description
    }

    nonisolated private func makeSampleBuffer(
        data: Data,
        timestamp: CMTime,
        formatDescription: CMVideoFormatDescription
    ) -> CMSampleBuffer? {
        var blockBuffer: CMBlockBuffer?
        guard CMBlockBufferCreateWithMemoryBlock(
            allocator: kCFAllocatorDefault,
            memoryBlock: nil,
            blockLength: data.count,
            blockAllocator: kCFAllocatorDefault,
            customBlockSource: nil,
            offsetToData: 0,
            dataLength: data.count,
            flags: 0,
            blockBufferOut: &blockBuffer
        ) == kCMBlockBufferNoErr,
        let blockBuffer
        else {
            return nil
        }

        let replaceStatus = data.withUnsafeBytes { bytes in
            CMBlockBufferReplaceDataBytes(
                with: bytes.baseAddress!,
                blockBuffer: blockBuffer,
                offsetIntoDestination: 0,
                dataLength: data.count
            )
        }
        guard replaceStatus == kCMBlockBufferNoErr else { return nil }

        var timing = CMSampleTimingInfo(
            duration: .invalid,
            presentationTimeStamp: timestamp,
            decodeTimeStamp: .invalid
        )
        var sampleSize = data.count
        var sampleBuffer: CMSampleBuffer?
        let status = CMSampleBufferCreateReady(
            allocator: kCFAllocatorDefault,
            dataBuffer: blockBuffer,
            formatDescription: formatDescription,
            sampleCount: 1,
            sampleTimingEntryCount: 1,
            sampleTimingArray: &timing,
            sampleSizeEntryCount: 1,
            sampleSizeArray: &sampleSize,
            sampleBufferOut: &sampleBuffer
        )
        guard status == noErr else { return nil }
        return sampleBuffer
    }
}

/// SwiftUI surface backed by an AVSampleBufferDisplayLayer.
@MainActor
public struct CameraPreviewSurface: View {
    private let renderer: CameraPreviewRenderer

    public init(renderer: CameraPreviewRenderer) {
        self.renderer = renderer
    }

    public var body: some View {
        CameraPreviewLayerView(renderer: renderer)
            .background(.black)
            .clipShape(.rect(cornerRadius: 20))
            .aspectRatio(16 / 9, contentMode: .fit)
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(pevLocalizedText("camera.preview.accessibility"))
    }
}

#if canImport(UIKit)
import UIKit

@MainActor
private struct CameraPreviewLayerView: UIViewRepresentable {
    let renderer: CameraPreviewRenderer

    func makeUIView(context: Context) -> UIView {
        let view = UIView()
        view.backgroundColor = .black
        view.layer.addSublayer(renderer.displayLayer)
        return view
    }

    func updateUIView(_ view: UIView, context: Context) {
        renderer.displayLayer.frame = view.bounds
    }
}
#elseif canImport(AppKit)
import AppKit

@MainActor
private struct CameraPreviewLayerView: NSViewRepresentable {
    let renderer: CameraPreviewRenderer

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor.black.cgColor
        view.layer?.addSublayer(renderer.displayLayer)
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        renderer.displayLayer.frame = view.bounds
    }
}
#endif
