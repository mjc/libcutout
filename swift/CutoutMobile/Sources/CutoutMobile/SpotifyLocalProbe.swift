#if DEBUG && os(iOS) && canImport(SpotifyiOS)
    import Observation
    @preconcurrency import SpotifyiOS
    import SwiftUI

    /// Isolates the documented App Remote flow from CutOut's production recovery.
    /// Only available through the --spotify-local-probe debug launch argument.
    // Spotify documents that SDK calls and delegate callbacks use the main thread.
    @MainActor @Observable
    final class SpotifyLocalProbe: NSObject, @preconcurrency SPTAppRemoteDelegate, @preconcurrency
        SPTAppRemotePlayerStateDelegate
    {
        var status = "Ready"
        var title = ""
        var artist = ""
        var isConnected = false
        var isPaused = true
        var errorCode: String?
        private var remote: SPTAppRemote?
        private var redirectURL: URL?
        private var sceneIsActive = false
        private var connecting = false
        private var awaitingCallback = false

        override init() {
            super.init()
            guard let clientID = Bundle.main.object(forInfoDictionaryKey: "SpotifyClientID") as? String,
                !clientID.isEmpty, !clientID.hasPrefix("$("),
                let redirect = Bundle.main.object(forInfoDictionaryKey: "SpotifyRedirectURI") as? String,
                let redirectURL = URL(string: redirect)
            else {
                status = "Spotify configuration missing"
                return
            }
            self.redirectURL = redirectURL
            let configuration = SPTConfiguration(clientID: clientID, redirectURL: redirectURL)
            remote = SPTAppRemote(configuration: configuration, logLevel: .none)
            remote?.delegate = self
        }

        func resumeInSpotify() {
            guard let remote else { return }
            errorCode = nil
            awaitingCallback = true
            status = "Opening Spotify"
            // This is a playback action. It is never called on launch or reconnect.
            remote.authorizeAndPlayURI("") { [weak self] installed in
                guard !installed else { return }
                Task { @MainActor [weak self] in
                    self?.awaitingCallback = false
                    self?.status = "Spotify is not installed"
                }
            }
        }

        func handleURL(_ url: URL) {
            guard awaitingCallback, let remote, let redirectURL,
                url.scheme == redirectURL.scheme, url.host == redirectURL.host,
                url.path == redirectURL.path || (url.path == "/" && redirectURL.path.isEmpty)
            else { return }
            awaitingCallback = false
            let parameters = remote.authorizationParameters(from: url)
            guard let token = parameters?[SPTAppRemoteAccessTokenKey], !token.isEmpty else {
                status = "Spotify did not connect"
                return
            }
            remote.connectionParameters.accessToken = token
            print("spotify_local_probe callback_token_received=true")
            connectIfReady()
        }

        func setActive(_ active: Bool) {
            sceneIsActive = active
            if active {
                connectIfReady()
            } else {
                remote?.disconnect()
                connecting = false
                isConnected = false
            }
        }

        func connectIfReady() {
            guard sceneIsActive, !connecting, let remote,
                !remote.isConnected, remote.connectionParameters.accessToken != nil
            else { return }
            connecting = true
            status = "Connecting"
            print("spotify_local_probe connect")
            remote.connect()
        }

        func togglePlayback() {
            guard isConnected, let player = remote?.playerAPI else { return }
            let callback: SPTAppRemoteCallback = { [weak self] _, error in
                guard let error else { return }
                Task { @MainActor [weak self] in self?.showError(error) }
            }
            if isPaused { player.resume(callback) } else { player.pause(callback) }
        }

        func appRemoteDidEstablishConnection(_ appRemote: SPTAppRemote) {
            connecting = false
            isConnected = true
            status = "Connected locally"
            errorCode = nil
            print("spotify_local_probe connected")
            appRemote.playerAPI?.delegate = self
            appRemote.playerAPI?.subscribe(toPlayerState: { [weak self] _, error in
                if let error { self?.showError(error) }
            })
            appRemote.playerAPI?.getPlayerState { [weak self] state, error in
                if let state = state as? SPTAppRemotePlayerState {
                    self?.playerStateDidChange(state)
                } else if let error {
                    self?.showError(error)
                }
            }
        }

        func appRemote(_ appRemote: SPTAppRemote, didFailConnectionAttemptWithError error: Error?) {
            connecting = false
            isConnected = false
            status = "Open Spotify to resume"
            if let error { showError(error) }
        }

        func appRemote(_ appRemote: SPTAppRemote, didDisconnectWithError error: Error?) {
            connecting = false
            isConnected = false
            status = "Open Spotify to resume"
            if let error { showError(error) }
        }

        func playerStateDidChange(_ playerState: SPTAppRemotePlayerState) {
            title = playerState.track.name
            artist = playerState.track.artist.name
            isPaused = playerState.isPaused
            status = isPaused ? "Paused · local connection" : "Playing · local connection"
            print("spotify_local_probe player_state paused=\(isPaused)")
        }

        private func showError(_ error: Error) {
            var codes: [String] = []
            var current: NSError? = error as NSError
            for _ in 0..<4 {
                guard let value = current else { break }
                codes.append("\(value.domain):\(value.code)")
                current = value.userInfo[NSUnderlyingErrorKey] as? NSError
            }
            errorCode = codes.joined(separator: " / ")
            print("spotify_local_probe error=\(errorCode ?? "unknown")")
        }
    }

    public struct SpotifyLocalProbeView: View {
        @Environment(\.scenePhase) private var scenePhase
        @State private var probe = SpotifyLocalProbe()
        @Binding private var callbackURL: URL?

        public init(callbackURL: Binding<URL?>) {
            _callbackURL = callbackURL
        }

        public var body: some View {
            NavigationStack {
                Form {
                    Section("App Remote only") {
                        Text(probe.status)
                        if !probe.title.isEmpty {
                            Text(probe.title).font(.headline)
                            Text(probe.artist)
                        }
                        if let errorCode = probe.errorCode { Text(errorCode).font(.caption) }
                    }
                    Section {
                        if probe.isConnected {
                            Button(probe.isPaused ? "Play" : "Pause") { probe.togglePlayback() }
                        } else {
                            Button("Resume in Spotify") { probe.resumeInSpotify() }
                        }
                    } footer: {
                        Text(
                            "Resume in Spotify opens Spotify and starts playback. This check makes no Web API requests and does not save account or ride data."
                        )
                    }
                }
                .navigationTitle("Local Spotify check")
            }
            .onAppear { probe.setActive(scenePhase == .active) }
            .onChange(of: scenePhase) { probe.setActive(scenePhase == .active) }
            .onChange(of: callbackURL) { _, url in
                guard let url else { return }
                probe.handleURL(url)
                callbackURL = nil
            }
        }
    }
#endif
