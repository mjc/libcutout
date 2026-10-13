import CutoutMobileFFI
import Foundation
import XCTest

@testable import CutoutMobile

final class ChargingBoundaryTests: XCTestCase {
    func testMinimumFieldsStayOptionalAndBlankAccessNoteReturnsTypedRustError() async throws {
        var fields = MobileChargingInputDto(
            coordinate: MobileMapCoordinateDto(latitudeDegrees: 40, longitudeDegrees: -105),
            accessNote: " ", cost: .free, name: nil, venue: nil, hours: nil,
            terms: nil, price: nil, supplies: []
        )
        XCTAssertNil(fields.name)
        XCTAssertNil(fields.price)
        XCTAssertTrue(fields.supplies.isEmpty)
        let fixtureDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "CutoutMobile-ChargingBoundaryTests-\(UUID().uuidString)", isDirectory: true
        )
        try FileManager.default.createDirectory(
            at: fixtureDirectory, withIntermediateDirectories: true
        )
        defer { try? FileManager.default.removeItem(at: fixtureDirectory) }
        let databasePath = fixtureDirectory.appendingPathComponent("charging.sqlite").path
        let database = try openRideDatabase(path: databasePath)
        defer { try? database.shutdown() }
        do {
            _ = try await database.saveChargingSpot(fields: fields)
            XCTFail("Expected the Rust access-note error")
        } catch {
            guard case MobileChargingError.AccessNoteBlank = error else {
                return XCTFail("Expected the Rust access-note error, got \(error)")
            }
        }
        fields.accessNote = "Patio outlet"
        fields.supplies = [MobileChargingSupplyDto(voltageVolts: 0, connectors: [.unknown])]
        do {
            _ = try await database.saveChargingSpot(fields: fields)
            XCTFail("Expected the Rust supply validation error")
        } catch {
            guard case MobileChargingError.InvalidMetadata = error else {
                return XCTFail("Expected the Rust supply validation error, got \(error)")
            }
        }
        do {
            _ = try await database.chargingSpot(id: "invalid")
            XCTFail("Expected the Rust identifier error")
        } catch {
            guard case MobileChargingError.InvalidIdentifier = error else {
                return XCTFail("Expected the Rust identifier error, got \(error)")
            }
        }
    }

}
