package lol.cutout

import android.content.Context
import uniffi.cutout_mobile_ffi.*
import java.util.Locale

/** Shared labels for Rust-normalized telemetry, without reconstructing protocol semantics. */
internal object TelemetryValueFormatting {
    fun current(context: Context, value: BatteryCurrent): String {
        fun amount(milliamps: UInt) = String.format(Locale.getDefault(), "%.1f", milliamps.toDouble() / 1000)
        return when (value) {
            BatteryCurrent.Zero -> "0.0 A"
            is BatteryCurrent.CurrentIn -> context.getString(R.string.pack_current_in, amount(value.milliamps))
            is BatteryCurrent.CurrentOut -> context.getString(R.string.pack_current_out, amount(value.milliamps))
            is BatteryCurrent.Unknown -> amount(value.milliamps) + " A ?"
        }
    }

    fun footpad(ui: CutoutUi, value: MobileFootpadTelemetryDto): String = when (value.contactState) {
        MobileFootpadContactState.NONE -> ui.label("footpad.contact.none")
        MobileFootpadContactState.LEFT -> ui.label("footpad.contact.left")
        MobileFootpadContactState.RIGHT -> ui.label("footpad.contact.right")
        MobileFootpadContactState.BOTH -> ui.label("footpad.contact.both")
        null -> String.format(Locale.getDefault(), ui.label("footpad.state").replace("%lld", "%d"), value.state.toInt())
    }
}
