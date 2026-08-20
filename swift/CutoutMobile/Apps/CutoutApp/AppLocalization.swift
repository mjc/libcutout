import CutoutMobile
import Foundation

#if SWIFT_PACKAGE
let appLocalizationBundle = Bundle.module
#else
let appLocalizationBundle = Bundle.main
#endif

func localizedAppText(_ key: String, _ arguments: CVarArg...) -> String {
    localizedAppText(key, arguments: arguments, bundle: appLocalizationBundle, locale: .current)
}

func localizedAppText(
    _ key: String,
    arguments: [CVarArg],
    bundle: Bundle,
    locale: Locale
) -> String {
    let format = bundle.localizedString(forKey: key, value: nil, table: "Localizable")
    if format == key {
        return pevLocalizedText(key, arguments: arguments)
    }
    return String(
        format: format,
        locale: locale,
        arguments: arguments
    )
}
