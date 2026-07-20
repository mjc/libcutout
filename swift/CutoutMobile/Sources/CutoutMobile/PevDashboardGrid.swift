import SwiftUI

public struct PevDashboardGrid<Content: View>: View {
    private let columns: [GridItem]
    private let spacing: CGFloat
    private let content: Content

    public init(
        columns: [GridItem],
        spacing: CGFloat,
        @ViewBuilder content: () -> Content
    ) {
        self.columns = columns
        self.spacing = spacing
        self.content = content()
    }

    public var body: some View {
        LazyVGrid(columns: columns, spacing: spacing) {
            content
        }
    }
}
