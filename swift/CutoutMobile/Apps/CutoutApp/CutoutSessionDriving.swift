import CutoutMobile
import CutoutMobileFFI

@MainActor
protocol CutoutSessionDriving: AnyObject {
    var rideSessionStateHandle: CutoutSessionStateHandle { get }
    /// The Rust-backed map adapter is optional while persistence is unavailable.
    var rideMapStateHandle: MobileRideMapState? { get }
    var onDisplayStateChange: ((RideDisplayState) -> Void)? { get set }
    var onPhaseChange: ((SessionConnectionPhase) -> Void)? { get set }
    var onReconnectScheduled: ((SessionConnectionRetry) -> Void)? { get set }
    var onCaptureEvent: ((CaptureEvent) -> Void)? { get set }
    var onScanStateChange: ((DevicePickerScanState) -> Void)? { get set }
    var onSettingsStateChange: ((EucSettingsState) -> Void)? { get set }
    var onSettingsReadbackChange: ((SettingsReadback?) -> Void)? { get set }
    var onFaultHistoryReadbackChange: ((FaultHistoryReadback?) -> Void)? { get set }
    var onBmsSnapshotChange: ((BmsSnapshot?) -> Void)? { get set }
    var onPhoneLocationSnapshotChange: ((MobilePhoneLocationSnapshotDto, MonotonicMilliseconds) -> Void)? { get set }
    var onRideMapDecisionChange: ((MobileRideMapSnapshotDto, MobileRideMapDecisionDto) -> Void)? { get set }
    var onRideMapSnapshotChange: ((MobileRideMapSnapshotDto) -> Void)? { get set }
    var onRideMapErrorChange: ((MobileRideMapError) -> Void)? { get set }
    var onRideMapAvailabilityChange: ((MobileRideMapAvailability) -> Void)? { get set }
    var onProtocolIdentityCandidateChange: ((DevicePickerDiscoveryCandidate?) -> Void)? { get set }
    var onBluetoothRestorationResolved: ((String?) -> Void)? { get set }
    var protocolIdentityCandidate: DevicePickerDiscoveryCandidate? { get }
    var electricUnicycleModel: ElectricUnicycleModel? { get }
    var settingsCapabilities: EucSettingsCapabilities? { get }
    var settingsState: EucSettingsState? { get }
    var tripMeterResetState: TripMeterResetState? { get }
    var headlightState: LightSettingState? { get }
    var headlightCommandStatus: LightCommandStatus? { get }
    var aeroHighBeamState: LightSettingState? { get }
    var aeroTiltbackSpeedState: AeroSpeedSettingState? { get }
    var aeroPwmPercentState: AeroPwmSettingState? { get }
    var aeroGyroCalibrationState: AeroGyroCalibrationSettingState? { get }
    var aeroRidingModeState: AeroRidingModeSettingState? { get }
    var aeroBrakeOverpressureAlarmState: AeroBrakeOverpressureAlarmSettingState? { get }
    var aeroPedalHardnessState: AeroPedalHardnessSettingState? { get }
    var aeroDisplayBacklightState: AeroDisplayBacklightSettingState? { get }
    var aeroWheelUnitsState: AeroWheelUnitsSettingState? { get }
    var aeroBeeperVolumeState: AeroBeeperVolumeSettingState? { get }
    var aeroDynamicAssistState: AeroDynamicAssistSettingState? { get }
    var aeroPedalDipCompensationState: AeroPedalDipCompensationSettingState? { get }
    var aeroLateralTiltLimitState: AeroLateralTiltLimitSettingState? { get }
    var aeroVoltageCorrectionState: AeroVoltageCorrectionSettingState? { get }
    var aeroMaxChargeVoltageRawState: AeroMaxChargeVoltageRawSettingState? { get }
    var aeroHighSpeedModeState: AeroToggleSettingState? { get }
    var aeroLowBatteryModeState: AeroToggleSettingState? { get }
    var aeroTransportModeState: AeroToggleSettingState? { get }
    var aeroAlarmSpeedState: AeroSpeedSettingState? { get }
    var aeroAngleAdjustmentState: AeroAngleAdjustmentSettingState? { get }
    var pedalModeState: PedalModeSettingState? { get }
    var rollAngleState: RollAngleSettingState? { get }
    var speedAlarmModeState: SpeedAlarmModeSettingState? { get }
    var accelerationAssistState: AccelerationAssistSettingState? { get }
    var taillightState: LightSettingState? { get }

