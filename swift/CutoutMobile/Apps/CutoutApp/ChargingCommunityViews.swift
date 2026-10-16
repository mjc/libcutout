import CutoutMobileFFI
import Foundation
import Observation
import SwiftUI

@MainActor
@Observable
final class ChargingContentReportModel {
    typealias Submit = @MainActor (MobileChargingContentReport) async throws -> MobileChargingReportReceiptDto

    enum State: Equatable {
        case editing
        case submitting
        case sent(String)
        case failed(String)
    }

    var details = ""
    private(set) var state: State = .editing
    private(set) var preparedReport: MobileChargingContentReport?
    private let remoteID: String
    private let submit: Submit

    init(remoteID: String, submit: @escaping Submit = submitToCommunity) {
        self.remoteID = remoteID
        self.submit = submit
    }

    static func submitToCommunity(_ report: MobileChargingContentReport) async throws -> MobileChargingReportReceiptDto
    {
        let credential = try await ChargingCommunitySync.shared.credentialForReport()
        return try await report.submit(token: credential.token)
    }

    func send() async {
        guard state != .submitting else { return }
        if case .sent = state { return }
        do {
            let report: MobileChargingContentReport
            if let preparedReport {
                report = preparedReport
            } else {
                report = try MobileChargingContentReport(spotId: remoteID, message: details)
                preparedReport = report
            }
            state = .submitting
            let receipt = try await submit(report)
            state = .sent(receipt.reportId)
        } catch let error as MobileChargingError {
            let key: String
            switch error {
            case .InvalidMetadata: key = "charging.report.invalid_details"
            case .Unauthorized: key = "charging.report.identity_required"
            case .Unavailable: key = "charging.report.unavailable"
            case .Throttled: key = "charging.report.throttled"
            default: key = "charging.report.failed"
            }
            state = .failed(key)
        } catch {
            state = .failed("charging.report.failed")
        }
    }
}

struct ChargingContentReportView: View {
    @State private var model: ChargingContentReportModel
    @Environment(\.dismiss) private var dismiss

