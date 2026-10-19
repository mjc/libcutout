package lol.cutout

import android.content.Context
import android.content.Intent
import android.graphics.Typeface
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

/** Compact live player; only commands advertised by the current Rust-projected snapshot render. */
class MusicCompactPlayer(
    context: Context,
    private val client: AndroidMusicClient,
    private val ui: CutoutUi = CutoutUi(context),
    private val onOpenDetails: () -> Unit = {},
) : LinearLayout(context) {
    private val artwork = ImageView(context).apply {
        scaleType = ImageView.ScaleType.CENTER_CROP
        background = ui.rounded(6, ui.card, false)
        contentDescription = context.getString(R.string.music_artwork)
    }
    private val itemTitle = ui.text(context.getString(R.string.music_nothing_playing), 14f, true).apply {
        tag = "music.now-playing-title"
        maxLines = 1; ellipsize = android.text.TextUtils.TruncateAt.END
    }
    private val itemArtist = ui.text("", 12f, color = ui.muted).apply {
        maxLines = 1; ellipsize = android.text.TextUtils.TruncateAt.END
    }
    private val controls = ui.row()
    private val commandButtons = linkedMapOf<MobileMusicCommandDto, TextView>()
    private val contentLink = TextView(context).apply {
        text = "↗"
        textSize = 17f
        setTextColor(ui.brand)
        gravity = Gravity.CENTER
        minWidth = ui.dp(44)
        minHeight = ui.dp(44)
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
        itemTitle.text = snapshot?.item?.title ?: ui.label(
            musicPlaybackTitleKey(snapshot?.state ?: MobileMusicPlaybackStateDto.UNAVAILABLE),
        )
        itemArtist.text = snapshot?.item?.artist.orEmpty()
        itemArtist.visibility = if (itemArtist.text.isNullOrBlank()) View.GONE else View.VISIBLE
        val statusLabel = context.getString((snapshot?.state ?: MobileMusicPlaybackStateDto.UNAVAILABLE).settingsStateLabel())
        track.contentDescription = listOfNotNull(
            context.getString(R.string.music_expand),
            snapshot?.item?.title?.takeIf(String::isNotBlank),
            snapshot?.item?.artist?.takeIf(String::isNotBlank),
            statusLabel,
        ).joinToString(", ")
        artwork.setImageBitmap(state.artwork)
        val spotifyUrl = snapshot?.takeIf { it.provider == MobileMusicProviderDto.SPOTIFY }
            ?.item?.identifier?.let { runCatching { spotifyContentUrl(it) }.getOrNull() }
            ?.takeIf { value -> runCatching { Uri.parse(value).let { it.scheme == "https" && it.host == "open.spotify.com" } }.getOrDefault(false) }
        contentLink.visibility = if (spotifyUrl == null) View.GONE else View.VISIBLE
        contentLink.setOnClickListener {
            spotifyUrl?.let { value -> runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(value))) } }
        }
        val visibleCommands = state.availableCommands(includePrevious = false, includeOpenProvider = true).toSet()
        val symbols = mapOf(
            MobileMusicCommandDto.PLAY to "▶", MobileMusicCommandDto.PAUSE to "Ⅱ",
            MobileMusicCommandDto.NEXT to "›", MobileMusicCommandDto.OPEN_PROVIDER to "↗",
        )
        commandButtons.keys.toList().filterNot(visibleCommands::contains).forEach { commandButtons[it]?.visibility = View.GONE }
        visibleCommands.forEach { command -> addCommand(symbols.getValue(command), command).visibility = View.VISIBLE }
    }

    private fun addCommand(title: String, command: MobileMusicCommandDto): TextView {
        commandButtons[command]?.let { return it }
        val label = when (command) {
            MobileMusicCommandDto.PLAY -> context.getString(R.string.music_play)
            MobileMusicCommandDto.PAUSE -> context.getString(R.string.music_pause)
            MobileMusicCommandDto.PREVIOUS -> context.getString(R.string.music_previous)
            MobileMusicCommandDto.NEXT -> context.getString(R.string.music_next)
            MobileMusicCommandDto.OPEN_PROVIDER -> context.getString(R.string.music_open_provider)
        }
        val button = TextView(context).apply {
            tag = "music.command.${command.name.lowercase()}"
            text = title
            textSize = 18f
            typeface = Typeface.create("sans-serif", Typeface.BOLD)
            setTextColor(ui.brand)
            gravity = Gravity.CENTER
            minWidth = ui.dp(48)
            minHeight = ui.dp(44)
            setOnClickListener { client.command(command) }
            isClickable = true
            isFocusable = true
            contentDescription = label
        }
        controls.addView(button, LayoutParams(-2, -1))
        commandButtons[command] = button
        return button
    }
}
