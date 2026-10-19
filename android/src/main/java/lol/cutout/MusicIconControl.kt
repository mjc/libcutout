package lol.cutout

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
import android.graphics.Canvas
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.drawable.Drawable
import android.graphics.drawable.RippleDrawable
import android.view.Gravity
import android.view.View
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Button
import android.widget.FrameLayout
import android.widget.ImageView
import uniffi.cutout_mobile_ffi.MobileMusicCommandDto

/** Shared native transport target for compact and expanded live playback. */
internal fun musicIconControl(
    context: Context,
    ui: CutoutUi,
    command: MobileMusicCommandDto,
    onClick: () -> Unit,
): FrameLayout {
    val label = context.getString(when (command) {
        MobileMusicCommandDto.PREVIOUS -> R.string.music_previous
        MobileMusicCommandDto.PLAY -> R.string.music_play
        MobileMusicCommandDto.PAUSE -> R.string.music_pause
        MobileMusicCommandDto.NEXT -> R.string.music_next
        MobileMusicCommandDto.OPEN_PROVIDER -> R.string.music_open_provider
    })
    val prominent = command == MobileMusicCommandDto.PLAY || command == MobileMusicCommandDto.PAUSE
    val fill = if (prominent) Color.argb(30, Color.red(ui.brand), Color.green(ui.brand), Color.blue(ui.brand)) else Color.TRANSPARENT
    return FrameLayout(context).apply {
        tag = "music.command.${command.name.lowercase()}"
        minimumWidth = ui.dp(44)
        minimumHeight = ui.dp(44)
        isClickable = true
        isFocusable = true
        contentDescription = label
        background = RippleDrawable(ColorStateList.valueOf(if (ui.dark) 0x33FFFFFF else 0x22000000),
            ui.rounded(22, fill, outlined = false), null)
        accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
                super.onInitializeAccessibilityNodeInfo(host, info)
                info.className = Button::class.java.name
            }
        }
        addView(ImageView(context).apply {
            if (command == MobileMusicCommandDto.OPEN_PROVIDER) {
                setImageDrawable(CutoutIcon(CutoutSymbol.OPEN_EXTERNAL, ui.foreground))
            } else {
                setImageDrawable(MusicTransportIcon(command, if (prominent) ui.brand else ui.foreground))
            }
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(ui.dp(20), ui.dp(20), Gravity.CENTER))
        setOnClickListener { onClick() }
    }
}

/** Flat filled transport symbols matching the native Swift player, without legacy Android assets. */
private class MusicTransportIcon(private val command: MobileMusicCommandDto, color: Int) : Drawable() {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { this.color = color }

    override fun draw(canvas: Canvas) {
        val saved = canvas.save()
        canvas.translate(bounds.left.toFloat(), bounds.top.toFloat())
        canvas.scale(bounds.width() / 24f, bounds.height() / 24f)
        fun triangle(a: Float, b: Float, c: Float) {
            canvas.drawPath(Path().apply {
                moveTo(a, 4f); lineTo(b, 12f); lineTo(c, 20f); close()
            }, paint)
        }
        when (command) {
            MobileMusicCommandDto.PREVIOUS -> { triangle(12f, 1f, 12f); triangle(23f, 12f, 23f) }
            MobileMusicCommandDto.NEXT -> { triangle(1f, 12f, 1f); triangle(12f, 23f, 12f) }
            MobileMusicCommandDto.PLAY -> triangle(6f, 21f, 6f)
            MobileMusicCommandDto.PAUSE -> {
                canvas.drawRoundRect(5f, 3f, 10f, 21f, 1f, 1f, paint)
                canvas.drawRoundRect(14f, 3f, 19f, 21f, 1f, 1f, paint)
            }
            MobileMusicCommandDto.OPEN_PROVIDER -> Unit
        }
        canvas.restoreToCount(saved)
    }

    override fun setAlpha(alpha: Int) { paint.alpha = alpha }
    override fun setColorFilter(filter: ColorFilter?) { paint.colorFilter = filter }
    @Suppress("DEPRECATION") override fun getOpacity() = PixelFormat.TRANSLUCENT
}