    init(remoteID: String) {
        _model = State(initialValue: ChargingContentReportModel(remoteID: remoteID))
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(localizedAppText("charging.report.explanation"))
                    if case .sent(let reportID) = model.state {
                        Label(localizedAppText("charging.report.sent"), systemImage: "checkmark.circle.fill")
                            .accessibilityIdentifier("charging.report.sent")
                        LabeledContent(localizedAppText("charging.report.receipt"), value: reportID)
                    } else {
                        TextField(localizedAppText("charging.report.details"), text: $model.details, axis: .vertical)
                            .lineLimit(3...6)
                            .disabled(model.preparedReport != nil)
                            .accessibilityIdentifier("charging.report.details")
                        if case .failed(let key) = model.state {
                            Text(localizedAppText(key)).foregroundStyle(.red)
                                .accessibilityIdentifier("charging.report.error")
                        }
                        Button(
                            localizedAppText(model.preparedReport == nil ? "charging.report.send" : "charging.retry")
                        ) {
                            Task { await model.send() }
                        }
                        .disabled(model.state == .submitting)
                        .accessibilityIdentifier("charging.report.send")
                        if model.state == .submitting { ProgressView() }
                    }
                }
            }
            .navigationTitle(localizedAppText("charging.report.title"))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(localizedAppText("ride.readings.done")) { dismiss() }
                        .disabled(model.state == .submitting)
                }
            }
        }
        .interactiveDismissDisabled(model.state == .submitting)
        .accessibilityIdentifier("charging.report.screen")
    }
}

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
        if messageKey != nil || model.communitySyncPendingCount > 0 || needsReview {
            VStack(spacing: 6) {
                if let messageKey {
                    HStack(spacing: 8) {
                        Text(localizedAppText(messageKey))
                        if canRetry {
                            Button(localizedAppText("charging.retry")) { model.retryCommunitySync() }
                                .frame(minHeight: 44)
                                .accessibilityIdentifier("charging.sync.retry")
                        }
                    }
                }
                if model.communitySyncPendingCount > 0 {
                    LabeledContent(
                        localizedAppText("charging.sync.pending_count"),
                        value: model.communitySyncPendingCount.formatted())
                }
                if needsReview {
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

    private var messageKey: String? { model.communitySyncStatus.messageKey }

    private var needsReview: Bool {
        !model.blockedChargingOperations.isEmpty || model.blockedOperationsError != nil
            || model.communitySyncStatus.needsReview
    }
}

extension ChargingCommunitySyncStatus {
    var messageKey: String? {
        switch self {
        case .notStarted, .syncing, .synced, .deleted: nil
        case .pending: "charging.sync.pending"
        case .offline: "charging.sync.offline"
        case .authenticationRequired: "charging.sync.unauthorized"
        case .conflict: "charging.sync.conflict"
        case .rejected: "charging.sync.rejected"
        case .throttled: "charging.sync.throttled"
        case .storageUnavailable: "charging.status.storage_failed"
        case .registrationUncertain: "charging.sync.registration_interrupted"
        case .deletionPending: "charging.account.deletion_pending"
        }
    }

    var canRetry: Bool {
        switch self {
        case .syncing, .synced, .conflict, .rejected, .deletionPending, .deleted: false
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
                            ChargingInputSummary(input: input, title: "charging.sync.your_changes")
                        }
                        if let input = issue.remoteInput {
                            ChargingInputSummary(input: input, title: "charging.sync.community_version")
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

}

struct ChargingInputSummary: View {
    let input: MobileChargingInputDto
    let title: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if let title { Text(localizedAppText(title)).font(.headline) }
            LabeledContent(
                localizedAppText("charging.sync.name"),
                value: ChargingSpotPresentation.conflictName(input.name)
            )
            LabeledContent(localizedAppText("charging.editor.access_note"), value: input.accessNote)
            LabeledContent(
                localizedAppText("charging.editor.cost"),
                value: localizedAppText(input.cost == .free ? "charging.cost.free" : "charging.cost.paid")
            )
            LabeledContent(
                localizedAppText("charging.editor.latitude"),
                value: String(input.coordinate.latitudeDegrees)
            )
            LabeledContent(
                localizedAppText("charging.editor.longitude"),
                value: String(input.coordinate.longitudeDegrees)
            )
            LabeledContent(
                localizedAppText("charging.editor.venue"),
                value: input.venue.map(ChargingSpotPresentation.venueName)
                    ?? localizedAppText("charging.venue.unknown")
            )
            LabeledContent(
                localizedAppText("charging.editor.hours"),
                value: input.hours ?? localizedAppText("charging.editor.latest.none")
            )
            LabeledContent(
                localizedAppText("charging.editor.terms"),
                value: input.terms.map(ChargingSpotPresentation.termsName)
                    ?? localizedAppText("charging.editor.latest.none")
            )
            LabeledContent(
                localizedAppText("charging.editor.price.amount"),
                value: input.price.map(ChargingSpotPresentation.priceText)
                    ?? localizedAppText("charging.editor.latest.none")
            )
            LabeledContent(
                localizedAppText("charging.details.supplies"),
                value: input.supplies.isEmpty
                    ? localizedAppText("charging.editor.latest.none")
                    : input.supplies.map(ChargingSpotPresentation.supplyText).joined(separator: "; ")
            )
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}

extension ChargingSpotPresentation {
    enum SavedSelectionVisibility: Equatable {
        case none
        case page
        case separateAnnotation
        case feedback
    }

    static func savedSelectionVisibility(
        selectedID: String?, pageIDs: [String], hasValidInput: Bool,
        hasVisibleStatus: Bool, filtersAreEmpty: Bool
    ) -> SavedSelectionVisibility {
        guard let selectedID, hasValidInput, hasVisibleStatus else { return .none }
        if pageIDs.contains(selectedID) { return .page }
        return filtersAreEmpty ? .separateAnnotation : .feedback
    }

    static func voltageOptions(in supplies: [MobileChargingSupplyDto]) -> Set<UInt16> {
        Set(supplies.compactMap(\.voltageVolts))
    }

    static func conflictName(_ name: String?) -> String {
        guard let name, !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return unknownName
        }
        return name
    }

    static func statusKey(_ status: MobileChargingStatusDto, pendingOperations: UInt32? = nil) -> String {
        if status == .queued && pendingOperations == 0 { return "charging.spot.status.local_only" }
        return switch status {
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
