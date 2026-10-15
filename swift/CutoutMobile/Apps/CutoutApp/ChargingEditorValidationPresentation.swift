import CutoutMobileFFI

enum ChargingEditorValidationPresentation {
    static func messageKey(for issue: MobileChargingFieldIssueDto) -> String {
        switch issue.field {
        case .coordinate:
            "charging.editor.error.coordinate"
        case .accessNote:
            switch issue.reason {
            case .required: "charging.editor.error.access_note_blank"
            case .tooLong: "charging.editor.error.access_note_too_long"
            case .controlCharacter: "charging.editor.error.access_note_control_character"
            default: "charging.editor.error.metadata"
            }
        case .name:
            "charging.editor.error.name"
        case .hours:
            "charging.editor.error.hours"
        case .payment:
            "charging.editor.error.price"
        case .supply:
            switch (issue.reason, issue.supplyIndex) {
            case (.tooMany, nil): "charging.editor.error.supplies"
            case (.required, _): "charging.editor.error.supply_confirmation"
            default: "charging.editor.error.connectors"
            }
        }
    }
}
