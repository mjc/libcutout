package lol.cutout

import android.content.Context
import android.content.res.Configuration
import android.content.res.ColorStateList
import android.graphics.*
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.RippleDrawable
import android.view.Gravity
import android.view.View
import android.widget.*
import org.json.JSONObject
import java.util.Locale

/** Native equivalents of PevDashboardColors and the shared SwiftUI surface treatment. */
class CutoutUi(val context: Context) {
    val dark = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
    val background = if (dark) Color.BLACK else Color.WHITE
    val card = Color.parseColor(if (dark) "#1C1C1E" else "#F2F2F7")
    val stroke = Color.parseColor(if (dark) "#38383A" else "#D1D1D6")
    val foreground = if (dark) Color.WHITE else Color.BLACK
    val muted = Color.parseColor(if (dark) "#AEAEB2" else "#636366")
    val yellow = Color.rgb(255, 204, 0)
    val brand = if (dark) yellow else Color.rgb(122, 74, 0)
    val green = Color.rgb(48, 209, 88)
    val orange = Color.rgb(255, 159, 10)
    val red = Color.rgb(255, 69, 58)
    val cyan = Color.rgb(100, 210, 255)
    private val labels by lazy {
        context.resources.openRawResource(R.raw.shared_labels).bufferedReader().use { JSONObject(it.readText()) }
    }
    fun dp(value: Int): Int = (value * context.resources.displayMetrics.density).toInt()
    fun column() = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
    fun row() = LinearLayout(context).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
    fun text(value: String, size: Float = 17f, bold: Boolean = false, color: Int = foreground) = TextView(context).apply {
        text = value; textSize = size; setTextColor(color); includeFontPadding = false
        if (bold) typeface = Typeface.create("sans-serif", Typeface.BOLD)
    }
    fun title(value: String) = text(value, 34f, true).apply { isAccessibilityHeading = true }
    fun rounded(radius: Int = 22, fill: Int = card, outlined: Boolean = true) = GradientDrawable().apply {
        setColor(fill); cornerRadius = dp(radius).toFloat()
        if (outlined) setStroke(dp(1).coerceAtLeast(1), stroke)
    }
    fun action(title: String, primary: Boolean = false, onClick: () -> Unit) = Button(context).apply {
        text = title; textSize = 17f; isAllCaps = false; includeFontPadding = false
        typeface = Typeface.create("sans-serif", Typeface.BOLD)
        setTextColor(if (primary) Color.BLACK else this@CutoutUi.foreground)
        minHeight = dp(48); minimumHeight = dp(48); minWidth = 0; minimumWidth = 0
        setPadding(dp(16), dp(12), dp(16), dp(12))
        elevation = 0f; stateListAnimator = null
        background = RippleDrawable(ColorStateList.valueOf(if (dark) 0x33FFFFFF else 0x22000000),
            rounded(14, if (primary) yellow else card, !primary), null)
        setOnClickListener { onClick() }
    }
    fun navigationRow(title: String, symbol: CutoutSymbol, onClick: () -> Unit) = row().apply {
        minimumHeight = dp(56); setPadding(dp(16), dp(12), dp(16), dp(12))
        background = RippleDrawable(ColorStateList.valueOf(if (dark) 0x22FFFFFF else 0x11000000), null, null)
        addView(icon(symbol, brand, 22))
        addView(text(title).apply { setPadding(dp(12), 0, dp(8), 0) }, LinearLayout.LayoutParams(0, -2, 1f))
        addView(icon(CutoutSymbol.CHEVRON, muted, 16))
        isClickable = true; isFocusable = true; contentDescription = title
        setOnClickListener { onClick() }
        accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun onInitializeAccessibilityNodeInfo(host: View, info: android.view.accessibility.AccessibilityNodeInfo) {
                super.onInitializeAccessibilityNodeInfo(host, info); info.className = Button::class.java.name
            }
        }
        descendantFocusability = android.view.ViewGroup.FOCUS_BLOCK_DESCENDANTS
        for (index in 0 until childCount) getChildAt(index).importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
    }
    fun icon(symbol: CutoutSymbol, color: Int = foreground, size: Int = 24) = ImageView(context).apply {
        setImageDrawable(CutoutIcon(symbol, color)); layoutParams = LinearLayout.LayoutParams(dp(size), dp(size))
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
    }
    fun space(height: Int) = View(context).apply { layoutParams = LinearLayout.LayoutParams(1, dp(height)) }
    fun divider() = View(context).apply { setBackgroundColor(stroke); layoutParams = LinearLayout.LayoutParams(-1, dp(1).coerceAtLeast(1)) }
    fun label(key: String): String {
        for (locale in listOf(Locale.getDefault().toLanguageTag(), Locale.getDefault().language, "en")) {
            val value = labels.optJSONObject(locale)?.optString(key).orEmpty()
            if (value.isNotEmpty()) return value
        }
        return context.getString(R.string.setting_unknown)
    }
}

enum class CutoutSymbol { DEVICES, RIDE, SETTINGS, BLUETOOTH, CHEVRON, CLOSE, INFO, GEAR, SIGNAL, MAP, MORE, CAMERA, LIGHTING, PACK, MUSIC, POSITION, FIT_ROUTE, BELL, OPEN_EXTERNAL }

