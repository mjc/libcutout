import SwiftUI

public enum PevStatusStripTone: Sendable, Equatable {
    case nominal
    case critical

    var indicatorColor: Color {
        switch self {
        case .nominal: PevDashboardColors.nominal
        case .critical: PevDashboardColors.red
        }
    }

    var accessibilityValueText: String {
        switch self {
        case .nominal: ""
        case .critical: pevLocalizedText("status.accessibility.critical")
        }
    }
}

public struct PevStatusStrip: View {
    let text: String
    public let tone: PevStatusStripTone
    public let accessibilityIdentifier: String

    var accessibilityLabelText: String { text }
    var accessibilityValueText: String { tone.accessibilityValueText }

    public init(
        text: String,
        tone: PevStatusStripTone = .nominal,
        accessibilityIdentifier: String = ""
    ) {
        self.text = text
        self.tone = tone
        self.accessibilityIdentifier = accessibilityIdentifier
    }

    public var body: some View {
        HStack(spacing: 10) {
            Circle()
                .fill(tone.indicatorColor)
                .frame(width: 10, height: 10)
                .accessibilityHidden(true)
            Text(text)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(PevDashboardColors.primaryText)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16)
        .frame(minHeight: 42)
        .frame(maxWidth: .infinity)
        .background(
            PevDashboardCardBackground(cornerRadius: 18)
        )
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityLabelText)
        .accessibilityValue(accessibilityValueText)
        .accessibilityIdentifier(accessibilityIdentifier)
    }
}
