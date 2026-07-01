import CutoutMobile
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: LiveSpeedModel

    var body: some View {
        VStack(spacing: 14) {
            Text(model.speed.displayValue)
                .font(.system(size: 112, weight: .bold, design: .rounded))
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.45)
                .accessibilityLabel("Speed")
                .accessibilityValue(model.speed.displayValue)

            Text(model.speed.displayUnit)
                .font(.title2.weight(.semibold))
                .foregroundStyle(.secondary)

            Text(model.status)
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .padding(32)
    }
}
