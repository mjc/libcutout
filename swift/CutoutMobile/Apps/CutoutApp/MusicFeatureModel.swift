import CutoutMobile
import CutoutMobileFFI
import Observation

/// Application-retained presentation shared by the compact player and setup.
/// Provider lifecycle and Rust-backed history effects are being moved here with
/// their existing owners; this model does not duplicate either domain state.
@MainActor
@Observable
final class MusicFeatureModel {
    var settingsNowPlaying: MusicNowPlaying?
    var timelineEvents = [MobileMusicRideEventDto]()
    var selectedProvider: MobileMusicProviderDto
    var isPlayerHidden: Bool
    var historyPolicy: MobileMusicHistoryPolicyDto
    var historyUnavailable = false
    var historySaveError: MobileRideMapError?
    var commandFeedback: MusicCommandFeedback?

    var nowPlaying: MusicNowPlaying? {
        isPlayerHidden ? nil : settingsNowPlaying
    }

    var commandStatusText: String? {
        commandFeedback?.messageKey.map { pevLocalizedText($0) }
    }

    init(
        selectedProvider: MobileMusicProviderDto,
        isPlayerHidden: Bool,
        historyPolicy: MobileMusicHistoryPolicyDto
    ) {
        self.selectedProvider = selectedProvider
        self.isPlayerHidden = isPlayerHidden
        self.historyPolicy = historyPolicy
    }
}
