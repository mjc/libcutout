import CutoutMobileFFI
import Foundation

public enum StorageMaintenanceResult: Equatable, Sendable {
    case completed(bytesBefore: UInt64, bytesAfter: UInt64, compressedEventCount: UInt64)
    case activeRide
    case activeCapture
    case pevcapImportInProgress
    case insufficientSpace(requiredBytes: UInt64, availableBytes: UInt64)
    case failed
}

/// Runs the explicit database maintenance command away from the main actor.
public enum StorageMaintenance {
    public static func optimize() async -> StorageMaintenanceResult {
        do {
            let database = try await RustPersistenceStore.open()
            return await Task.detached(priority: .userInitiated) {
                switch database.optimizeStorage() {
                case let .completed(report):
                    .completed(
                        bytesBefore: report.bytesBefore,
                        bytesAfter: report.bytesAfter,
                        compressedEventCount: report.compressedEventCount
                    )
                case .activeRide:
                    .activeRide
                case .activeCapture:
                    .activeCapture
                case .pevcapImportInProgress:
                    .pevcapImportInProgress
                case let .insufficientSpace(requiredBytes, availableBytes):
                    .insufficientSpace(requiredBytes: requiredBytes, availableBytes: availableBytes)
                case .failed:
                    .failed
                }
            }.value
        } catch {
            return .failed
        }
    }
}