/** Small native stroke icons keep navigation independent of a platform emoji/font. */
class CutoutIcon(private val symbol: CutoutSymbol, private val tint: Int) : Drawable() {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = tint; style = Paint.Style.STROKE; strokeWidth = 1.8f; strokeCap = Paint.Cap.ROUND; strokeJoin = Paint.Join.ROUND }
    override fun draw(canvas: Canvas) {
        val save = canvas.save()
        canvas.translate(bounds.left.toFloat(), bounds.top.toFloat())
        canvas.scale(bounds.width() / 24f, bounds.height() / 24f)
        fun path(vararg points: Float) { val path = Path(); path.moveTo(points[0], points[1]); for (i in 2 until points.size step 2) path.lineTo(points[i], points[i + 1]); canvas.drawPath(path, paint) }
        when (symbol) {
            CutoutSymbol.OPEN_EXTERNAL -> { path(10f, 4f, 4f, 4f, 4f, 20f, 20f, 20f, 20f, 14f); path(13f, 3f, 21f, 3f, 21f, 11f); path(10f, 14f, 21f, 3f) }
            CutoutSymbol.BELL -> {
                canvas.drawArc(6f, 3f, 18f, 15f, 180f, 180f, false, paint)
                path(6f, 9f, 6f, 15f, 4f, 18f, 20f, 18f, 18f, 15f, 18f, 9f)
                canvas.drawArc(9f, 18f, 15f, 22f, 0f, 180f, false, paint)
            }
            CutoutSymbol.POSITION -> path(3f, 11f, 21f, 3f, 13f, 21f, 11f, 13f, 3f, 11f)
            CutoutSymbol.FIT_ROUTE -> { path(3f, 9f, 3f, 3f, 9f, 3f); path(15f, 3f, 21f, 3f, 21f, 9f); path(21f, 15f, 21f, 21f, 15f, 21f); path(9f, 21f, 3f, 21f, 3f, 15f) }
            CutoutSymbol.MAP -> { path(3f, 5f, 9f, 3f, 15f, 5f, 21f, 3f, 21f, 19f, 15f, 21f, 9f, 19f, 3f, 21f, 3f, 5f); path(9f, 3f, 9f, 19f); path(15f, 5f, 15f, 21f) }
            CutoutSymbol.MORE -> { for (x in listOf(5f, 12f, 19f)) canvas.drawCircle(x, 12f, 1.3f, paint) }
            CutoutSymbol.CAMERA -> { canvas.drawRoundRect(2f, 6f, 16f, 19f, 3f, 3f, paint); path(16f, 10f, 22f, 6f, 22f, 19f, 16f, 15f) }
            CutoutSymbol.LIGHTING -> { canvas.drawOval(6f, 2f, 18f, 16f, paint); path(8f, 15f, 10f, 21f, 14f, 21f, 16f, 15f); path(7f, 11f, 17f, 11f) }
            CutoutSymbol.PACK -> { canvas.drawRoundRect(2f, 6f, 20f, 18f, 3f, 3f, paint); path(23f, 10f, 23f, 14f); path(6f, 9f, 6f, 15f); path(10f, 9f, 10f, 15f); path(14f, 9f, 14f, 15f) }
            CutoutSymbol.MUSIC -> { path(9f, 17f, 9f, 5f, 20f, 2f, 20f, 14f); canvas.drawOval(3f, 15f, 9f, 20f, paint); canvas.drawOval(14f, 12f, 20f, 17f, paint) }
            CutoutSymbol.CHEVRON -> path(9f, 5f, 16f, 12f, 9f, 19f)
            CutoutSymbol.CLOSE -> { path(6f, 6f, 18f, 18f); path(18f, 6f, 6f, 18f) }
            CutoutSymbol.BLUETOOTH -> { path(12f, 2f, 12f, 22f, 19f, 16f, 5f, 6f); path(12f, 2f, 19f, 8f, 5f, 18f) }
            CutoutSymbol.DEVICES -> { canvas.drawRoundRect(5f, 3f, 19f, 21f, 4f, 4f, paint); path(9f, 17f, 15f, 17f); path(10f, 6f, 14f, 6f) }
            CutoutSymbol.RIDE -> { canvas.drawCircle(12f, 12f, 9f, paint); path(12f, 12f, 17f, 7f); path(5f, 13f, 7f, 13f); path(17f, 13f, 19f, 13f); path(12f, 5f, 12f, 7f) }
            CutoutSymbol.SETTINGS -> { for ((y, x) in listOf(6f to 8f, 12f to 16f, 18f to 10f)) { path(3f, y, x - 2f, y); path(x + 2f, y, 21f, y); canvas.drawCircle(x, y, 2f, paint) } }
            CutoutSymbol.GEAR -> { canvas.drawCircle(12f, 12f, 4f, paint); for (i in 0..7) { val a = i * Math.PI / 4; path((12 + 7 * kotlin.math.cos(a)).toFloat(), (12 + 7 * kotlin.math.sin(a)).toFloat(), (12 + 10 * kotlin.math.cos(a)).toFloat(), (12 + 10 * kotlin.math.sin(a)).toFloat()) }; canvas.drawCircle(12f, 12f, 7f, paint) }
            CutoutSymbol.INFO -> { canvas.drawCircle(12f, 12f, 9f, paint); path(12f, 11f, 12f, 17f); canvas.drawPoint(12f, 7f, paint) }
            CutoutSymbol.SIGNAL -> { path(5f, 20f, 5f, 16f); path(10f, 20f, 10f, 12f); path(15f, 20f, 15f, 8f); path(20f, 20f, 20f, 4f) }
        }
        canvas.restoreToCount(save)
    }
    override fun setAlpha(alpha: Int) { paint.alpha = alpha }
    override fun setColorFilter(filter: ColorFilter?) { paint.colorFilter = filter }
    @Suppress("DEPRECATION") override fun getOpacity() = PixelFormat.TRANSLUCENT
}
