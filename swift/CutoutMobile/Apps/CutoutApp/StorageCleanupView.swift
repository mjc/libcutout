import CutoutMobile
import Observation
import SwiftUI

@MainActor
@Observable
final class StorageCleanupModel {
    enum State: Equatable {
        case idle
        case running
        case completed(bytesBefore: UInt64, bytesAfter: UInt64, compressedEventCount: UInt64)
        case failed(StorageMaintenanceFailure)
    }

    private(set) var state: State = .idle
    private let optimizeStorage: @MainActor () async -> StorageMaintenanceResult

    init(
        optimizeStorage: @escaping @MainActor () async -> StorageMaintenanceResult = {
            await StorageMaintenance.optimize()
        }
    ) {
        self.optimizeStorage = optimizeStorage
    }

    var isRunning: Bool {
        if case .running = state { return true }
        return false
    }

    var canDismiss: Bool { !isRunning }

    func optimize() async {
        guard !isRunning else { return }
        state = .running
        switch await optimizeStorage() {
        case let .completed(bytesBefore, bytesAfter, compressedEventCount):
            state = .completed(
                bytesBefore: bytesBefore,
                bytesAfter: bytesAfter,
                compressedEventCount: compressedEventCount
            )
        case .activeRide:
            state = .failed(.activeRide)
        case .activeCapture:
            state = .failed(.activeCapture)
        case .pevcapImportInProgress:
            state = .failed(.pevcapImportInProgress)
        case let .insufficientSpace(requiredBytes, availableBytes):
            state = .failed(
                .insufficientSpace(requiredBytes: requiredBytes, availableBytes: availableBytes)
            )
        case .failed:
            state = .failed(.storage)
        }
    }
}

enum StorageMaintenanceFailure: Equatable {
    case activeRide
    case activeCapture
    case pevcapImportInProgress
    case insufficientSpace(requiredBytes: UInt64, availableBytes: UInt64)
    case storage
}

struct StorageCleanupView: View {
    @State private var model: StorageCleanupModel

    init(model: StorageCleanupModel) {
        _model = State(initialValue: model)
    }

    var body: some View {
        Form {
            Section {
                Text(localizedAppText("storage_cleanup.description"))
                    .fixedSize(horizontal: false, vertical: true)
                Text(localizedAppText("storage_cleanup.keeps_data"))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Section {
                Button {
                    Task { await model.optimize() }
                } label: {
                    if model.isRunning {
                        HStack(spacing: 12) {
                            ProgressView()
                            Text(localizedAppText("storage_cleanup.running"))
                        }
                    } else {
                        Text(
                            model.state.isRetry
                                ? localizedAppText("storage_cleanup.retry")
                                : localizedAppText("storage_cleanup.action")
                        )
                    }
                }
                .disabled(model.isRunning)
                .accessibilityIdentifier("storage-cleanup.action")

                status
            }
        }
        .formStyle(.grouped)
        .navigationTitle(localizedAppText("storage_cleanup.title"))
        .navigationBarBackButtonHidden(model.isRunning)
    }

    @ViewBuilder
    private var status: some View {
        switch model.state {
        case .idle:
            EmptyView()
        case .running:
            Text(localizedAppText("storage_cleanup.running_detail"))
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("storage-cleanup.status")
        case let .completed(bytesBefore, bytesAfter, _):
            Text(
                localizedAppText(
                    "storage_cleanup.completed_sizes",
                    Self.fileSize(bytesBefore),
                    Self.fileSize(bytesAfter)
                )
            )
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityIdentifier("storage-cleanup.status")
        case let .failed(failure):
            Text(failureText(failure))
                .foregroundStyle(.red)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier("storage-cleanup.status")
        }
    }

    private func failureText(_ failure: StorageMaintenanceFailure) -> String {
        switch failure {
        case .activeRide:
            localizedAppText("storage_cleanup.active_ride")
        case .activeCapture:
            localizedAppText("storage_cleanup.active_capture")
        case .pevcapImportInProgress:
            localizedAppText("storage_cleanup.import_in_progress")
        case let .insufficientSpace(requiredBytes, availableBytes):
            localizedAppText(
                "storage_cleanup.insufficient_space_detail",
                Self.fileSize(requiredBytes),
                Self.fileSize(availableBytes)
            )
        case .storage:
            localizedAppText("storage_cleanup.failed")
        }
    }

    private static func fileSize(_ bytes: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    }
}

extension StorageCleanupModel.State {
    fileprivate var isRetry: Bool {
        if case .failed = self { return true }
        return false
    }
}
