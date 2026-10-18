package lol.cutout.local

import android.app.Activity
import org.robolectric.Robolectric
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.view.Gravity
import android.view.View
import lol.cutout.CutoutUi
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

@RunWith(org.robolectric.RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "w393dp-h852dp-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class NativeTextRenderingTest {
    private val missingInk = mutableListOf<String>()
    @Test
    fun nativeCanvasRendersProductionTextViewsAndBothTextDrawingApis() {
        val controller = Robolectric.buildActivity(Activity::class.java).setup()
        val context = controller.get()
        try {
            val ui = CutoutUi(context)
            val width = 720
            val rowHeight = 64
            val bitmap = Bitmap.createBitmap(width, rowHeight * 5, Bitmap.Config.ARGB_8888)
            val canvas = Canvas(bitmap)
            canvas.drawColor(Color.BLACK)

            val rows = listOf(
                "Battery" to Gravity.LEFT,
                "Power" to Gravity.CENTER_HORIZONTAL,
                "Temperature" to Gravity.RIGHT,
            )
            rows.forEachIndexed { index, (value, alignment) ->
                val text = ui.text(value, 20f, color = Color.WHITE).apply {
                    setSingleLine(true)
                    gravity = alignment or Gravity.CENTER_VERTICAL
                    setBackgroundColor(Color.TRANSPARENT)
                }
                context.setContentView(text)
                controller.visible()
                drawView(text, canvas, 0, index * rowHeight, width, rowHeight)
                assertInk(bitmap, 0, index * rowHeight, width, rowHeight, "TextView $value (attached=${text.isAttachedToWindow}, scroll=${text.scrollX}, layout=${text.layout?.width})")
            }

            val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = Color.WHITE
                textSize = 40f
                typeface = android.graphics.Typeface.create("sans-serif", android.graphics.Typeface.NORMAL)
            }
            val sample = "Battery Power Temperature"
            val baseline = rowHeight * 3f + (rowHeight - (paint.fontMetrics.bottom - paint.fontMetrics.top)) / 2f - paint.fontMetrics.top
            canvas.drawText(sample, 8f, baseline, paint)
            assertInk(bitmap, 0, rowHeight * 3, width, rowHeight, "Canvas.drawText")

            canvas.drawTextRun(
                sample, 0, sample.length, 0, sample.length, 8f, baseline + rowHeight, false, paint,
            )
            assertInk(bitmap, 0, rowHeight * 4, width, rowHeight, "Canvas.drawTextRun")

            val output = File(System.getProperty("cutout.ui.artifacts") ?: "build/test-artifacts/ui")
            check(output.isDirectory || output.mkdirs()) { "Could not create UI artifact directory: $output" }
            try {
                File(output, "native-text-sdk35.png").outputStream().use { stream ->
                    check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, stream)) {
                        "Could not encode native-text-sdk35.png"
                    }
                }
            } finally {
                bitmap.recycle()
            }
            check(missingInk.isEmpty()) { missingInk.joinToString("; ") }
        } finally {
            controller.pause().stop().destroy()
        }
    }

    private fun drawView(view: View, canvas: Canvas, left: Int, top: Int, width: Int, height: Int) {
        view.measure(
            View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY),
        )
        view.layout(0, 0, width, height)
        view.viewTreeObserver.dispatchOnPreDraw()
        val save = canvas.save()
        canvas.translate(left.toFloat(), top.toFloat())
        canvas.clipRect(0, 0, width, height)
        // Parent drawChild normally applies the child scroll; this is a direct root draw.
        canvas.translate(-view.scrollX.toFloat(), -view.scrollY.toFloat())
        view.draw(canvas)
        canvas.restoreToCount(save)
    }

    private fun assertInk(bitmap: Bitmap, left: Int, top: Int, width: Int, height: Int, label: String) {
        var count = 0
        for (y in top until top + height) {
            for (x in left until left + width) {
                if (bitmap.getPixel(x, y) != Color.BLACK) count++
            }
        }
        if (count <= 32) missingInk += "$label rendered no visible text pixels (ink=$count)"
    }
}
