import Foundation
import PackagePlugin

@main
struct VerifyRustArtifact: BuildToolPlugin {
    func createBuildCommands(context: PluginContext, target: Target) throws -> [Command] {
        let root = context.package.directoryURL.deletingLastPathComponent().deletingLastPathComponent()
        let output = context.pluginWorkDirectoryURL.appendingPathComponent("verified")
        return [.prebuildCommand(
            displayName: "Verify Rust FFI source and artifact identity",
            executable: root.appendingPathComponent("target/swift-ffi/CutoutMobileFFI/.cutout-ffi-check"),
            arguments: ["swift-ffi-check", root.path, output.path],
            outputFilesDirectory: output
        )]
    }
}
