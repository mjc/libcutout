package lol.cutout

import android.content.Context
import android.content.Intent
import android.view.Gravity
import android.view.View
import android.net.Uri
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import uniffi.cutout_mobile_ffi.*

/** Swift keeps a compact row for a live/transient session, or for a stopped item. */
internal fun AndroidMusicPresentation.showsCompactPlayer(): Boolean {
    val state = snapshot?.state ?: MobileMusicPlaybackStateDto.UNAVAILABLE
    val hasItem = snapshot?.item != null
    return when (state) {
        MobileMusicPlaybackStateDto.PLAYING,
        MobileMusicPlaybackStateDto.PAUSED,
        MobileMusicPlaybackStateDto.BUFFERING,
        MobileMusicPlaybackStateDto.INTERRUPTED -> true
        MobileMusicPlaybackStateDto.STOPPED,
        MobileMusicPlaybackStateDto.DISCONNECTED,
        MobileMusicPlaybackStateDto.STALE -> hasItem
        MobileMusicPlaybackStateDto.UNAVAILABLE -> hasItem && snapshot?.capabilities?.play == true
        MobileMusicPlaybackStateDto.UNAUTHORIZED -> false
    }
}

/** Applies the same playback-state gates Swift uses before showing provider commands. */
internal fun AndroidMusicPresentation.availableCommands(
    includePrevious: Boolean,
    includeOpenProvider: Boolean,
): List<MobileMusicCommandDto> {
    val current = this.snapshot ?: return emptyList()
    if (!listenerAccessGranted && selectedProvider != MobileMusicProviderDto.SOUND_CLOUD) return emptyList()
    val caps = current.capabilities
    val state = current.state
    val commands = mutableListOf<MobileMusicCommandDto>()
    if (includePrevious && state in setOf(MobileMusicPlaybackStateDto.PLAYING, MobileMusicPlaybackStateDto.PAUSED) && caps.previous) {
        commands += MobileMusicCommandDto.PREVIOUS
    }
    when (state) {
        MobileMusicPlaybackStateDto.PLAYING, MobileMusicPlaybackStateDto.BUFFERING ->
            if (caps.pause) commands += MobileMusicCommandDto.PAUSE
        MobileMusicPlaybackStateDto.PAUSED, MobileMusicPlaybackStateDto.STOPPED,
        MobileMusicPlaybackStateDto.DISCONNECTED, MobileMusicPlaybackStateDto.STALE,
        MobileMusicPlaybackStateDto.UNAVAILABLE -> if (caps.play) commands += MobileMusicCommandDto.PLAY
        MobileMusicPlaybackStateDto.INTERRUPTED, MobileMusicPlaybackStateDto.UNAUTHORIZED -> Unit
    }
    if (state in setOf(MobileMusicPlaybackStateDto.PLAYING, MobileMusicPlaybackStateDto.PAUSED) && caps.next) {
        commands += MobileMusicCommandDto.NEXT
    }
    if (includeOpenProvider && commands.none { it == MobileMusicCommandDto.PLAY || it == MobileMusicCommandDto.PAUSE || it == MobileMusicCommandDto.NEXT } && caps.openProvider) {
        commands += MobileMusicCommandDto.OPEN_PROVIDER
    }
    return commands
}

/** Insert newly available controls without moving existing accessibility targets. */
internal fun musicCommandInsertionIndex(
    parent: LinearLayout,
    existing: Map<MobileMusicCommandDto, View>,
    command: MobileMusicCommandDto,
): Int {
    val order = listOf(MobileMusicCommandDto.PREVIOUS, MobileMusicCommandDto.PLAY,
        MobileMusicCommandDto.PAUSE, MobileMusicCommandDto.NEXT, MobileMusicCommandDto.OPEN_PROVIDER)
    val next = order.dropWhile { it != command }.drop(1)
        .firstNotNullOfOrNull { existing[it]?.takeIf { view -> view.parent === parent } }
    return next?.let(parent::indexOfChild) ?: parent.childCount
}

