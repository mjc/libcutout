import CutoutMobileFFI
import SwiftUI

struct ChargingCommunityStatusView: View {
    @Bindable var model: ChargingMapModel
    let onReview: () -> Void

    private var canRetry: Bool {
        #if DEBUG
            if CutoutUITestChargingFixture.isEnabled { return false }
        #endif
        return model.communitySyncStatus.canRetry
    }

    var body: some View {
        VStack(spacing: 6) {
            HStack(spacing: 8) {
                if model.communitySyncStatus == .syncing { ProgressView().controlSize(.small) }
                Text(localizedAppText(model.communitySyncStatus.messageKey))
                if canRetry {
                    Button(localizedAppText("charging.retry")) { model.retryCommunitySync() }
                        .frame(minHeight: 44)
                        .accessibilityIdentifier("charging.sync.retry")
                }
            }
            if model.communitySyncPendingCount > 0 {
                LabeledContent(
                    localizedAppText("charging.sync.pending_count"), value: model.communitySyncPendingCount.formatted())
            }
            if let date = model.communitySyncLastUpdated {
                LabeledContent(localizedAppText("charging.sync.updated")) {
                    Text(date, format: .dateTime.month(.abbreviated).day().hour().minute())
                }
            }
            if !model.blockedChargingOperations.isEmpty || model.blockedOperationsError != nil
                || model.communitySyncStatus.needsReview
            {
                Button(localizedAppText("charging.sync.review"), action: onReview)
                    .frame(minHeight: 44)
                    .accessibilityIdentifier("charging.sync.review")
            }
        }
        .font(.caption)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("charging.sync.status")
    }
}

extension ChargingCommunitySyncStatus {
    var messageKey: String {
        switch self {
        case .notStarted: "charging.status.local_data"
        case .syncing: "charging.sync.updating"
        case .synced: "charging.sync.current"
        case .pending: "charging.sync.pending"
        case .offline: "charging.sync.offline"
        case .authenticationRequired: "charging.sync.unauthorized"
        case .conflict: "charging.sync.conflict"
        case .rejected: "charging.sync.rejected"
        case .throttled: "charging.sync.throttled"
        case .storageUnavailable: "charging.status.storage_failed"
        case .registrationUncertain: "charging.sync.registration_interrupted"
        }
    }

    var canRetry: Bool {
        switch self {
        case .syncing, .synced, .conflict, .rejected: false
        case .notStarted, .pending, .offline, .authenticationRequired, .throttled, .storageUnavailable,
            .registrationUncertain:
            true
        }
    }

    var needsReview: Bool { self == .conflict || self == .rejected }
}

struct ChargingSyncIssuesView: View {
    @Bindable var model: ChargingMapModel
    let onEdit: @MainActor (MobileChargingSyncConflictDto) async throws -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var resolvingID: String?
    @State private var pendingDiscard: MobileChargingSyncConflictDto?
    @State private var resolutionError: MobileChargingError?

