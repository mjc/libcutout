package lol.cutout

import android.content.Context
import uniffi.cutout_mobile_ffi.MobileRideDisplayUnitDto
import uniffi.cutout_mobile_ffi.rideDistanceDisplay
import uniffi.cutout_mobile_ffi.rideDurationText
import uniffi.cutout_mobile_ffi.ridePowerDisplay
import uniffi.cutout_mobile_ffi.rideSpeedDisplay
import kotlin.math.roundToLong

/** Presentation of canonical Rust totals shared by Live, history, and notifications. */
internal object RideMetricFormatting {
    fun distance(context: Context, meters: Double): String {
        val millimetres = when {
            meters.isNaN() || meters.isInfinite() -> 0L
            else -> (meters * 1_000.0).roundToLong()
        }
        val quantity = rideDistanceDisplay(millimetres)
        return "${quantity.text} ${unitWord(context, quantity.unit)}"
    }

    fun duration(milliseconds: ULong): String = rideDurationText(milliseconds)

    fun speed(millimetresPerSecond: Long): String = rideSpeedDisplay(millimetresPerSecond).text

    fun speedUnit(context: Context): String = unitWord(context, rideSpeedDisplay(0).unit)

    fun power(context: Context, milliwatts: Long): String {
        val quantity = ridePowerDisplay(milliwatts)
        return "${quantity.text} ${unitWord(context, quantity.unit)}"
    }

    private fun unitWord(context: Context, unit: MobileRideDisplayUnitDto): String = when (unit) {
        MobileRideDisplayUnitDto.MILES_PER_HOUR -> context.getString(R.string.speed_unit)
        MobileRideDisplayUnitDto.MILES -> context.getString(R.string.distance_unit)
        MobileRideDisplayUnitDto.KILOWATTS -> "kW"
        MobileRideDisplayUnitDto.PERCENT -> "%"
    }
}
