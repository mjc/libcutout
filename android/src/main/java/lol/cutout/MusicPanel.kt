package lol.cutout

import android.content.Context
import android.app.Activity
import android.content.Intent
import android.content.ContextWrapper
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
        clipToOutline = true
        contentDescription = context.getString(R.string.music_artwork)
    }
    private val title = ui.text(context.getString(R.string.music_nothing_playing), 22f, true)
    private val artist = ui.text("", 16f, color = ui.muted)
    private val status = ui.text("", 13f, color = ui.muted)
    private val controls = ui.row().apply { gravity = Gravity.CENTER }
    private val controlViews = linkedMapOf<MobileMusicCommandDto, View>()
    private val timeline = ui.column()
    private val timelineSection = ui.column().apply { tag = "music.timeline.section" }
    private val providerName = ui.text("", 14f, color = ui.muted)
    private var renderedTimelineKey: List<String>? = null
    private val spotifyLink = formAction(context.getString(R.string.music_spotify_content_link), CutoutSymbol.OPEN_EXTERNAL) {
        spotifyLinkUrl?.let(::openValidatedLink)
    }.apply { tag = "music.spotify-content-link"; visibility = View.GONE }
    private var spotifyLinkUrl: String? = null
    private val providerAction = formAction(context.getString(R.string.music_open_provider), CutoutSymbol.OPEN_EXTERNAL) {
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
            background = null
        }, LinearLayout.LayoutParams(-2, -2))
        addView(toolbar, LayoutParams(-1, -2))

        val form = ui.column()
        title.gravity = Gravity.START
        artist.gravity = Gravity.START
        status.gravity = Gravity.START
        val largeText = resources.configuration.fontScale >= 1.5f
        title.maxLines = if (largeText) Int.MAX_VALUE else 2
        artist.maxLines = if (largeText) Int.MAX_VALUE else 2
        title.ellipsize = android.text.TextUtils.TruncateAt.END
        artist.ellipsize = android.text.TextUtils.TruncateAt.END
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
            setPadding(if (largeText) 0 else ui.dp(16), if (largeText) ui.dp(8) else 0, 0, 0)
            addView(title, LayoutParams(-1, -2))
            addView(artist, LayoutParams(-1, -2))
            addView(status, LayoutParams(-1, -2))
        }
        hero.addView(metadata, if (largeText) LayoutParams(-1, -2) else LinearLayout.LayoutParams(0, -2, 1f))
        form.addView(group("music.playback.section").apply {
            addView(hero, LayoutParams(-1, -2))
            addView(ui.space(8))
            addView(controls, LayoutParams(-1, ui.dp(44)))
            addView(providerAction, LayoutParams(-1, -2))
            addView(spotifyLink, LayoutParams(-1, -2))
            addView(providerName, LayoutParams(-1, -2))
        }, LayoutParams(-1, -2))
        form.addView(ui.space(18))
        form.addView(group("music.actions.section").apply {
            addView(formAction(context.getString(R.string.music_settings), CutoutSymbol.GEAR) { onOpenSettings() }.apply {
                tag = "music.open-settings"
            }, LayoutParams(-1, -2))
            addView(ui.divider())
            addView(formAction(context.getString(R.string.music_hide), CutoutSymbol.HIDE) { onHidePlayer() }.apply {
                tag = "music.hide"
            }, LayoutParams(-1, -2))
        }, LayoutParams(-1, -2))
        timelineSection.addView(ui.space(18))
        timelineSection.addView(group("music.timeline.content").apply { addView(timeline, LayoutParams(-1, -2)) })
        form.addView(timelineSection, LayoutParams(-1, -2))
        val scroll = ScrollView(context).apply {
            isFillViewport = true
            addView(form, android.widget.FrameLayout.LayoutParams(-1, -2))
        }
        addView(scroll, LayoutParams(-1, 0, 1f))
        render(client.latestPresentation)
    }

    private fun group(identifier: String) = ui.column().apply {
        tag = identifier
        setPadding(ui.dp(16), ui.dp(12), ui.dp(16), ui.dp(12))
        background = ui.rounded(22)
    }

    private fun formAction(label: String, symbol: CutoutSymbol, action: () -> Unit) =
        musicFormAction(context, ui, label, plainRow = false, symbol = symbol, action = action)

    fun render(value: AndroidMusicPresentation) {
        val snapshot = value.snapshot
        title.text = snapshot?.item?.title ?: ui.label(
            musicPlaybackTitleKey(snapshot?.state ?: MobileMusicPlaybackStateDto.UNAVAILABLE),
        )
        artist.text = snapshot?.item?.artist.orEmpty()
        artist.visibility = if (artist.text.isNullOrBlank()) View.GONE else View.VISIBLE
        status.text = value.error ?: value.snapshot?.state?.let { context.getString(it.settingsStateLabel()) }
            ?: context.getString(R.string.music_state_not_connected)
        status.contentDescription = status.text
        status.visibility = if (value.error == null && snapshot?.state in setOf(
            MobileMusicPlaybackStateDto.PLAYING, MobileMusicPlaybackStateDto.PAUSED,
        )) View.GONE else View.VISIBLE
        if (value.artwork == null) artwork.setImageDrawable(CutoutIcon(CutoutSymbol.MUSIC, ui.muted))
        else artwork.setImageBitmap(value.artwork)
        val visibleCommands = value.availableCommands(includePrevious = true, includeOpenProvider = false).toSet()
        val controlsByCommand = listOf(MobileMusicCommandDto.PREVIOUS, MobileMusicCommandDto.PAUSE,
            MobileMusicCommandDto.PLAY, MobileMusicCommandDto.NEXT)
        controlViews.keys.toList().filterNot(visibleCommands::contains).forEach { controlViews[it]?.visibility = View.GONE }
        controlsByCommand.filter { it in visibleCommands }.forEach { command ->
            addControl(command).visibility = View.VISIBLE
        }
        controls.visibility = if (visibleCommands.isEmpty()) View.GONE else View.VISIBLE
        val caps = snapshot?.capabilities
        spotifyLinkUrl = contentLink(snapshot)
        spotifyLink.visibility = if (spotifyLinkUrl == null) View.GONE else View.VISIBLE
        providerAction.text = context.getString(R.string.music_open_named_provider, value.selectedProvider.providerTitle(context))
        providerAction.visibility = if (
            spotifyLinkUrl == null && (value.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD ||
                caps?.openProvider == true || snapshot == null)
        ) View.VISIBLE else View.GONE
        providerName.text = value.selectedProvider.providerTitle(context)
        providerName.visibility = if (spotifyLinkUrl == null && providerAction.visibility == View.GONE) View.VISIBLE else View.GONE
        val events = value.history?.events.orEmpty().filter {
            it.kind != MobileMusicRideEventKindDto.PROVIDER_DISCONNECTED &&
                !(it.kind == MobileMusicRideEventKindDto.STOPPED && it.itemIdentifier == null && it.title == null)
        }
        val timelineKey = events.map { listOf(it.sequence.toString(), it.kind.name, it.itemIdentifier.orEmpty(), it.title.orEmpty(), it.artist.orEmpty()).joinToString("|") }
        timelineSection.visibility = if (events.isEmpty()) View.GONE else View.VISIBLE
        if (timelineKey != renderedTimelineKey) {
            timeline.removeAllViews()
            renderedTimelineKey = timelineKey
            if (events.isNotEmpty()) {
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
                            timeline.addView(formAction(context.getString(R.string.music_spotify_content_link), CutoutSymbol.OPEN_EXTERNAL) {
                                openValidatedLink(url)
                            }.apply { tag = "music.timeline.spotify-link" }, LayoutParams(-1, -2))
                        }
                    }
                    timeline.addView(ui.divider())
                }
            }
        }
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


    private fun addControl(command: MobileMusicCommandDto): View {
        controlViews[command]?.let { return it }
        val view = musicIconControl(context, ui, command) { client.command(command) }
        controls.addView(view, musicCommandInsertionIndex(controls, controlViews, command), LayoutParams(ui.dp(44), ui.dp(44)))
        controlViews[command] = view
        return view
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

/** Plain, leading-aligned action row used by the SwiftUI Form settings surface. */
private fun musicFormAction(
    context: Context,
    ui: CutoutUi,
    label: String,
    plainRow: Boolean = true,
    symbol: CutoutSymbol? = null,
    tag: String? = null,
    destructive: Boolean = false,
    action: () -> Unit,
) = android.widget.Button(context).apply {
    text = label
    if (tag != null) this.tag = tag
    textSize = 17f
    isAllCaps = false
    includeFontPadding = false
    typeface = android.graphics.Typeface.DEFAULT
    gravity = Gravity.START or Gravity.CENTER_VERTICAL
    minHeight = ui.dp(44)
    minimumHeight = ui.dp(44)
    minWidth = 0
    minimumWidth = 0
    setPadding(0, ui.dp(if (plainRow) 8 else 10), 0, ui.dp(if (plainRow) 8 else 10))
    setTextColor(if (destructive) ui.red else ui.brand)
    background = if (plainRow) {
        android.graphics.drawable.RippleDrawable(
            android.content.res.ColorStateList.valueOf(if (ui.dark) 0x22FFFFFF else 0x11000000), null, null,
        )
    } else {
        android.graphics.drawable.RippleDrawable(
            android.content.res.ColorStateList.valueOf(if (ui.dark) 0x33FFFFFF else 0x22000000), null,
            ui.rounded(10, android.graphics.Color.WHITE, outlined = false),
        )
    }
    if (symbol != null) {
        setCompoundDrawablesRelative(CutoutIcon(symbol, if (destructive) ui.red else ui.brand).apply {
            setBounds(0, 0, ui.dp(20), ui.dp(20))
        }, null, null, null)
        compoundDrawablePadding = ui.dp(12)
    }
    setOnClickListener { action() }
}

/** Provider selection and the OS permission handoff live in Setup, as in MusicSettingsView. */
class MusicSettingsPanel(
    context: Context,
    private val client: AndroidMusicClient,
    private val ui: CutoutUi = CutoutUi(context),
    private val onRequestListenerAccess: () -> Unit,
    private val onChooseSoundCloud: () -> Unit = {},
) : ScrollView(context) {
    private val page = ui.column()
    private val status = ui.text("", 14f, color = ui.muted).apply { tag = "music.connection-status" }
    private val accessExplanation = ui.text(context.getString(R.string.music_access_explanation), 13f, color = ui.muted)
    private val accessAction = musicFormAction(context, ui, context.getString(R.string.music_enable_media_access), tag = "music.enable-access") {
        onRequestListenerAccess()
    }
    private val connectAction = musicFormAction(context, ui, ui.label("music.connect_provider"), tag = "music.connect-provider") {
        client.connectOrRetrySelectedProvider(context.musicActivity())
    }
    private val openProviderAction = musicFormAction(context, ui, "", tag = "music.open-provider") {
        client.openProviderFromSettings()
    }
    private val spotifyDisconnect = musicFormAction(context, ui, context.getString(R.string.music_spotify_disconnect), tag = "music.disconnect-spotify", destructive = true) {
        android.app.AlertDialog.Builder(context)
            .setMessage(ui.label("music.spotify.disconnect.message"))
            .setNegativeButton(R.string.cancel, null)
            .setPositiveButton(R.string.music_spotify_disconnect) { _, _ -> client.disconnectSpotify() }
            .show()
    }
    private val soundCloudChoose = musicFormAction(context, ui, context.getString(R.string.music_soundcloud_choose_music), tag = "music.soundcloud.choose-music") {
        onChooseSoundCloud()
    }
    private val providerError = ui.text("", 13f, color = ui.red).apply { tag = "music.provider-error" }
    private val providers = MusicSettingsChoice(context, ui, context.getString(R.string.music_provider_title), listOf(
        MobileMusicProviderDto.APPLE_MUSIC to context.getString(R.string.music_apple_music),
        MobileMusicProviderDto.SPOTIFY to context.getString(R.string.music_spotify),
        MobileMusicProviderDto.SOUND_CLOUD to context.getString(R.string.music_soundcloud),
    ), { provider, completed -> client.selectProvider(provider); completed() }).apply { tag = "music.provider-picker" }
    private val historyPolicies = MusicSettingsChoice(context, ui, context.getString(R.string.music_history_title), listOf(
        MobileMusicHistoryPolicyDto.DISABLED to context.getString(R.string.music_history_disabled),
        MobileMusicHistoryPolicyDto.OPAQUE_ITEM to context.getString(R.string.music_history_opaque),
        MobileMusicHistoryPolicyDto.HUMAN_READABLE to context.getString(R.string.music_history_human_readable),
    ), { policy, completed -> client.setHistoryPolicy(policy) { completed() } }).apply { tag = "music.history-picker" }
    private val historyStatus = ui.text("", 13f, color = ui.muted).apply { tag = "music.history-status" }
    private val historyExplanation = ui.text("", 13f, color = ui.muted)
    private var observation: AutoCloseable? = client.observe(::render)

    init {
        isFillViewport = true
        tag = "music.settings.screen"
        page.setPadding(ui.dp(20), ui.dp(16), ui.dp(20), ui.dp(20))
        page.addView(ui.title(context.getString(R.string.music_settings)), LayoutParams(-1, -2))
        page.addView(ui.space(8))
        page.addView(section(R.string.music_provider_title) {
            addView(providers, LayoutParams(-1, -2))
            addView(status, LayoutParams(-1, -2))
            addView(accessExplanation, LayoutParams(-1, -2))
            addView(accessAction, LayoutParams(-1, -2))
            addView(connectAction, LayoutParams(-1, -2))
            addView(openProviderAction, LayoutParams(-1, -2))
            addView(spotifyDisconnect, LayoutParams(-1, -2))
            addView(soundCloudChoose, LayoutParams(-1, -2))
            addView(providerError, LayoutParams(-1, -2))
        }, LayoutParams(-1, -2))
        page.addView(ui.space(14))
        page.addView(section(R.string.music_history_title) {
            addView(historyPolicies, LayoutParams(-1, -2))
            addView(historyStatus, LayoutParams(-1, -2))
            addView(historyExplanation, LayoutParams(-1, -2))
        }, LayoutParams(-1, -2))
        addView(page, android.widget.FrameLayout.LayoutParams(-1, -2))
        render(client.latestPresentation)
        client.refreshHistory()
    }

    fun render(value: AndroidMusicPresentation) {
        status.text = when {
            value.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD -> context.getString(R.string.music_soundcloud_ready)
            value.selectedProvider == MobileMusicProviderDto.SPOTIFY && !value.spotifyConfigured -> context.getString(R.string.spotify_unavailable_build)
            value.selectedProvider == MobileMusicProviderDto.SPOTIFY && value.spotifyAuthorizing -> context.getString(R.string.spotify_connecting)
            value.selectedProvider == MobileMusicProviderDto.SPOTIFY && value.spotifyNeedsAuthorization -> context.getString(R.string.spotify_auth_required)
            value.selectedProvider != MobileMusicProviderDto.SPOTIFY && !value.listenerAccessGranted -> context.getString(R.string.music_access_required)
            value.snapshot?.state in setOf(MobileMusicPlaybackStateDto.PLAYING, MobileMusicPlaybackStateDto.PAUSED) ->
                value.snapshot?.item?.title?.takeIf(String::isNotBlank)
                    ?: context.getString(value.snapshot?.state?.settingsStateLabel() ?: R.string.music_state_not_connected)
            value.snapshot?.state == MobileMusicPlaybackStateDto.BUFFERING -> context.getString(R.string.music_state_buffering)
            value.snapshot?.state == MobileMusicPlaybackStateDto.INTERRUPTED -> context.getString(R.string.music_state_interrupted)
            value.snapshot?.state == MobileMusicPlaybackStateDto.STOPPED &&
                value.selectedProvider == MobileMusicProviderDto.SPOTIFY && !value.snapshot?.item?.title.isNullOrBlank() ->
                ui.label("music.open_to_resume").replace("%@", value.selectedProvider.providerTitle(context))
            else -> context.getString(value.snapshot?.state?.settingsStateLabel() ?: R.string.music_state_not_connected)
        }
        status.contentDescription = status.text
        val requiresNotificationListener = value.selectedProvider != MobileMusicProviderDto.SPOTIFY &&
            value.selectedProvider != MobileMusicProviderDto.SOUND_CLOUD
        accessAction.visibility = if (!requiresNotificationListener || value.listenerAccessGranted) View.GONE else View.VISIBLE
        accessExplanation.visibility = if (!requiresNotificationListener || value.listenerAccessGranted) View.GONE else View.VISIBLE
        // Android session control is unavailable until the user grants notification access.
        // Showing Connect here would only retry an action that cannot succeed yet.
        val profile = musicProviderProfile(value.selectedProvider)
        val handoffOnly = profile.`interface` == MobileMusicProviderInterface.APP_HANDOFF_ONLY
        val canConnect = when (value.selectedProvider) {
            MobileMusicProviderDto.SPOTIFY -> value.spotifyConfigured &&
                (!value.spotifyAuthorized || value.spotifyNeedsAuthorization || !value.spotifyAuthorizing)
            MobileMusicProviderDto.SOUND_CLOUD -> false
            MobileMusicProviderDto.APPLE_MUSIC -> value.listenerAccessGranted
        }
        connectAction.visibility = if (handoffOnly || !canConnect || value.spotifyDisconnectPending) View.GONE else View.VISIBLE
        connectAction.isEnabled = !value.spotifyAuthorizing
        val providerTitle = value.selectedProvider.providerTitle(context)
        val reconnect = value.snapshot?.state in setOf(MobileMusicPlaybackStateDto.STALE, MobileMusicPlaybackStateDto.DISCONNECTED)
        connectAction.text = if (value.selectedProvider == MobileMusicProviderDto.SPOTIFY) {
            when {
                value.spotifyCredentialRestoreRetryAvailable -> ui.label("music.spotify.retry_saved_authorization")
                value.spotifyNeedsAuthorization -> context.getString(R.string.spotify_reauthorize)
                else -> ui.label(if (reconnect) "music.reconnect_provider" else "music.connect_provider").replace("%@", providerTitle)
            }
        } else ui.label(if (reconnect) "music.reconnect_provider" else "music.connect_provider").replace("%@", providerTitle)
        openProviderAction.text = ui.label("music.open_named_provider").replace("%@", providerTitle)
        spotifyDisconnect.visibility = if (value.selectedProvider == MobileMusicProviderDto.SPOTIFY) View.VISIBLE else View.GONE
        spotifyDisconnect.isEnabled = !value.spotifyDisconnectPending
        spotifyDisconnect.text = context.getString(if (value.spotifyDisconnectError != null) R.string.music_spotify_disconnect_retry else R.string.music_spotify_disconnect)
        soundCloudChoose.visibility = if (value.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD) View.VISIBLE else View.GONE
        providerError.text = when {
            value.spotifyDisconnectPending -> context.getString(R.string.music_spotify_disconnect_pending)
            requiresNotificationListener && !value.listenerAccessGranted -> ""
            else -> value.spotifyDisconnectError ?: value.error.orEmpty()
        }
        providerError.visibility = if (providerError.text.isNullOrBlank()) View.GONE else View.VISIBLE
        providers.render(value.selectedProvider, enabled = !value.spotifyDisconnectPending)
        val providerSupportsHistory = !handoffOnly
        historyPolicies.render(value.historyPolicy, enabled = providerSupportsHistory && !value.spotifyDisconnectPending)
        listOf(accessAction, openProviderAction, soundCloudChoose).forEach {
            it.isEnabled = !value.spotifyDisconnectPending
        }
        historyStatus.text = value.historyError ?: when (value.history?.status) {
            MobileMusicHistoryStatusDto.UNAVAILABLE -> context.getString(R.string.music_history_unavailable)
            MobileMusicHistoryStatusDto.DELETED -> context.getString(R.string.music_history_deleted)
            MobileMusicHistoryStatusDto.REDACTED -> context.getString(R.string.music_history_redacted)
            MobileMusicHistoryStatusDto.AVAILABLE -> context.getString(R.string.music_history_available)
            MobileMusicHistoryStatusDto.DISABLED -> context.getString(R.string.music_history_disabled)
            MobileMusicHistoryStatusDto.MISSING, null -> ""
        }
        historyStatus.setTextColor(if (value.historyError != null) ui.red else ui.muted)
        if (!providerSupportsHistory) {
            historyStatus.text = context.getString(R.string.music_history_provider_unavailable)
        }
        historyStatus.contentDescription = historyStatus.text
        historyExplanation.text = context.getString(when (value.historyPolicy) {
            MobileMusicHistoryPolicyDto.DISABLED -> R.string.music_history_disabled_explanation
            MobileMusicHistoryPolicyDto.OPAQUE_ITEM -> R.string.music_history_opaque_explanation
            MobileMusicHistoryPolicyDto.HUMAN_READABLE -> R.string.music_history_human_readable_explanation
        }) + " " + context.getString(R.string.music_history_privacy)
        historyExplanation.visibility = if (providerSupportsHistory) View.VISIBLE else View.GONE
    }

    fun dispose() {
        observation?.close()
        observation = null
    }

    private fun section(title: Int, content: LinearLayout.() -> Unit): LinearLayout = ui.column().apply {
        background = ui.rounded(18)
        setPadding(ui.dp(14), ui.dp(10), ui.dp(14), ui.dp(10))
        addView(ui.text(context.getString(title), 14f, true, ui.muted), LayoutParams(-1, -2))
        addView(ui.space(6))
        content()
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

internal fun MobileMusicPlaybackStateDto.settingsStateLabel(): Int = when (this) {
    MobileMusicPlaybackStateDto.PLAYING -> R.string.music_state_playing
    MobileMusicPlaybackStateDto.PAUSED -> R.string.music_state_paused
    MobileMusicPlaybackStateDto.STOPPED -> R.string.music_state_stopped
    MobileMusicPlaybackStateDto.BUFFERING -> R.string.music_state_buffering
    MobileMusicPlaybackStateDto.INTERRUPTED -> R.string.music_state_interrupted
    MobileMusicPlaybackStateDto.UNAUTHORIZED -> R.string.music_state_unauthorized
    MobileMusicPlaybackStateDto.UNAVAILABLE -> R.string.music_state_unavailable
    MobileMusicPlaybackStateDto.DISCONNECTED -> R.string.music_state_disconnected
    MobileMusicPlaybackStateDto.STALE -> R.string.music_state_stale
}

private fun MobileMusicProviderDto.providerTitle(context: Context): String = when (this) {
    MobileMusicProviderDto.APPLE_MUSIC -> context.getString(R.string.music_apple_music)
    MobileMusicProviderDto.SPOTIFY -> context.getString(R.string.music_spotify)
    MobileMusicProviderDto.SOUND_CLOUD -> context.getString(R.string.music_soundcloud)
}

private fun Context.musicActivity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.musicActivity()
    else -> null
}

private fun MobileMusicRideEventKindDto.timelineTitle(context: Context): String = when (this) {
    MobileMusicRideEventKindDto.PLAY -> context.getString(R.string.music_timeline_play)
    MobileMusicRideEventKindDto.PAUSE -> context.getString(R.string.music_timeline_pause)
    MobileMusicRideEventKindDto.SKIP -> context.getString(R.string.music_timeline_skip)
    MobileMusicRideEventKindDto.ITEM_CHANGED -> context.getString(R.string.music_timeline_item_changed)
    MobileMusicRideEventKindDto.STOPPED -> context.getString(R.string.music_timeline_stopped)
    MobileMusicRideEventKindDto.PROVIDER_DISCONNECTED -> context.getString(R.string.music_timeline_disconnected)
}
