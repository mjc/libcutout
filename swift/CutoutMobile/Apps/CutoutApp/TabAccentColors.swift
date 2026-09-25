#if os(iOS)
    import UIKit

    // UIKit can resolve these providers on SwiftUI's background renderer.
    enum TabAccentColors {
        nonisolated static let purple = UIColor { traits in
            traits.userInterfaceStyle == .dark
                ? .systemPurple
                : UIColor(red: 0.34, green: 0.08, blue: 0.52, alpha: 1)
        }

        nonisolated static let yellow = UIColor { traits in
            traits.userInterfaceStyle == .dark
                ? .systemYellow
                : UIColor(red: 0.45, green: 0.25, blue: 0.0, alpha: 1)
        }
    }
#endif
