package lol.cutout

import android.content.Context
import android.content.Intent
import android.content.res.ColorStateList
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.RippleDrawable
import android.net.Uri
import android.text.Editable
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.TextView
import uniffi.cutout_mobile_ffi.MobileSoundCloudCommand
import uniffi.cutout_mobile_ffi.MobileSoundCloudState

/** Search, select, and control the Rust-owned SoundCloud queue. */
class AndroidSoundCloudPanel(
    context: Context,
    private val player: AndroidSoundCloudPlayer,
    private val ui: CutoutUi = CutoutUi(context),
) : LinearLayout(context) {
    private val query: EditText = EditText(context).apply {
        hint = context.getString(R.string.music_soundcloud_search)
        setSingleLine(true)
        imeOptions = EditorInfo.IME_ACTION_SEARCH
        setTextColor(ui.foreground)
        setHintTextColor(ui.muted)
        setBackgroundColor(Color.TRANSPARENT)
        setPadding(0, ui.dp(10), 0, ui.dp(10))
        tag = "music.soundcloud.search-field"
        contentDescription = context.getString(R.string.music_soundcloud_search)
        setOnEditorActionListener { _, actionId, _ ->
            if (actionId == EditorInfo.IME_ACTION_SEARCH) {
                searchCurrentQuery()
                true
            } else false
        }
    }
    private val searchButton = ui.text(context.getString(R.string.music_soundcloud_search), 17f, true, ui.brand).apply {
        gravity = Gravity.CENTER_VERTICAL
        minHeight = ui.dp(44)
        setPadding(ui.dp(4), 0, ui.dp(8), 0)
        isClickable = true
        isFocusable = true
        tag = "music.soundcloud.search-button"
        contentDescription = context.getString(R.string.music_soundcloud_search)
        setOnClickListener { searchCurrentQuery() }
        accessibilityDelegate = buttonRole()
    }
    private val searchSpinner = ProgressBar(context).apply {
        isIndeterminate = true
        visibility = View.GONE
        contentDescription = context.getString(R.string.music_soundcloud_searching)
    }
    private val searchProgressText = ui.text(context.getString(R.string.music_soundcloud_searching), 14f, color = ui.muted)
    private val searchProgress = ui.row().apply {
        gravity = Gravity.CENTER_VERTICAL
        visibility = View.GONE
        addView(searchSpinner, LayoutParams(ui.dp(20), ui.dp(20)))
        addView(ui.space(0), LayoutParams(ui.dp(8), 1))
        addView(searchProgressText, LayoutParams(-2, -2))
    }
    private val error = ui.text("", 14f, color = ui.red).apply {
        tag = "music.soundcloud.error"
        visibility = View.GONE
    }
    private val currentSection = section().apply { visibility = View.GONE }
    private val currentTitle = ui.text("", 20f, true).apply { tag = "music.soundcloud.current-title" }
    private val currentUploader = ui.text("", 15f, color = ui.muted).apply { tag = "music.soundcloud.current-uploader" }
    private val currentElapsed = ui.text("", 14f, color = ui.muted).apply {
        tag = "music.soundcloud.elapsed"
        setFontFeatureSettings("tnum")
        contentDescription = context.getString(R.string.music_soundcloud_elapsed, 0, 0)
    }
    private val trackLink = ui.text(context.getString(R.string.music_soundcloud_track_link), 15f, color = ui.brand).apply {
        minHeight = ui.dp(44)
        gravity = Gravity.CENTER_VERTICAL
        tag = "music.soundcloud.track-link"
        isClickable = true
        isFocusable = true
        accessibilityDelegate = buttonRole()
        setOnClickListener { openTrackLink() }
    }
    private val controls = ui.row().apply {
        gravity = Gravity.CENTER
        setPadding(0, ui.dp(8), 0, 0)
    }
    private val previousControl = transportControl(
        R.string.music_previous,
        android.R.drawable.ic_media_previous,
        MobileSoundCloudCommand.PREVIOUS,
        "music.soundcloud.command.previous",
    )
    private val playPauseControl = transportControl(
        R.string.music_play,
        android.R.drawable.ic_media_play,
        MobileSoundCloudCommand.PLAY,
        "music.soundcloud.command.play",
    )
    private val nextControl = transportControl(
        R.string.music_next,
        android.R.drawable.ic_media_next,
        MobileSoundCloudCommand.NEXT,
        "music.soundcloud.command.next",
    )
    private val loadingSpinner = ProgressBar(context).apply {
        isIndeterminate = true
        visibility = View.GONE
        contentDescription = context.getString(R.string.music_soundcloud_searching)
    }
    private val resultsSection = section().apply { visibility = View.GONE }
    private val resultsHeader = ui.text(context.getString(R.string.music_soundcloud_search), 15f, true, ui.muted).apply {
        visibility = View.GONE
    }
    private val results = ui.column()
    private var resultsSignature: List<String> = emptyList()
    private var currentPermalink: String? = null
    private var searching = false
    private var observation: java.io.Closeable? = player.observe(::render)

    init {
        orientation = VERTICAL
        tag = "music.soundcloud.player"
        setPadding(ui.dp(16), ui.dp(12), ui.dp(16), ui.dp(20))
        query.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(value: CharSequence?, start: Int, count: Int, after: Int) = Unit
            override fun onTextChanged(value: CharSequence?, start: Int, before: Int, count: Int) = updateSearchButton()
            override fun afterTextChanged(value: Editable?) = Unit
        })

        val searchSection = section()
        val logoRow = ui.row().apply { gravity = Gravity.CENTER }
        val logo = ImageView(context).apply {
            setImageBitmap(BitmapFactory.decodeResource(resources, R.drawable.soundcloud_logo))
            contentDescription = context.getString(R.string.music_soundcloud_brand)
            scaleType = ImageView.ScaleType.FIT_CENTER
            setOnClickListener { openSoundCloudHome() }
            isClickable = true
            isFocusable = true
            background = ripple()
            setPadding(ui.dp(10), ui.dp(8), ui.dp(10), ui.dp(8))
            layoutParams = LayoutParams(ui.dp(140), ui.dp(35))
        }
        logoRow.addView(logo)
        searchSection.addView(logoRow, LayoutParams(-1, ui.dp(44)))
        searchSection.addView(query, LayoutParams(-1, ui.dp(48)))
        searchSection.addView(ui.divider(), LayoutParams(-1, ui.dp(1)))
        val searchRow = ui.row().apply { gravity = Gravity.CENTER_VERTICAL }
        searchRow.addView(searchButton, LayoutParams(0, -2, 1f))
        searchRow.addView(searchProgress, LayoutParams(-2, -2))
        searchSection.addView(searchRow, LayoutParams(-1, -2))
        addView(searchSection, LayoutParams(-1, -2))

        addView(error.apply { setPadding(ui.dp(4), ui.dp(8), ui.dp(4), ui.dp(4)) }, LayoutParams(-1, -2))

        currentSection.addView(currentTitle, LayoutParams(-1, -2))
        currentSection.addView(currentUploader, LayoutParams(-1, -2))
        val detailsRow = ui.row().apply { gravity = Gravity.CENTER_VERTICAL }
        detailsRow.addView(currentElapsed, LayoutParams(-2, -2))
        detailsRow.addView(ui.space(0), LayoutParams(ui.dp(16), 1))
        detailsRow.addView(trackLink, LayoutParams(-2, -2))
        currentSection.addView(detailsRow, LayoutParams(-1, -2))
        controls.addView(previousControl)
        controls.addView(playPauseControl)
        controls.addView(nextControl)
        controls.addView(loadingSpinner, LayoutParams(ui.dp(20), ui.dp(20)).apply {
            marginStart = ui.dp(8)
        })
        currentSection.addView(controls, LayoutParams(-1, -2))
        addView(currentSection, LayoutParams(-1, -2).apply { topMargin = ui.dp(20) })

        resultsSection.addView(resultsHeader, LayoutParams(-1, -2))
        resultsSection.addView(ui.divider(), LayoutParams(-1, ui.dp(1)))
        resultsSection.addView(results, LayoutParams(-1, -2))
        addView(resultsSection, LayoutParams(-1, -2).apply { topMargin = ui.dp(20) })

        render(player.latest)
    }

    private fun section() = ui.column().apply {
        setPadding(ui.dp(16), ui.dp(12), ui.dp(16), ui.dp(12))
        background = ui.rounded(12, ui.card, false)
        clipToOutline = true
    }

    private fun searchCurrentQuery() { player.search(query.text?.toString().orEmpty()) }

    private fun updateSearchButton() {
        searchButton.isEnabled = query.text?.isNotBlank() == true && !searching
        searchButton.alpha = if (searchButton.isEnabled) 1f else 0.5f
    }

    fun render(value: AndroidSoundCloudPresentation) {
        searching = value.isSearching
        searchProgress.visibility = if (value.isSearching) View.VISIBLE else View.GONE
        searchSpinner.visibility = if (value.isSearching) View.VISIBLE else View.GONE
        updateSearchButton()

        error.text = value.error.orEmpty()
        error.visibility = if (value.error.isNullOrBlank()) View.GONE else View.VISIBLE

        val track = value.snapshot.track
        currentSection.visibility = if (track == null) View.GONE else View.VISIBLE
        if (track != null) {
            currentTitle.text = track.title
            currentUploader.text = track.uploader
            currentElapsed.text = context.getString(
                R.string.music_soundcloud_elapsed,
                value.elapsedSeconds / 60,
                value.elapsedSeconds % 60,
            )
            currentElapsed.contentDescription = context.getString(
                R.string.music_soundcloud_elapsed,
                value.elapsedSeconds / 60,
                value.elapsedSeconds % 60,
            )
            currentPermalink = track.permalink.takeIf(::isCanonicalSoundCloudLink)
            trackLink.visibility = if (currentPermalink != null) View.VISIBLE else View.GONE
            trackLink.isEnabled = currentPermalink != null
            val busy = value.snapshot.state == MobileSoundCloudState.LOADING
            val playing = value.snapshot.state == MobileSoundCloudState.PLAYING || busy
            updateControl(previousControl, enabled = value.snapshot.hasPrevious)
            updateControl(nextControl, enabled = value.snapshot.hasNext)
            updateControl(playPauseControl, enabled = true, pause = playing)
            loadingSpinner.visibility = if (busy) View.VISIBLE else View.GONE
        } else {
            currentPermalink = null
        }

        val signature = value.tracks.map { "${it.urn}\u0000${it.title}\u0000${it.uploader}" }
        if (signature != resultsSignature) {
            resultsSignature = signature
            results.removeAllViews()
            value.tracks.forEachIndexed { index, resultTrack ->
                if (index > 0) results.addView(ui.divider(), LayoutParams(-1, ui.dp(1)))
                results.addView(resultRow(resultTrack), LayoutParams(-1, -2))
            }
        }
        resultsSection.visibility = if (value.tracks.isEmpty()) View.GONE else View.VISIBLE
        resultsHeader.visibility = if (value.tracks.isEmpty()) View.GONE else View.VISIBLE
    }

    private fun resultRow(track: uniffi.cutout_mobile_ffi.MobileSoundCloudTrack): View = ui.row().apply {
        gravity = Gravity.CENTER_VERTICAL
        setPadding(ui.dp(12), ui.dp(6), ui.dp(4), ui.dp(6))
        minimumHeight = ui.dp(60)
        background = ripple()
        isClickable = true
        isFocusable = true
        tag = "music.soundcloud.track.${track.urn}"
        contentDescription = context.getString(R.string.music_soundcloud_select_track, track.title, track.uploader)
        accessibilityDelegate = buttonRole()
        val metadata = ui.column()
        metadata.addView(ui.text(track.title, 16f, true), LayoutParams(-1, -2))
        metadata.addView(ui.text(track.uploader, 14f, color = ui.muted), LayoutParams(-1, -2))
        addView(metadata, LayoutParams(0, -2, 1f))
        addView(iconTarget(
            android.R.drawable.ic_media_play,
            context.getString(R.string.music_play),
            "music.soundcloud.track-play.${track.urn}",
            onClick = { player.select(track) },
        ), LayoutParams(ui.dp(44), ui.dp(44)))
        setOnClickListener { player.select(track) }
    }

    private fun transportControl(
        label: Int,
        icon: Int,
        command: MobileSoundCloudCommand,
        tagValue: String,
    ): FrameLayout = iconTarget(icon, context.getString(label), tagValue) { player.command(command) }.apply {
        layoutParams = LayoutParams(ui.dp(44), ui.dp(44)).apply {
            marginStart = ui.dp(12)
            marginEnd = ui.dp(12)
        }
    }

    private fun iconTarget(icon: Int, label: String, tagValue: String, onClick: () -> Unit): FrameLayout = FrameLayout(context).apply {
        minimumWidth = ui.dp(44)
        minimumHeight = ui.dp(44)
        isClickable = true
        isFocusable = true
        background = ripple()
        contentDescription = label
        tag = tagValue
        accessibilityDelegate = buttonRole()
        addView(ImageView(context).apply {
            setImageResource(icon)
            imageTintList = ColorStateList.valueOf(ui.foreground)
            contentDescription = null
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
            layoutParams = FrameLayout.LayoutParams(ui.dp(24), ui.dp(24), Gravity.CENTER)
        })
        setOnClickListener { onClick() }
    }

    private fun updateControl(control: View, enabled: Boolean, pause: Boolean = false) {
        control.isEnabled = enabled
        control.alpha = if (enabled) 1f else 0.45f
        if (control === playPauseControl) {
            val image = (control as FrameLayout).getChildAt(0) as ImageView
            image.setImageResource(if (pause) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play)
            val label = context.getString(if (pause) R.string.music_pause else R.string.music_play)
            control.contentDescription = label
            control.tag = "music.soundcloud.command.${if (pause) "pause" else "play"}"
        }
    }

    private fun openTrackLink() {
        val link = currentPermalink ?: return
        runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(link))) }
            .onFailure { error.text = context.getString(R.string.music_provider_unavailable); error.visibility = View.VISIBLE }
    }

    private fun openSoundCloudHome() {
        runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://soundcloud.com"))) }
            .onFailure { error.text = context.getString(R.string.music_provider_unavailable); error.visibility = View.VISIBLE }
    }

    private fun isCanonicalSoundCloudLink(value: String): Boolean = runCatching {
        val uri = Uri.parse(value)
        uri.scheme == "https" && (uri.host == "soundcloud.com" || uri.host == "www.soundcloud.com")
    }.getOrDefault(false)

    private fun ripple() = RippleDrawable(
        ColorStateList.valueOf(if (ui.dark) 0x22FFFFFF else 0x14000000),
        null,
        null,
    )

    private fun buttonRole() = object : View.AccessibilityDelegate() {
        override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
            super.onInitializeAccessibilityNodeInfo(host, info)
            info.className = android.widget.Button::class.java.name
        }
    }

    fun dispose() {
        observation?.close()
        observation = null
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        if (observation == null) observation = player.observe(::render)
    }

    override fun onDetachedFromWindow() {
        dispose()
        super.onDetachedFromWindow()
    }
}
