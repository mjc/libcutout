import CutoutMobile

enum CaptureStatus: Equatable {
    case recordingLocally(fileName: String)
    case recording(label: String?, notificationCount: Int, fileName: String?)
    case labelStarted(label: String, notificationCount: Int, fileName: String?)
    case labelStopped(label: String, notificationCount: Int, fileName: String?)
    case saved(fileName: String)
    case stored
    case failed

    var isRecording: Bool {
        switch self {
        case .recordingLocally, .recording, .labelStarted, .labelStopped:
            true
        case .saved, .stored, .failed:
            false
        }
    }

    var displayText: String {
        switch self {
        case .recordingLocally(let fileName):
            localizedAppText("capture.status.recording_locally", fileName)
        case .recording(let label, let notificationCount, let fileName):
            captureProgressText(label: label, notificationCount: notificationCount, fileName: fileName)
        case .labelStarted(let label, let notificationCount, let fileName):
            captureLabelText(label: label, action: "started", notificationCount: notificationCount, fileName: fileName)
        case .labelStopped(let label, let notificationCount, let fileName):
            captureLabelText(label: label, action: "stopped", notificationCount: notificationCount, fileName: fileName)
        case .saved(let fileName):
            localizedAppText("capture.status.saved", fileName)
        case .stored:
            localizedAppText("capture.status.stored")
        case .failed:
            localizedAppText("capture.announcement.failed")
        }
    }

    var rideDisplayText: String {
        switch self {
        case .recordingLocally, .recording, .labelStarted, .labelStopped:
            localizedAppText("capture.status.ride_recording")
        case .saved, .stored:
            localizedAppText("capture.status.ride_saved")
        case .failed:
            localizedAppText("capture.announcement.failed")
        }
    }

    var accessibilityAnnouncement: String? {
        switch self {
        case .labelStarted(let label, _, _):
            localizedAppText("capture.announcement.label_started", label)
        case .labelStopped(let label, _, _):
            localizedAppText("capture.announcement.label_stopped", label)
        case .saved, .stored:
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
        case (.some(let label), .some(let fileName)):
            localizedAppText("capture.status.recording_labeled_file", label, Int64(notificationCount), fileName)
        case (.some(let label), nil):
            localizedAppText("capture.status.recording_labeled", label, Int64(notificationCount))
        case (nil, .some(let fileName)):
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
        let key =
            switch (action, fileName) {
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
        case .identified(let selection), .connecting(let selection, _), .retrying(let selection, _),
            .connected(let selection), .failed(let selection, _):
            selection
        }
    }

    func replacingSelection(with selection: ConnectionSelection) -> Self {
        switch self {
        case .picker, .identified:
            .identified(selection)
        case .connecting(_, let phase):
            .connecting(selection, phase: phase)
        case .retrying(_, let retry):
            .retrying(selection, retry: retry)
        case .connected:
            .connected(selection)
        case .failed(_, let failure):
            .failed(selection, failure)
        }
    }

    var statusText: String? {
        switch self {
        case .picker, .identified:
            nil
        case .connecting(_, let phase):
            phase.displayText
        case .retrying:
            localizedAppText("picker.status.retrying")
        case .connected:
            SessionConnectionPhase.live.displayText
        case .failed(_, let failure):
            failure.displayText
        }
    }

    func navigationIntent(isRecordOnlyCapture: Bool) -> PhaseNavigationIntent {
        guard !isRecordOnlyCapture else { return .openCapture }

        switch self {
        case .connected(let selection):
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
    case openCapture
    case openRide(DevicePickerConnectionRoute)
    case returnToPicker
}
