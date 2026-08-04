import Foundation

public func pevLocalizedText(_ key: String, _ arguments: CVarArg...) -> String {
    pevLocalizedText(key, arguments: arguments)
}

public func pevLocalizedText(_ key: String, arguments: [CVarArg]) -> String {
    pevLocalizedText(key, arguments: arguments, bundle: .module, locale: .current)
}

func pevLocalizedText(
    _ key: String,
    arguments: [CVarArg],
    bundle: Bundle,
    locale: Locale
) -> String {
    String(
        format: bundle.localizedString(forKey: key, value: nil, table: "Localizable"),
        locale: locale,
        arguments: arguments
    )
}
