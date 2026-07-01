import SwiftUI

@main
struct CutoutMobileSpeedApp: App {
    @StateObject private var model = LiveSpeedModel()

    init() {
        if CommandLine.arguments.contains("--smoke") {
            print("speed_app=ok")
            Foundation.exit(EXIT_SUCCESS)
        }
    }

    var body: some Scene {
        WindowGroup("Cutout Speed") {
            ContentView(model: model)
                .frame(minWidth: 360, minHeight: 280)
                .task {
                    model.start()
                }
        }
    }
}
