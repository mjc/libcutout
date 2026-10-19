package lol.cutout

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.drawable.RippleDrawable
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Color
import android.view.View
import android.widget.Button
import android.widget.ImageButton
import android.widget.LinearLayout
import android.graphics.drawable.Drawable

/** Native two-segment control matching the charging map/list selector in SwiftUI. */
internal class ChargingDisplayModeSelector(
    context: Context,
    mapLabel: String,
    listLabel: String,
    onSelect: (mapSelected: Boolean) -> Unit,
) : LinearLayout(context) {
    private val ui = CutoutUi(context)
    val mapButton: Button = ui.action(mapLabel, false) { onSelect(true) }.apply {
        tag = "charging_view_map"
        minWidth = ui.dp(64)
        minHeight = ui.dp(44)
        minimumHeight = ui.dp(44)
        textSize = 14f
        setPadding(0, 0, 0, 0)
    }
    val listButton: Button = ui.action(listLabel, false) { onSelect(false) }.apply {
        tag = "charging_view_list"
        minWidth = ui.dp(64)
        minHeight = ui.dp(44)
        minimumHeight = ui.dp(44)
        textSize = 14f
        setPadding(0, 0, 0, 0)
    }

    init {
        orientation = HORIZONTAL
        gravity = android.view.Gravity.CENTER_VERTICAL
        setPadding(ui.dp(2), 0, ui.dp(2), 0)
        minimumWidth = ui.dp(132)
        background = ui.rounded(24, ui.card, outlined = false).also {
            it.setStroke(ui.dp(1), ui.stroke)
        }
        addView(mapButton, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        addView(listButton, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        selectMap(true)
    }

    fun selectMap(value: Boolean) {
        style(mapButton, selected = value, label = mapButton.text.toString())
        style(listButton, selected = !value, label = listButton.text.toString())
    }

    private fun style(button: Button, selected: Boolean, label: String) {
        button.isSelected = selected
        button.setTextColor(ui.foreground)
        button.background = RippleDrawable(
            ColorStateList.valueOf(if (ui.dark) 0x22FFFFFF else 0x22000000),
            android.graphics.drawable.InsetDrawable(
                ui.rounded(22, if (selected) ui.background else android.graphics.Color.TRANSPARENT, outlined = false),
                0, ui.dp(6), 0, ui.dp(6),
            ),
            null,
        )
        button.contentDescription = if (selected) "$label, ${context.getString(R.string.charging_selected)}" else label
        button.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
    }
}

/** Shared production toolbar assembly so compact and accessibility layouts stay testable. */
internal class ChargingToolbar(
    context: Context,
    val selector: ChargingDisplayModeSelector,
    filterButton: ImageButton,
    addButton: ImageButton,
    moreButton: ImageButton,
) : LinearLayout(context) {
    init {
        if (resources.configuration.fontScale >= 1.3f) {
            orientation = VERTICAL
            addView(selector, LayoutParams(-1, LayoutParams.WRAP_CONTENT))
            val actions = LinearLayout(context).apply {
                orientation = HORIZONTAL
                addView(filterButton, LayoutParams(dp(context, 44), dp(context, 44)))
                addView(View(context), LayoutParams(0, dp(context, 1), 1f))
                addView(addButton, LayoutParams(dp(context, 44), dp(context, 44)))
                addView(moreButton, LayoutParams(dp(context, 44), dp(context, 44)).apply {
                    marginStart = dp(context, 4)
                })
            }
            addView(actions, LayoutParams(-1, LayoutParams.WRAP_CONTENT).apply { topMargin = dp(context, 4) })
        } else {
            orientation = HORIZONTAL
            addView(selector, LayoutParams(0, LayoutParams.WRAP_CONTENT, 2f))
            addView(filterButton, LayoutParams(dp(context, 44), dp(context, 44)).apply {
                marginStart = dp(context, 8)
            })
            addView(addButton, LayoutParams(dp(context, 44), dp(context, 44)).apply {
                marginStart = dp(context, 8)
            })
            addView(moreButton, LayoutParams(dp(context, 44), dp(context, 44)).apply {
                marginStart = dp(context, 4)
            })
        }
    }

    private fun dp(context: Context, value: Int): Int =
        (value * context.resources.displayMetrics.density).toInt()
}

internal enum class ChargingToolbarGlyph { FILTER, PLUS, MORE }

internal fun chargingToolbarIconButton(
    context: Context,
    ui: CutoutUi,
    glyph: ChargingToolbarGlyph,
    description: String,
    tagValue: String,
    onClick: () -> Unit,
): ImageButton = ImageButton(context).apply {
    tag = tagValue
    contentDescription = description
    importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
    isClickable = true
    isFocusable = true
    setPadding(0, 0, 0, 0)
    minimumWidth = ui.dp(44)
    minimumHeight = ui.dp(44)
    setImageDrawable(ChargingToolbarGlyphDrawable(glyph, ui.foreground, ui.dp(24)))
    background = RippleDrawable(
        ColorStateList.valueOf(if (ui.dark) 0x22FFFFFF else 0x22000000),
        android.graphics.drawable.ColorDrawable(Color.TRANSPARENT),
        null,
    )
    setOnClickListener { onClick() }
}

private class ChargingToolbarGlyphDrawable(private val glyph: ChargingToolbarGlyph, color: Int, private val sizePx: Int) : Drawable() {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        this.color = color
        style = Paint.Style.STROKE
        strokeWidth = 1.9f
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }

    override fun draw(canvas: Canvas) {
        val b = bounds
        val sx = b.width() / 24f
        val sy = b.height() / 24f
        val save = canvas.save()
        canvas.translate(b.left.toFloat(), b.top.toFloat())
        canvas.scale(sx, sy)
        when (glyph) {
            ChargingToolbarGlyph.FILTER -> {
                line(canvas, 4f, 6f, 20f, 6f)
                line(canvas, 7f, 12f, 17f, 12f)
                line(canvas, 10f, 18f, 14f, 18f)
            }
            ChargingToolbarGlyph.PLUS -> {
                line(canvas, 12f, 4f, 12f, 20f)
                line(canvas, 4f, 12f, 20f, 12f)
            }
            ChargingToolbarGlyph.MORE -> {
                for (x in listOf(5f, 12f, 19f)) canvas.drawCircle(x, 12f, 1.4f, paint)
            }
        }
        canvas.restoreToCount(save)
    }

    private fun line(canvas: Canvas, x1: Float, y1: Float, x2: Float, y2: Float) {
        val path = Path().apply { moveTo(x1, y1); lineTo(x2, y2) }
        canvas.drawPath(path, paint)
    }

    override fun getIntrinsicWidth(): Int = sizePx
    override fun getIntrinsicHeight(): Int = sizePx
    override fun setAlpha(alpha: Int) { paint.alpha = alpha }
    override fun setColorFilter(filter: android.graphics.ColorFilter?) { paint.colorFilter = filter }
    @Suppress("DEPRECATION") override fun getOpacity(): Int = android.graphics.PixelFormat.TRANSLUCENT
}
