package lol.cutout

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.text.format.DateFormat
import android.view.Gravity
import android.view.View
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import uniffi.cutout_mobile_ffi.*

/** Expanded live player, parallel to MusicExpandedPlayer. */
class MusicPanel(
    context: Context,
    private val client: AndroidMusicClient,
    private val ui: CutoutUi = CutoutUi(context),
    private val onOpenSettings: () -> Unit,
    private val onHidePlayer: () -> Unit,
    private val onDone: () -> Unit,
) : LinearLayout(context) {
    private val artwork = ImageView(context).apply {
        scaleType = ImageView.ScaleType.CENTER_CROP
        background = ui.rounded(12, ui.card, false)
        contentDescription = context.getString(R.string.music_artwork)
    }
    private val title = ui.text(context.getString(R.string.music_nothing_playing), 22f, true)
    private val artist = ui.text("", 16f, color = ui.muted)
    private val status = ui.text("", 13f, color = ui.muted)
    private val controls = ui.row().apply { gravity = Gravity.CENTER }
    private val timeline = ui.column()
    private val spotifyLink = ui.action(context.getString(R.string.music_spotify_content_link)) {
        spotifyLinkUrl?.let(::openValidatedLink)
    }.apply { tag = "music.spotify-content-link"; visibility = View.GONE }
    private var spotifyLinkUrl: String? = null
    private val providerAction = ui.action(context.getString(R.string.music_open_provider)) {
        client.openSelectedProvider()
    }.apply { tag = "music.open-provider" }
    private var observation: AutoCloseable? = client.observe(::render)

    init {
        orientation = VERTICAL
        tag = "music.player.screen"
        setPadding(ui.dp(16), ui.dp(8), ui.dp(16), ui.dp(20))
        val toolbar = ui.row()
        toolbar.addView(ui.text(context.getString(R.string.music_title), 20f, true), LinearLayout.LayoutParams(0, ui.dp(48), 1f))
        toolbar.addView(ui.action(context.getString(R.string.music_done)) { onDone() }.apply {
            tag = "music.done"
        }, LinearLayout.LayoutParams(-2, -2))
        addView(toolbar, LayoutParams(-1, -2))

        val form = ui.column()
        form.gravity = Gravity.CENTER_HORIZONTAL
        title.gravity = Gravity.START
        artist.gravity = Gravity.START
        status.gravity = Gravity.START
        val largeText = resources.configuration.fontScale >= 1.5f
        val hero = ui.row().apply {
            orientation = if (largeText) VERTICAL else HORIZONTAL
            gravity = if (largeText) Gravity.START else Gravity.CENTER_VERTICAL
            setPadding(0, ui.dp(8), 0, ui.dp(8))
        }
        hero.addView(artwork, LinearLayout.LayoutParams(ui.dp(84), ui.dp(84)).apply {
            if (largeText) gravity = Gravity.START
        })
        val metadata = ui.column().apply {
            gravity = Gravity.CENTER_VERTICAL
            setPadding(ui.dp(16), 0, 0, 0)
            addView(title, LayoutParams(-1, -2))
            addView(artist, LayoutParams(-1, -2))
            addView(status, LayoutParams(-1, -2))
        }
        hero.addView(metadata, if (largeText) LayoutParams(-1, -2) else LinearLayout.LayoutParams(0, -2, 1f))
        form.addView(hero, LayoutParams(-1, -2))
        form.addView(ui.space(12))
        form.addView(controls, LayoutParams(-1, ui.dp(52)))
        form.addView(timeline, LayoutParams(-1, -2))
        form.addView(providerAction, LayoutParams(-1, -2))
        form.addView(spotifyLink, LayoutParams(-1, -2))
        form.addView(ui.action(context.getString(R.string.music_settings)) { onOpenSettings() }.apply {
            tag = "music.open-settings"
        }, LayoutParams(-1, -2))
        form.addView(ui.action(context.getString(R.string.music_hide)) { onHidePlayer() }, LayoutParams(-1, -2))
        val scroll = ScrollView(context).apply {
            isFillViewport = true
            addView(form, android.widget.FrameLayout.LayoutParams(-1, -2))
        }
        addView(scroll, LayoutParams(-1, 0, 1f))
        render(client.latestPresentation)
    }

    fun render(value: AndroidMusicPresentation) {
        val snapshot = value.snapshot
        title.text = snapshot?.item?.title ?: ui.label(
            musicPlaybackTitleKey(snapshot?.state ?: MobileMusicPlaybackStateDto.UNAVAILABLE),
        )
        artist.text = snapshot?.item?.artist.orEmpty()
        artist.visibility = if (artist.text.isNullOrBlank()) View.GONE else View.VISIBLE
        status.text = value.error ?: value.selectedProvider.providerTitle(context)
        artwork.setImageBitmap(value.artwork)
        controls.removeAllViews()
        val caps = snapshot?.capabilities
        if (value.listenerAccessGranted && caps != null) {
            if (caps.previous) addControl("‹", context.getString(R.string.music_previous), MobileMusicCommandDto.PREVIOUS)
            if (caps.pause && snapshot.state == MobileMusicPlaybackStateDto.PLAYING) {
                addControl("Ⅱ", context.getString(R.string.music_pause), MobileMusicCommandDto.PAUSE)
            } else if (caps.play) {
                addControl("▶", context.getString(R.string.music_play), MobileMusicCommandDto.PLAY)
            }
            if (caps.next) addControl("›", context.getString(R.string.music_next), MobileMusicCommandDto.NEXT)
        }
        providerAction.visibility = if (
            value.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD || caps?.openProvider == true ||
            snapshot == null
        ) View.VISIBLE else View.GONE
        timeline.removeAllViews()
        val events = value.history?.events.orEmpty().filter {
            it.kind != MobileMusicRideEventKindDto.PROVIDER_DISCONNECTED &&
                !(it.kind == MobileMusicRideEventKindDto.STOPPED && it.itemIdentifier == null && it.title == null)
        }
        if (events.isNotEmpty()) {
            timeline.addView(ui.space(12))
            timeline.addView(ui.text(context.getString(R.string.music_timeline_title), 15f, true), LayoutParams(-1, -2))
            events.forEach { event ->
                val titleText = event.title ?: context.getString(R.string.music_history_opaque_item)
                val providerTitle = event.provider.providerTitle(context)
                val subtitle = "${event.kind.timelineTitle(context)} · $providerTitle" +
                    (event.artist?.takeIf(String::isNotBlank)?.let { " · $it" } ?: "")
                val eventRow = ui.row().apply {
                    gravity = Gravity.CENTER_VERTICAL
                    setPadding(0, ui.dp(7), 0, ui.dp(7))
                    val eventMetadata = ui.column()
                    eventMetadata.addView(ui.text(titleText, 15f, true), LinearLayout.LayoutParams(-1, -2))
                    eventMetadata.addView(ui.text(subtitle, 13f, color = ui.muted), LinearLayout.LayoutParams(-1, -2))
                    addView(eventMetadata, LinearLayout.LayoutParams(0, -2, 1f))
                    addView(ui.text(DateFormat.getTimeFormat(context).format(java.util.Date(event.wallClockAtMs.toLong())), 12f, color = ui.muted),
                        LinearLayout.LayoutParams(-2, -2))
                }
                timeline.addView(eventRow, LayoutParams(-1, -2))
                if (event.provider == MobileMusicProviderDto.SPOTIFY && !event.itemIdentifier.isNullOrBlank()) {
                    spotifyContentLink(event.itemIdentifier!!)?.let { url ->
                        timeline.addView(ui.action(context.getString(R.string.music_spotify_content_link)) {
                            openValidatedLink(url)
                        }.apply { tag = "music.timeline.spotify-link" }, LayoutParams(-1, -2))
                    }
                }
                timeline.addView(ui.divider())
            }
        }
        spotifyLinkUrl = contentLink(snapshot)
        spotifyLink.visibility = if (spotifyLinkUrl == null) View.GONE else View.VISIBLE
    }

    private fun contentLink(snapshot: MobileMusicSnapshotDto?): String? =
        if (snapshot?.provider == MobileMusicProviderDto.SPOTIFY) snapshot.item?.identifier?.let(::spotifyContentLink) else null

    private fun spotifyContentLink(identifier: String): String? =
        runCatching { spotifyContentUrl(identifier) }.getOrNull()?.takeIf { value ->
            runCatching { Uri.parse(value).let { it.scheme == "https" && it.host == "open.spotify.com" } }.getOrDefault(false)
        }

    private fun openValidatedLink(value: String) {
        runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(value))) }
            .onFailure { status.text = it.localizedMessage ?: context.getString(R.string.music_provider_unavailable) }
    }


    private fun addControl(glyph: String, label: String, command: MobileMusicCommandDto) {
        controls.addView(ui.action(glyph, primary = command == MobileMusicCommandDto.PLAY || command == MobileMusicCommandDto.PAUSE) {
            client.command(command)
        }.apply { contentDescription = label }, LayoutParams(0, -2, 1f))
    }

    fun dispose() {
        observation?.close()
        observation = null
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (observation == null) observation = client.observe(::render)
    }

    override fun onDetachedFromWindow() {
        dispose()
        super.onDetachedFromWindow()
    }
}

