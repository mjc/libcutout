package lol.cutout

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
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
                setImageResource(when (command) {
                    MobileMusicCommandDto.PREVIOUS -> android.R.drawable.ic_media_previous
                    MobileMusicCommandDto.PLAY -> android.R.drawable.ic_media_play
                    MobileMusicCommandDto.PAUSE -> android.R.drawable.ic_media_pause
                    MobileMusicCommandDto.NEXT -> android.R.drawable.ic_media_next
                    MobileMusicCommandDto.OPEN_PROVIDER -> error("handled above")
                })
                imageTintList = ColorStateList.valueOf(if (prominent) ui.brand else ui.foreground)
            }
            importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
        }, FrameLayout.LayoutParams(ui.dp(20), ui.dp(20), Gravity.CENTER))
        setOnClickListener { onClick() }
    }
}
