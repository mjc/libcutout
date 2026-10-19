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

/** Compact live player; only commands advertised by the current Rust-projected snapshot render. */
class MusicCompactPlayer(
    context: Context,
    private val client: AndroidMusicClient,
    private val ui: CutoutUi = CutoutUi(context),
    private val onOpenDetails: () -> Unit = {},
    private val onDismiss: () -> Unit = {},
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
        artwork.setImageBitmap(state.artwork)
        val spotifyUrl = snapshot?.takeIf { it.provider == MobileMusicProviderDto.SPOTIFY }
            ?.item?.identifier?.let { runCatching { spotifyContentUrl(it) }.getOrNull() }
            ?.takeIf { value -> runCatching { Uri.parse(value).let { it.scheme == "https" && it.host == "open.spotify.com" } }.getOrDefault(false) }
        contentLink.visibility = if (spotifyUrl == null) View.GONE else View.VISIBLE
        contentLink.setOnClickListener {
            spotifyUrl?.let { value -> runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(value))) } }
        }
        controls.removeAllViews()
        if (state.listenerAccessGranted || state.selectedProvider == MobileMusicProviderDto.SOUND_CLOUD) {
            val caps = snapshot?.capabilities
            if (caps != null) {
                if (snapshot?.state == MobileMusicPlaybackStateDto.PLAYING) {
                    if (caps.pause) addCommand("Ⅱ", MobileMusicCommandDto.PAUSE)
                } else if (caps.play) {
                    addCommand("▶", MobileMusicCommandDto.PLAY)
                }
            }
            if (caps?.next == true) addCommand("›", MobileMusicCommandDto.NEXT)
            if (caps != null && !caps.play && !caps.pause && !caps.next && caps.openProvider) {
                addCommand("↗", MobileMusicCommandDto.OPEN_PROVIDER)
            }
        }
        controls.addView(TextView(context).apply {
            text = "×"
            textSize = 20f
            setTextColor(ui.muted)
            gravity = Gravity.CENTER
            minWidth = ui.dp(48)
            minHeight = ui.dp(44)
            contentDescription = context.getString(R.string.music_hide)
            isClickable = true
            isFocusable = true
            setOnClickListener { onDismiss() }
        }, LayoutParams(-2, -1))
    }

    private fun addCommand(title: String, command: MobileMusicCommandDto) {
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
    }
}
