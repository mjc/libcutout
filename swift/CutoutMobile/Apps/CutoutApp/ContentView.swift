import CutoutMobile
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: LiveSpeedModel
    @State private var selectedScreenID = MockupScreenCatalog.v2.screens.first?.id

    private let catalog = MockupScreenCatalog.v2

    var body: some View {
        NavigationSplitView {
            List(catalog.screens, selection: $selectedScreenID) { screen in
                VStack(alignment: .leading, spacing: 4) {
                    Text(screen.title)
                        .font(.headline)
                    Text(screen.subtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .tag(screen.id)
                .accessibilityLabel(screen.title)
            }
            .navigationTitle("Mockups")
        } detail: {
            if let screen = selectedScreen {
                MockupScreenView(screen: screen, liveSpeed: model.speed.displayValue)
            }
        }
    }

    private var selectedScreen: MockupScreen? {
        catalog.screens.first { $0.id == selectedScreenID } ?? catalog.screens.first
    }
}

private struct MockupScreenView: View {
    let screen: MockupScreen
    let liveSpeed: String

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text(screen.title)
                            .font(.title.weight(.semibold))
                        Spacer()
                        Text("fixture")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                    }

                    Text(screen.subtitle)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }

                VStack(alignment: .leading, spacing: 8) {
                    Text(screen.primaryValue)
                        .font(.system(size: 64, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.5)
                    Text(screen.secondaryValue)
                        .font(.title3.weight(.semibold))
                        .foregroundStyle(.secondary)
                }

                if let warning = screen.warning {
                    Text(warning)
                        .font(.body.weight(.semibold))
                        .foregroundStyle(.orange)
                }

                VStack(spacing: 10) {
                    ForEach(screen.metrics, id: \.label) { metric in
                        HStack {
                            Text(metric.label)
                                .foregroundStyle(.secondary)
                            Spacer()
                            Text(metric.value)
                                .monospacedDigit()
                        }
                        .font(.body)
                    }
                }

                Divider()

                VStack(alignment: .leading, spacing: 6) {
                    Text("Live speed MVP")
                        .font(.headline)
                    Text("Current readout: \(liveSpeed) mph")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(24)
            .frame(maxWidth: 560, alignment: .leading)
        }
        .navigationTitle(screen.title)
    }
}
