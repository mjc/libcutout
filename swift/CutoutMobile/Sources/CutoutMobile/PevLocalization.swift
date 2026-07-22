import Foundation

func pevLocalizedText(_ key: String, _ arguments: CVarArg...) -> String {
    String(
        format: Bundle.module.localizedString(forKey: key, value: nil, table: "Localizable"),
        locale: .current,
        arguments: arguments
    )
}
