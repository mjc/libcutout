package lol.cutout

import android.content.Context
import android.icu.number.NumberFormatter
import android.icu.number.Precision
import android.icu.util.MeasureUnit
import java.util.Locale

/** Presentation of canonical Rust totals shared by Live, history, and notifications. */
internal object RideMetricFormatting {
    fun distance(context: Context, meters: Double): String {
        val locale = context.resources.configuration.locales[0]
        return NumberFormatter.withLocale(locale).unit(MeasureUnit.METER)
            .usage("road").precision(Precision.maxFraction(1)).format(meters).toString()
    }

    fun duration(milliseconds: ULong): String {
        val seconds = milliseconds / 1000u
        return if (seconds >= 3600u) String.format(Locale.getDefault(), "%d:%02d:%02d", (seconds / 3600u).toLong(),
            (seconds % 3600u / 60u).toLong(), (seconds % 60u).toLong()) else
            String.format(Locale.getDefault(), "%d:%02d", (seconds / 60u).toLong(), (seconds % 60u).toLong())
    }
}
