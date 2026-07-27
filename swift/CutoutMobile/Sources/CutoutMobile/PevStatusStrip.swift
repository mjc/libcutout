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
}

public struct PevStatusStrip: View {
    let text: String
    public let tone: PevStatusStripTone

    var accessibilityLabelText: String { text }

    public init(text: String, tone: PevStatusStripTone = .nominal) {
        self.text = text
        self.tone = tone
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
    }
}