/** Compact live player; only commands advertised by the current Rust-projected snapshot render. */
class MusicCompactPlayer(
    context: Context,
    private val client: AndroidMusicClient,
    private val ui: CutoutUi = CutoutUi(context),
    private val onOpenDetails: () -> Unit = {},
) : LinearLayout(context) {
    private val largeText = context.resources.configuration.fontScale > 1f
    private val artwork = ImageView(context).apply {
        scaleType = ImageView.ScaleType.CENTER_CROP
        background = ui.rounded(6, ui.card, false)
        contentDescription = context.getString(R.string.music_artwork)
        clipToOutline = true
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
    }
    private val itemTitle = ui.text(context.getString(R.string.music_nothing_playing), 14f, true).apply {
        tag = "music.now-playing-title"
        maxLines = 1; ellipsize = android.text.TextUtils.TruncateAt.END
        setTextSize(android.util.TypedValue.COMPLEX_UNIT_DIP,
            14f * context.resources.configuration.fontScale.coerceAtMost(1.35f))
    }
    private val itemArtist = ui.text("", 12f, color = ui.muted).apply {
        maxLines = 1; ellipsize = android.text.TextUtils.TruncateAt.END
    }
    private val controls = ui.row()
    private val commandButtons = linkedMapOf<MobileMusicCommandDto, View>()
    private val contentLink = musicIconControl(context, ui, MobileMusicCommandDto.OPEN_PROVIDER) {}.apply {
        tag = "music.spotify-content-link"
        contentDescription = context.getString(R.string.music_spotify_content_link)
        visibility = View.GONE
    }
    private val track = ui.row().apply {
        gravity = Gravity.CENTER_VERTICAL
        tag = "music.expand"
        setOnClickListener { onOpenDetails() }
        isClickable = true
        isFocusable = true
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
        accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                super.onInitializeAccessibilityNodeInfo(host, info)
                info.className = android.widget.Button::class.java.name
            }
        }
        contentDescription = context.getString(R.string.music_expand)
    }

    init {
        orientation = HORIZONTAL
        tag = "music.compact-player"
        background = ui.rounded(18)
        gravity = Gravity.CENTER_VERTICAL
        setPadding(ui.dp(8), 0, ui.dp(4), 0)
        track.addView(artwork, LayoutParams(ui.dp(32), ui.dp(32)))
        val metadata = ui.column().apply {
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
            gravity = Gravity.CENTER_VERTICAL
            setPadding(ui.dp(10), 0, ui.dp(4), 0)
            addView(itemTitle, LayoutParams(-1, -2))
            addView(itemArtist, LayoutParams(-1, -2))
        }
        track.addView(metadata, LayoutParams(0, -1, 1f))
        addView(track, LayoutParams(0, -1, 1f))
        addView(contentLink, LayoutParams(-2, -1))
        addView(controls, LayoutParams(-2, -1))
    }

    fun render(state: AndroidMusicPresentation) {
        val snapshot = state.snapshot
        val playback = snapshot?.state ?: MobileMusicPlaybackStateDto.UNAVAILABLE
        val status = when (playback) {
            MobileMusicPlaybackStateDto.PLAYING, MobileMusicPlaybackStateDto.PAUSED -> null
            MobileMusicPlaybackStateDto.STOPPED -> if (snapshot?.provider == MobileMusicProviderDto.SPOTIFY && snapshot.item != null)
                String.format(java.util.Locale.getDefault(), ui.label("music.open_to_resume").replace("%@", "%s"), context.getString(R.string.music_spotify))
                else context.getString(playback.settingsStateLabel())
            else -> context.getString(playback.settingsStateLabel())
        }
        val title = snapshot?.item?.title ?: ui.label(
            musicPlaybackTitleKey(snapshot?.state ?: MobileMusicPlaybackStateDto.UNAVAILABLE),
        )
        itemTitle.text = if (largeText) status ?: title else title
        itemArtist.text = if (status == title) snapshot?.item?.artist.orEmpty() else status ?: snapshot?.item?.artist.orEmpty()
        itemArtist.visibility = if (largeText || itemArtist.text.isNullOrBlank()) View.GONE else View.VISIBLE
        val statusLabel = context.getString((snapshot?.state ?: MobileMusicPlaybackStateDto.UNAVAILABLE).settingsStateLabel())
        track.contentDescription = listOfNotNull(
            context.getString(R.string.music_expand),
            snapshot?.item?.title?.takeIf(String::isNotBlank),
            snapshot?.item?.artist?.takeIf(String::isNotBlank),
            statusLabel,
        ).joinToString(", ")
        if (state.artwork == null) artwork.setImageDrawable(CutoutIcon(CutoutSymbol.MUSIC, ui.muted))
        else artwork.setImageBitmap(state.artwork)
        val spotifyUrl = snapshot?.takeIf { it.provider == MobileMusicProviderDto.SPOTIFY }
            ?.item?.identifier?.let { runCatching { spotifyContentUrl(it) }.getOrNull() }
            ?.takeIf { value -> runCatching { Uri.parse(value).let { it.scheme == "https" && it.host == "open.spotify.com" } }.getOrDefault(false) }
        contentLink.visibility = if (spotifyUrl == null) View.GONE else View.VISIBLE
        contentLink.setOnClickListener {
            spotifyUrl?.let { value -> runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(value))) } }
        }
        val visibleCommands = state.availableCommands(includePrevious = false, includeOpenProvider = true).toSet()
        commandButtons.keys.toList().filterNot(visibleCommands::contains).forEach { commandButtons[it]?.visibility = View.GONE }
        visibleCommands.forEach { command -> addCommand(command).visibility = View.VISIBLE }
    }

    private fun addCommand(command: MobileMusicCommandDto): View {
        commandButtons[command]?.let { return it }
        val button = musicIconControl(context, ui, command) { client.command(command) }
        controls.addView(button, musicCommandInsertionIndex(controls, commandButtons, command), LayoutParams(-2, -1))
        commandButtons[command] = button
        return button
    }
}
