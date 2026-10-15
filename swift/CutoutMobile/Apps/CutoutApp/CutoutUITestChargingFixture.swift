#if DEBUG
    import CutoutMobile
    import CutoutMobileFFI
    import Foundation
    import MapKit

    enum CutoutUITestChargingFixture {
        static var isEnabled: Bool {
            if ChargingAcceptanceEnvironment.databaseID != nil { return true }
            #if os(iOS) && targetEnvironment(simulator)
                return CommandLine.arguments.contains("--seed-ui-test-charging-spots")
                    && CutoutUITestSessionFixture(arguments: CommandLine.arguments) != nil
                    && ProcessInfo.processInfo.environment["CUTOUT_UI_TEST_RIDE_DATABASE_ID"]
                        .flatMap(UUID.init(uuidString:)) != nil
            #else
                return false
            #endif
        }

        static var region: MKCoordinateRegion {
            MKCoordinateRegion(
                center: CLLocationCoordinate2D(latitude: 39.74, longitude: -104.99),
                span: MKCoordinateSpan(latitudeDelta: 0.018, longitudeDelta: 0.018)
            )
        }

        static func runIfRequested(database: RideDatabaseHandle) async throws {
            guard isEnabled else { return }
            let bounds = MobileGeoBoundsDto(
                minimumLatitudeDegrees: 39.72, maximumLatitudeDegrees: 39.76,
                minimumLongitudeDegrees: -105.01, maximumLongitudeDegrees: -104.97
            )
            let existing = try await database.chargingSpotsInBounds(
                bounds: bounds,
                selection: MobileChargingFilterDto(cost: nil, connector: nil, voltageVolts: nil, venue: nil),
                cursor: nil, limit: 1
            )
            guard existing.spots.isEmpty else { return }
            let inputs = [
                MobileChargingInputDto(
                    coordinate: MobileMapCoordinateDto(latitudeDegrees: 39.74, longitudeDegrees: -104.99),
                    accessNote: "Outlet beside the entrance. Ask at the desk.", cost: .free,
                    name: "Library", venue: .other, hours: "09:00–18:00", terms: nil, price: nil,
                    supplies: [MobileChargingSupplyDto(voltageVolts: 120, connectors: [.nema515], outletCount: nil)]
                ),
                MobileChargingInputDto(
                    coordinate: MobileMapCoordinateDto(latitudeDegrees: 39.744, longitudeDegrees: -104.986),
                    accessNote: "Ask staff before using the patio outlet.", cost: .paid,
                    name: "Coffee shop", venue: .cafe, hours: nil, terms: .purchaseRequired, price: nil,
                    supplies: [MobileChargingSupplyDto(voltageVolts: nil, connectors: [], outletCount: nil)]
                ),
                MobileChargingInputDto(
                    coordinate: MobileMapCoordinateDto(latitudeDegrees: 39.736, longitudeDegrees: -104.995),
                    accessNote: "Chargers on the east side of the parking lot.", cost: .paid,
                    name: nil, venue: .evChargingStation, hours: nil, terms: .directFee, price: nil,
                    supplies: [MobileChargingSupplyDto(voltageVolts: 240, connectors: [.j1772], outletCount: nil)]
                ),
            ]
            for input in inputs { _ = try await database.saveChargingSpot(fields: input) }
        }
    }
#endif