    func start()
    func pair(platformIdentifier: String) -> Bool
    func pair(platformIdentifier: String, model: ElectricUnicycleModel) -> Bool
    func probe(platformIdentifier: String) -> Bool
    func recordOnly(platformIdentifier: String, note: String?, annotations: [String]) -> Bool
    func annotateCapture(label: String)
    func annotateCapture(key: String, value: String)
    func updateMusicCapturePolicy(_ policy: MobileMusicHistoryPolicyDto)
    func updateMusicCaptureObservation(_ observation: MobilePevcapMusicEventDto?)
    func flushCapture() async -> Bool
    func disconnectAndScan()
    func setLights(_ state: LightState) -> SettingCommandResult
    func setAeroHighBeam(_ state: LightState) -> SettingCommandResult
    func setPedalMode(_ mode: PedalMode.Kind) -> SettingCommandResult
    func setRollAngle(_ angle: RollAngle.Kind) -> SettingCommandResult
    func setSpeedAlarmMode(_ mode: SpeedAlarmMode.Kind) -> SettingCommandResult
    func setBegodeMaxSpeed(_ speed: BegodeMaxSpeed) -> SettingCommandResult
    func setBegodeBeeperVolume(_ volume: BegodeBeeperVolume) -> SettingCommandResult
    func setBegodeLedMode(_ mode: BegodeLedMode) -> SettingCommandResult
    func resetTripMeter() -> SettingCommandResult
    func setAeroTiltbackSpeed(_ speed: AeroSpeedSetting) -> SettingCommandResult
    func setAeroPwmPercent(_ percent: AeroPwmPercent) -> SettingCommandResult
    func setAeroPwmOff() -> SettingCommandResult
    func setAeroGyroCalibration() -> SettingCommandResult
    func setAeroRidingMode(_ mode: AeroRidingMode) -> SettingCommandResult
    func setAeroBrakeOverpressureAlarm(_ value: AeroBrakeOverpressureAlarm) -> SettingCommandResult
    func setAeroPedalHardness(_ hardness: AeroPedalHardness) -> SettingCommandResult
    func setAeroDisplayBacklight(_ value: AeroDisplayBacklight) -> SettingCommandResult
    func setAeroWheelUnits(_ value: AeroWheelUnits) -> SettingCommandResult
    func setAeroBeeperVolume(_ value: AeroBeeperVolume) -> SettingCommandResult
    func setAeroDynamicAssist(_ value: AeroDynamicAssist) -> SettingCommandResult
    func setAeroPedalDipCompensation(_ value: AeroPedalDipCompensation) -> SettingCommandResult
    func setAeroLateralTiltLimit(_ value: AeroLateralTiltLimit) -> SettingCommandResult
    func setAeroVoltageCorrection(_ value: AeroVoltageCorrection) -> SettingCommandResult
    func setAeroMaxChargeVoltageRaw(_ value: AeroMaxChargeVoltageRaw) -> SettingCommandResult
    func setAeroHighSpeedMode(_ value: AeroToggle) -> SettingCommandResult
    func setAeroLowBatteryMode(_ value: AeroToggle) -> SettingCommandResult
    func setAeroTransportMode(_ value: AeroToggle) -> SettingCommandResult
    func setAeroAlarmSpeed(_ speed: AeroSpeedSetting) -> SettingCommandResult
    func setAeroAngleAdjustment(_ angle: AeroAngleAdjustment) -> SettingCommandResult
    func now() -> MonotonicMilliseconds

    func resetRideMapLocationAdmission()
    func startRideMapGpsOnly(atMs: UInt64, lastConnectedVehicle: String?) throws -> MobileRideMapSnapshotDto
    func pauseRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func resumeRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func stopRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto
    func saveRideMap() throws -> MobileRideMapSnapshotDto
    func discardRideMap() throws -> MobileRideMapSnapshotDto
}

extension CutoutSessionCore: CutoutSessionDriving {}

extension CutoutSessionDriving {
    var settingsState: EucSettingsState? { nil }

    func setAeroPwmOff() -> SettingCommandResult { .failed }
    func setAeroGyroCalibration() -> SettingCommandResult { .failed }
    func setAeroMaxChargeVoltageRaw(_ value: AeroMaxChargeVoltageRaw) -> SettingCommandResult { .failed }

    var aeroGyroCalibrationState: AeroGyroCalibrationSettingState? { nil }
    var aeroRidingModeState: AeroRidingModeSettingState? { nil }
    var aeroBrakeOverpressureAlarmState: AeroBrakeOverpressureAlarmSettingState? { nil }
    var aeroMaxChargeVoltageRawState: AeroMaxChargeVoltageRawSettingState? { nil }

    func setAeroRidingMode(_ mode: AeroRidingMode) -> SettingCommandResult { .failed }
    func setAeroBrakeOverpressureAlarm(_ value: AeroBrakeOverpressureAlarm) -> SettingCommandResult { .failed }
    var aeroHighSpeedModeState: AeroToggleSettingState? { nil }
    var aeroLowBatteryModeState: AeroToggleSettingState? { nil }
    var aeroTransportModeState: AeroToggleSettingState? { nil }

    func setAeroHighSpeedMode(_ value: AeroToggle) -> SettingCommandResult { .failed }
    func setAeroLowBatteryMode(_ value: AeroToggle) -> SettingCommandResult { .failed }
    func setAeroTransportMode(_ value: AeroToggle) -> SettingCommandResult { .failed }

    var rideMapStateHandle: MobileRideMapState? { nil }
    var aeroHighBeamState: LightSettingState? { nil }
    var aeroTiltbackSpeedState: AeroSpeedSettingState? { nil }
    var aeroPwmPercentState: AeroPwmSettingState? { nil }
    var aeroAlarmSpeedState: AeroSpeedSettingState? { nil }
    var aeroAngleAdjustmentState: AeroAngleAdjustmentSettingState? { nil }

    var rideMapStorageError: String? {
        guard let state = rideMapStateHandle else { return "Rust ride database is unavailable" }
        guard case let .storageError(message)? = state.initializationError else { return nil }
        return message
    }

    var rideMapAvailability: MobileRideMapAvailability {
        rideMapStorageError == nil ? .ready : .storageUnavailable
    }

    private func requireRideMapState() throws -> MobileRideMapState {
        guard let state = rideMapStateHandle else {
            throw MobileRideMapError.storageError("Rust ride database is unavailable")
        }
        return state
    }

    func resetRideMapLocationAdmission() {}

    func startRideMapGpsOnly(atMs: UInt64, lastConnectedVehicle: String?) throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().startGpsOnly(atMs: atMs, lastConnectedVehicle: lastConnectedVehicle)
    }

    func pauseRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().pause(atMs: atMs)
    }

    func resumeRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().resume(atMs: atMs)
    }

    func stopRideMap(atMs: UInt64) throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().stop(atMs: atMs)
    }

    func saveRideMap() throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().save()
    }

    func discardRideMap() throws -> MobileRideMapSnapshotDto {
        try requireRideMapState().discard()
    }
}
