import CutoutMobile

enum CaptureStatus: Equatable {
    case recordingLocally(fileName: String)
    case recording(label: String?, notificationCount: Int, fileName: String?)
    case labelStarted(label: String, notificationCount: Int, fileName: String?)
    case labelStopped(label: String, notificationCount: Int, fileName: String?)
    case saved(fileName: String)
    case failed

    var isRecording: Bool {
        switch self {
        case .recordingLocally, .recording, .labelStarted, .labelStopped:
            true
        case .saved, .failed:
            false
        }
    }

    var displayText: String {
        switch self {
        case let .recordingLocally(fileName):
            localizedAppText("capture.status.recording_locally", fileName)
        case let .recording(label, notificationCount, fileName):
            captureProgressText(label: label, notificationCount: notificationCount, fileName: fileName)
        case let .labelStarted(label, notificationCount, fileName):
            captureLabelText(label: label, action: "started", notificationCount: notificationCount, fileName: fileName)
        case let .labelStopped(label, notificationCount, fileName):
            captureLabelText(label: label, action: "stopped", notificationCount: notificationCount, fileName: fileName)
        case let .saved(fileName):
            localizedAppText("capture.status.saved", fileName)
        case .failed:
            localizedAppText("capture.announcement.failed")
        }
    }

    var accessibilityAnnouncement: String? {
        switch self {
        case let .labelStarted(label, _, _):
            localizedAppText("capture.announcement.label_started", label)
        case let .labelStopped(label, _, _):
            localizedAppText("capture.announcement.label_stopped", label)
        case .saved:
            localizedAppText("capture.announcement.saved")
        case .failed:
            localizedAppText("capture.announcement.failed")
        case .recordingLocally, .recording:
            nil
        }
    }

    var statusStripTone: PevStatusStripTone {
        switch self {
        case .failed: .critical
        default: .nominal
        }
    }

    private func captureProgressText(label: String?, notificationCount: Int, fileName: String?) -> String {
        switch (label, fileName) {
        case let (.some(label), .some(fileName)):
            localizedAppText("capture.status.recording_labeled_file", label, Int64(notificationCount), fileName)
        case let (.some(label), nil):
            localizedAppText("capture.status.recording_labeled", label, Int64(notificationCount))
        case let (nil, .some(fileName)):
            localizedAppText("capture.status.recording_file", Int64(notificationCount), fileName)
        case (nil, nil):
            localizedAppText("capture.status.recording", Int64(notificationCount))
        }
    }

    private func captureLabelText(
        label: String,
        action: String,
        notificationCount: Int,
        fileName: String?
    ) -> String {
        let key = switch (action, fileName) {
        case ("started", .some): "capture.status.label_started_file"
        case ("stopped", .some): "capture.status.label_stopped_file"
        case ("started", nil): "capture.status.label_started"
        default: "capture.status.label_stopped"
        }
        guard let fileName else { return localizedAppText(key, label) }
        return localizedAppText(key, label, Int64(notificationCount), fileName)
    }
}

struct ConnectionSelection: Equatable {
    let platformIdentifier: String
    let title: String
    let route: DevicePickerConnectionRoute
}

enum ConnectionState: Equatable {
    case picker
    case identified(ConnectionSelection)
    case connecting(ConnectionSelection, phase: SessionConnectionPhase)
    case retrying(ConnectionSelection, retry: SessionConnectionRetry)
    case connected(ConnectionSelection)
    case failed(ConnectionSelection, SessionConnectionFailure)

    var selection: ConnectionSelection? {
        switch self {
        case .picker:
            nil
        case let .identified(selection), let .connecting(selection, _), let .retrying(selection, _), let .connected(selection), let .failed(selection, _):
            selection
        }
    }

    var statusText: String? {
        switch self {
        case .picker, .identified:
            nil
        case let .connecting(_, phase):
            phase.displayText
        case .retrying:
            localizedAppText("picker.status.retrying")
        case .connected:
            SessionConnectionPhase.live.displayText
        case let .failed(_, failure):
            failure.displayText
        }
    }

    func navigationIntent(isRecordOnlyCapture: Bool) -> PhaseNavigationIntent {
        guard !isRecordOnlyCapture else { return .stay }

        switch self {
        case let .connected(selection):
            return .openRide(selection.route)
        case .picker, .failed:
            return .returnToPicker
        case .identified, .connecting, .retrying:
            return .stay
        }
    }
}

enum PhaseNavigationIntent: Equatable {
    case stay
    case openRide(DevicePickerConnectionRoute)
    case returnToPicker
}
