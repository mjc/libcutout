import Foundation

public struct DevicePickerSelectionStore {
    private static let key = "io.cutout.devicePicker.selectedPlatformIdentifier"
    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var platformIdentifier: String? {
        defaults.string(forKey: Self.key)
    }

    public func save(platformIdentifier: String) {
        let trimmed = platformIdentifier.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        defaults.set(trimmed, forKey: Self.key)
    }

    public func clear() {
        defaults.removeObject(forKey: Self.key)
    }
}
