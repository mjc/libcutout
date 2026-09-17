import CutoutMobileFFI

// Native views render the shared semantic catalog and immutable owner snapshots.
public typealias DeviceControlsSnapshot = CutoutMobileFFI.MobileDeviceControlsSnapshotDto
public typealias DeviceSettingID = CutoutMobileFFI.MobileSettingIdDto
public typealias DeviceSettingValue = CutoutMobileFFI.MobileSettingValueDto
public typealias DeviceSettingDescriptor = CutoutMobileFFI.MobileSettingDescriptorDto
public typealias DeviceSettingSnapshot = CutoutMobileFFI.MobileSettingSnapshotDto
public typealias DeviceSettingsSnapshot = CutoutMobileFFI.MobileDeviceSettingsSnapshotDto
public typealias DeviceSettingsDescriptorSnapshot = CutoutMobileFFI.MobileSettingsDescriptorSnapshotDto
public typealias DeviceSettingSubmissionError = CutoutMobileFFI.MobileDeviceSettingRequestError
public typealias DeviceSettingControl = CutoutMobileFFI.MobileSettingControlDto
public typealias DeviceSettingChoice = CutoutMobileFFI.MobileSettingChoiceDto
public typealias DeviceSettingGroup = CutoutMobileFFI.MobileSettingGroupDto
public typealias DeviceSettingAccess = CutoutMobileFFI.MobileSettingAccessDto
public typealias DeviceSettingStatus = CutoutMobileFFI.MobileSettingStatusDto
public typealias DeviceSettingEvidence = CutoutMobileFFI.MobileSettingEvidenceDto
public typealias DeviceSettingUnit = CutoutMobileFFI.MobileSettingUnitDto

public typealias DeviceActionID = CutoutMobileFFI.MobileDeviceActionIdDto
public typealias DeviceActionDescriptor = CutoutMobileFFI.MobileDeviceActionDescriptorDto
public typealias DeviceActionDescriptors = CutoutMobileFFI.MobileDeviceActionDescriptorsDto
public typealias DeviceActionSnapshot = CutoutMobileFFI.MobileDeviceActionSnapshotDto
public typealias DeviceActionsSnapshot = CutoutMobileFFI.MobileDeviceActionsSnapshotDto
public typealias DeviceActionProgress = CutoutMobileFFI.MobileDeviceActionProgressDto
public typealias DeviceActionStatus = CutoutMobileFFI.MobileDeviceActionStatusDto
public typealias DeviceActionNextStep = CutoutMobileFFI.MobileDeviceActionNextStepDto
public typealias DeviceActionStep = CutoutMobileFFI.MobileDeviceActionStepDto
public typealias DeviceActionSubmissionError = CutoutMobileFFI.MobileDeviceActionSubmissionError

public extension DeviceSettingsDescriptorSnapshot {
    /// Returns the descriptor for one semantic setting, if this profile exposes it.
    func descriptor(for id: DeviceSettingID) -> DeviceSettingDescriptor? {
        descriptors.first { $0.id == id }
    }
}

public extension DeviceSettingsSnapshot {
    /// Returns the lifecycle snapshot for one semantic setting, if it exists.
    func setting(for id: DeviceSettingID) -> DeviceSettingSnapshot? {
        settings.first { $0.id == id }
    }
}

public extension DeviceControlsSnapshot {
    /// Returns the descriptor for one semantic setting, if this profile exposes it.
    func descriptor(for id: DeviceSettingID) -> DeviceSettingDescriptor? {
        settingDescriptors.first { $0.id == id }
    }

    /// Returns the lifecycle snapshot for one semantic setting, if it exists.
    func setting(for id: DeviceSettingID) -> DeviceSettingSnapshot? {
        settings.first { $0.id == id }
    }
}
