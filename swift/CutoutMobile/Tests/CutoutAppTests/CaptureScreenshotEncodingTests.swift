import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers
import XCTest

@testable import CutoutApp

final class CaptureScreenshotEncodingTests: XCTestCase {
    func testSelectedScreenshotIsResizedAndDoesNotCopyPhotoLocationOrCaptureDate() throws {
        let context = try XCTUnwrap(
            CGContext(
                data: nil, width: 2200, height: 20, bitsPerComponent: 8, bytesPerRow: 2200 * 4,
                space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue))
        let image = try XCTUnwrap(context.makeImage())
        let sourceData = NSMutableData()
        let destination = try XCTUnwrap(
            CGImageDestinationCreateWithData(
                sourceData, UTType.jpeg.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(
            destination, image,
            [
                kCGImagePropertyGPSDictionary: [
                    kCGImagePropertyGPSLatitude: 39.7,
                    kCGImagePropertyGPSLatitudeRef: "N",
                    kCGImagePropertyGPSLongitude: 105.0,
                    kCGImagePropertyGPSLongitudeRef: "W",
                ],
                kCGImagePropertyExifDictionary: [kCGImagePropertyExifDateTimeOriginal: "2026:10:19 12:00:00"],
            ] as CFDictionary)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let original = try XCTUnwrap(CGImageSourceCreateWithData(sourceData as CFData, nil))
        let originalProperties = try XCTUnwrap(CGImageSourceCopyPropertiesAtIndex(original, 0, nil) as? [CFString: Any])
        XCTAssertNotNil(originalProperties[kCGImagePropertyGPSDictionary])

        let encoded = try captureScreenshotJPEG(sourceData as Data)
        let decoded = try XCTUnwrap(CGImageSourceCreateWithData(encoded as CFData, nil))
        let imageType = try XCTUnwrap(CGImageSourceGetType(decoded))
        XCTAssertEqual(imageType as String, UTType.jpeg.identifier)
        let properties = try XCTUnwrap(CGImageSourceCopyPropertiesAtIndex(decoded, 0, nil) as? [CFString: Any])
        XCTAssertLessThanOrEqual(try XCTUnwrap(properties[kCGImagePropertyPixelWidth] as? Int), 2048)
        XCTAssertNil(properties[kCGImagePropertyGPSDictionary])
        let exif = properties[kCGImagePropertyExifDictionary] as? [CFString: Any]
        XCTAssertNil(exif?[kCGImagePropertyExifDateTimeOriginal])
    }

    func testUnreadableImageIsRejectedBeforeItCanBeSaved() {
        XCTAssertThrowsError(try captureScreenshotJPEG(Data("not an image".utf8)))
    }
}