    var body: some View {
        NavigationStack {
            List {
                if let error = resolutionError ?? model.blockedOperationsError {
                    Text(localizedAppText(ChargingSpotPresentation.loadErrorKey(error)))
                        .foregroundStyle(.red)
                }
                if model.blockedChargingOperations.isEmpty {
                    Text(localizedAppText("charging.sync.no_issues"))
                }
                ForEach(model.blockedChargingOperations, id: \.operationId) { issue in
                    Section(issue.input?.name ?? ChargingSpotPresentation.unknownName) {
                        Text(
                            localizedAppText(
                                issue.reason == .invalid ? "charging.sync.rejected" : "charging.sync.conflict"))
                        Text(localizedAppText(issue.action.messageKey)).font(.headline)
                        if issue.action.editsFields, let input = issue.input {
                            draftSummary(input, title: "charging.sync.your_changes")
                        }
                        if let input = issue.remoteInput {
                            draftSummary(input, title: "charging.sync.community_version")
                        } else if let status = issue.remoteStatus {
                            Text(localizedAppText(ChargingSpotPresentation.statusKey(status)))
                        }
                        if issue.reason == .invalid, issue.action.editsFields, issue.input != nil {
                            Button(localizedAppText("charging.editor.edit")) { edit(issue) }
                                .accessibilityIdentifier("charging.sync.edit-draft")
                        }
                        if issue.canRebase {
                            Button(localizedAppText("charging.sync.keep_changes")) {
                                resolve(issue, with: .rebaseKeepingLocal)
                            }
                            .accessibilityIdentifier("charging.sync.keep-changes")
                        }
                        Button(
                            localizedAppText(
                                issue.remoteStatus == nil ? "charging.sync.discard" : "charging.sync.use_community"),
                            role: .destructive
                        ) {
                            pendingDiscard = issue
                        }
                        .accessibilityIdentifier("charging.sync.use-community")
                        if resolvingID == issue.operationId { ProgressView() }
                    }
                }
            }
            .textSelection(.enabled)
            .disabled(resolvingID != nil)
            .navigationTitle(localizedAppText("charging.sync.review"))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(localizedAppText("ride.readings.done")) { dismiss() }
                        .disabled(resolvingID != nil)
                }
            }
            .task { await model.loadBlockedOperations() }
        }
        .confirmationDialog(
            localizedAppText("charging.sync.discard_confirm"),
            isPresented: Binding(get: { pendingDiscard != nil }, set: { if !$0 { pendingDiscard = nil } })
        ) {
            if let issue = pendingDiscard {
                Button(localizedAppText("charging.sync.discard"), role: .destructive) {
                    pendingDiscard = nil
                    resolve(issue, with: .acceptServer)
                }
            }
        }
        .interactiveDismissDisabled(resolvingID != nil)
    }

    private func edit(_ issue: MobileChargingSyncConflictDto) {
        guard resolvingID == nil else { return }
        resolvingID = issue.operationId
        resolutionError = nil
        Task {
            defer { resolvingID = nil }
            do {
                try await onEdit(issue)
            } catch let error as MobileChargingError {
                resolutionError = error
            } catch {
                resolutionError = .StorageFailure
            }
        }
    }

    private func resolve(_ issue: MobileChargingSyncConflictDto, with resolution: MobileChargingResolutionDto) {
        guard resolvingID == nil else { return }
        resolvingID = issue.operationId
        resolutionError = nil
        Task {
            defer { resolvingID = nil }
            do {
                try await model.resolveChargingOperation(issue, resolution: resolution)
            } catch let error as MobileChargingError {
                resolutionError = error
            } catch {
                resolutionError = .StorageFailure
            }
        }
    }

    private func draftSummary(_ input: MobileChargingInputDto, title: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(localizedAppText(title)).font(.headline)
            Text(input.accessNote)
            Text(localizedAppText(input.cost == .free ? "charging.cost.free" : "charging.cost.paid"))
            Text("\(input.coordinate.latitudeDegrees), \(input.coordinate.longitudeDegrees)")
            if let venue = input.venue { Text(ChargingSpotPresentation.venueName(venue)) }
            if let hours = input.hours { Text(hours) }
            if let terms = input.terms { Text(ChargingSpotPresentation.termsName(terms)) }
            if let price = input.price { Text(ChargingSpotPresentation.priceText(price)) }
            ForEach(Array(input.supplies.enumerated()), id: \.offset) { _, supply in
                Text(ChargingSpotPresentation.supplyText(supply))
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}

extension ChargingSpotPresentation {
    static func statusKey(_ status: MobileChargingStatusDto) -> String {
        switch status {
        case .queued: "charging.spot.status.pending"
        case .published: "charging.status.published"
        case .hidden: "charging.status.hidden"
        case .withdrawn: "charging.status.withdrawn"
        case .removed: "charging.status.removed"
        }
    }
}

extension MobileChargingPendingActionDto {
    var editsFields: Bool {
        switch self {
        case .create, .edit: true
        case .withdraw, .verify, .vote: false
        }
    }

    var messageKey: String {
        switch self {
        case .create: "charging.editor.title"
        case .edit: "charging.editor.edit_title"
        case .withdraw: "charging.editor.delete"
        case .verify: "charging.sync.verify"
        case .vote(selection: .up): "charging.vote.up"
        case .vote(selection: .down): "charging.vote.down"
        case .vote(selection: nil): "charging.sync.clear_vote"
        }
    }
}