/** Provider selection and the OS permission handoff live in Setup, as in MusicSettingsView. */
class MusicSettingsPanel(
    context: Context,
    private val client: AndroidMusicClient,
    private val ui: CutoutUi = CutoutUi(context),
    private val onRequestListenerAccess: () -> Unit,
    private val onChooseSoundCloud: () -> Unit = {},
) : LinearLayout(context) {
    private val status = ui.text("", 14f, color = ui.muted)
    private val accessAction = ui.action(context.getString(R.string.music_enable_media_access), primary = true) {
        onRequestListenerAccess()
    }
    private val connectAction = ui.action(context.getString(R.string.music_connect_provider), primary = true) {
        client.connectOrRetrySelectedProvider()
    }.apply { tag = "music.connect-provider" }
    private val openProviderAction = ui.action(context.getString(R.string.music_open_provider)) {
        client.openProviderFromSettings()
    }.apply { tag = "music.open-provider" }
    private val spotifyDisconnect = ui.action(context.getString(R.string.music_spotify_disconnect)) {
        android.app.AlertDialog.Builder(context)
            .setMessage(R.string.music_spotify_disconnect_confirm)
            .setNegativeButton(R.string.cancel, null)
            .setPositiveButton(R.string.music_spotify_disconnect) { _, _ -> client.disconnectSpotify() }
            .show()
    }.apply { tag = "music.disconnect-spotify" }
    private val soundCloudChoose = ui.action(context.getString(R.string.music_soundcloud_choose_music), primary = true) {
        onChooseSoundCloud()
    }.apply { tag = "music.soundcloud.choose-music" }
    private val providerError = ui.text("", 13f, color = ui.red)
    private val providers = ui.column()
    private val historyPolicies = ui.column()
    private val historyStatus = ui.text("", 13f, color = ui.muted)
    private val forgetHistory = ui.action(context.getString(R.string.music_history_forget)) {
        client.deleteCurrentHistory()
    }.apply { tag = "music.history.forget" }
    private var observation: AutoCloseable? = client.observe(::render)

    init {
        orientation = VERTICAL
        tag = "music.settings.screen"
        setPadding(ui.dp(20), ui.dp(16), ui.dp(20), ui.dp(20))
        addView(ui.title(context.getString(R.string.music_settings)), LayoutParams(-1, -2))
        addView(ui.space(8))
        addView(status, LayoutParams(-1, -2))
        addView(accessAction, LayoutParams(-1, -2))
        addView(ui.space(12))
        addView(connectAction, LayoutParams(-1, -2))
        addView(openProviderAction, LayoutParams(-1, -2))
        addView(spotifyDisconnect, LayoutParams(-1, -2))
        addView(soundCloudChoose, LayoutParams(-1, -2))
        addView(providerError, LayoutParams(-1, -2))
        addView(ui.text(context.getString(R.string.music_provider_title), 13f, true, ui.muted), LayoutParams(-1, -2))
        addView(providers, LayoutParams(-1, -2))
        addView(ui.space(16))
        addView(ui.text(context.getString(R.string.music_history_title), 13f, true, ui.muted), LayoutParams(-1, -2))
        addView(historyPolicies, LayoutParams(-1, -2))
        addView(historyStatus, LayoutParams(-1, -2))
        addView(forgetHistory, LayoutParams(-1, -2))
        client.refreshHistory()
    }

    fun render(value: AndroidMusicPresentation) {
        status.text = when {
            value.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD -> context.getString(R.string.music_soundcloud_ready)
            !value.listenerAccessGranted -> context.getString(R.string.music_access_required)
            value.error != null -> value.error
            else -> context.getString(R.string.music_access_enabled)
        }
        accessAction.visibility = if (value.listenerAccessGranted || value.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD) View.GONE else View.VISIBLE
        connectAction.visibility = if (value.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD || value.spotifyDisconnectPending) View.GONE else View.VISIBLE
        openProviderAction.text = context.getString(R.string.music_open_named_provider, value.selectedProvider.providerTitle(context))
        spotifyDisconnect.visibility = if (value.selectedProvider == MobileMusicProviderDto.SPOTIFY) View.VISIBLE else View.GONE
        spotifyDisconnect.isEnabled = !value.spotifyDisconnectPending
        spotifyDisconnect.text = context.getString(if (value.spotifyDisconnectError != null) R.string.music_spotify_disconnect_retry else R.string.music_spotify_disconnect)
        soundCloudChoose.visibility = if (value.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD) View.VISIBLE else View.GONE
        providerError.text = when {
            value.spotifyDisconnectPending -> context.getString(R.string.music_spotify_disconnect_pending)
            else -> value.spotifyDisconnectError ?: value.error.orEmpty()
        }
        providerError.visibility = if (providerError.text.isNullOrBlank()) View.GONE else View.VISIBLE
        providers.removeAllViews()
        listOf(
            MobileMusicProviderDto.APPLE_MUSIC to context.getString(R.string.music_apple_music),
            MobileMusicProviderDto.SPOTIFY to context.getString(R.string.music_spotify),
            MobileMusicProviderDto.SOUND_CLOUD to context.getString(R.string.music_soundcloud),
        ).forEach { (provider, label) ->
            val selected = value.selectedProvider == provider
            providers.addView(ui.action(if (selected) "✓  $label" else label, primary = selected) {
                client.selectProvider(provider)
            }, LayoutParams(-1, -2))
        }
        historyPolicies.removeAllViews()
        val providerSupportsHistory = value.selectedProvider != MobileMusicProviderDto.SOUND_CLOUD
        listOf(
            MobileMusicHistoryPolicyDto.DISABLED to context.getString(R.string.music_history_disabled),
            MobileMusicHistoryPolicyDto.OPAQUE_ITEM to context.getString(R.string.music_history_opaque),
            MobileMusicHistoryPolicyDto.HUMAN_READABLE to context.getString(R.string.music_history_human_readable),
        ).forEach { (policy, label) ->
            val selected = value.historyPolicy == policy
            historyPolicies.addView(ui.action(if (selected) "✓  $label" else label, primary = selected) {
                client.setHistoryPolicy(policy)
            }.apply {
                tag = "music.history.policy.${policy.name.lowercase()}"
                isEnabled = providerSupportsHistory
                alpha = if (providerSupportsHistory) 1f else 0.55f
            }, LayoutParams(-1, -2))
        }
        historyPolicies.isEnabled = providerSupportsHistory
        historyPolicies.alpha = if (providerSupportsHistory) 1f else 0.55f
        historyStatus.text = value.historyError ?: when (value.history?.status) {
            MobileMusicHistoryStatusDto.UNAVAILABLE -> context.getString(R.string.music_history_unavailable)
            MobileMusicHistoryStatusDto.DELETED -> context.getString(R.string.music_history_deleted)
            MobileMusicHistoryStatusDto.REDACTED -> context.getString(R.string.music_history_redacted)
            MobileMusicHistoryStatusDto.AVAILABLE -> context.getString(R.string.music_history_available)
            MobileMusicHistoryStatusDto.DISABLED -> context.getString(R.string.music_history_disabled)
            MobileMusicHistoryStatusDto.MISSING, null -> ""
        }
        if (!providerSupportsHistory) {
            historyStatus.text = context.getString(R.string.music_history_provider_unavailable)
        }
        forgetHistory.visibility = if (value.history?.events?.isNotEmpty() == true) View.VISIBLE else View.GONE
    }

    fun dispose() {
        observation?.close()
        observation = null
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (observation == null) observation = client.observe(::render)
    }

    override fun onDetachedFromWindow() {
        dispose()
        super.onDetachedFromWindow()
    }
}

private fun MobileMusicProviderDto.providerTitle(context: Context): String = when (this) {
    MobileMusicProviderDto.APPLE_MUSIC -> context.getString(R.string.music_apple_music)
    MobileMusicProviderDto.SPOTIFY -> context.getString(R.string.music_spotify)
    MobileMusicProviderDto.SOUND_CLOUD -> context.getString(R.string.music_soundcloud)
}

private fun MobileMusicRideEventKindDto.timelineTitle(context: Context): String = when (this) {
    MobileMusicRideEventKindDto.PLAY -> context.getString(R.string.music_timeline_play)
    MobileMusicRideEventKindDto.PAUSE -> context.getString(R.string.music_timeline_pause)
    MobileMusicRideEventKindDto.SKIP -> context.getString(R.string.music_timeline_skip)
    MobileMusicRideEventKindDto.ITEM_CHANGED -> context.getString(R.string.music_timeline_item_changed)
    MobileMusicRideEventKindDto.STOPPED -> context.getString(R.string.music_timeline_stopped)
    MobileMusicRideEventKindDto.PROVIDER_DISCONNECTED -> context.getString(R.string.music_timeline_disconnected)
}
