import SwiftUI

public struct PevDashboardGrid<Content: View>: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private let layout: Layout
    private let spacing: CGFloat
    private let content: Content

    public init(
        columns: [GridItem],
        spacing: CGFloat,
        @ViewBuilder content: () -> Content
    ) {
        layout = .fixed(columns)
        self.spacing = spacing
        self.content = content()
    }

    public init(
        adaptiveMinimumColumnWidth: CGFloat = 150,
        accessibilityMinimumColumnWidth: CGFloat = 240,
        columnSpacing: CGFloat = 26,
        spacing: CGFloat,
        @ViewBuilder content: () -> Content
    ) {
        layout = .adaptive(
            defaultMinimumColumnWidth: adaptiveMinimumColumnWidth,
            accessibilityMinimumColumnWidth: accessibilityMinimumColumnWidth,
            columnSpacing: columnSpacing
        )
        self.spacing = spacing
        self.content = content()
    }

    public var body: some View {
        LazyVGrid(columns: resolvedColumns, spacing: spacing) {
            content
        }
    }

    nonisolated static func adaptiveMinimumColumnWidth(
        for dynamicTypeSize: DynamicTypeSize,
        default defaultMinimumColumnWidth: CGFloat,
        accessibility accessibilityMinimumColumnWidth: CGFloat
    ) -> CGFloat {
        dynamicTypeSize.isAccessibilitySize
            ? max(defaultMinimumColumnWidth, accessibilityMinimumColumnWidth)
            : defaultMinimumColumnWidth
    }

    private var resolvedColumns: [GridItem] {
        switch layout {
        case let .fixed(columns):
            columns
        case let .adaptive(
            defaultMinimumColumnWidth,
            accessibilityMinimumColumnWidth,
            columnSpacing
        ):
            [
                GridItem(
                    .adaptive(
                        minimum: Self.adaptiveMinimumColumnWidth(
                            for: dynamicTypeSize,
                            default: defaultMinimumColumnWidth,
                            accessibility: accessibilityMinimumColumnWidth
                        )
                    ),
                    spacing: columnSpacing
                ),
            ]
        }
    }

    private enum Layout {
        case fixed([GridItem])
        case adaptive(
            defaultMinimumColumnWidth: CGFloat,
            accessibilityMinimumColumnWidth: CGFloat,
            columnSpacing: CGFloat
        )
    }
}
