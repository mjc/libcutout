import Foundation
import PackagePlugin

@main
struct VerifyRustArtifact: BuildToolPlugin {
    func createBuildCommands(context: PluginContext, target: Target) throws -> [Command] {
        let root = context.package.directoryURL.deletingLastPathComponent().deletingLastPathComponent()
        let output = context.pluginWorkDirectoryURL.appendingPathComponent("verified")
        guard
            let ffi = context.package.dependencies.map(\.package)
                .first(where: { $0.displayName == "CutoutMobileFFI" }),
            let target = ffi.targets.first(where: { $0.name == "CutoutMobileFFI" })
        else {
            throw MissingRustDependency()
        }
        // Read the pinned build graph, never the current generation selector.
        let generation = target.directoryURL.deletingLastPathComponent().deletingLastPathComponent()
        return [
            .prebuildCommand(
                displayName: "Verify Rust FFI source and artifact identity",
                executable: generation.appendingPathComponent(".cutout-ffi-check"),
                arguments: ["swift-ffi-check", root.path, generation.path, output.path],
                outputFilesDirectory: output
            )
        ]
    }
}

private struct MissingRustDependency: Error {}
