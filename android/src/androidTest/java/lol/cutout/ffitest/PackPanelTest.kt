package lol.cutout.ffitest

import android.content.Intent
import android.view.View
import android.view.ViewGroup
import android.widget.Button
import android.widget.TextView
import androidx.test.platform.app.InstrumentationRegistry
import lol.cutout.MainActivity
import lol.cutout.PackPanel
import lol.cutout.R
import org.junit.Test
import uniffi.cutout_mobile_ffi.*

/** Pack screens consume Rust DTOs; synthetic snapshots exercise layout only and never perform device IO. */
open class PackPanelTest {
    @Test
    fun missingReadbackIsNotPresentedAsAnEmptyCellMap() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            instrumentation.runOnMainSync {
                val panel = PackPanel(activity)
                activity.setContentView(panel)
                check(texts(panel).contains(activity.getString(R.string.pack_no_readback)))
                check(!texts(panel).contains(activity.getString(R.string.pack_no_cells))) {
                    "No live snapshot must not be described as a reported empty pack"
                }
                panel.render(productionReplayPresentation())
                check(texts(panel).contains(activity.getString(R.string.pack_no_readback)))

                val verified = productionReplayPresentation()
                val base = snapshot().copy(groups = emptyList())
                panel.render(verified.copy(bms = base.copy(availability = MobileReadbackAvailabilityDto.UNSUPPORTED)))
                check(texts(panel).contains(activity.getString(R.string.pack_unsupported)))
                panel.render(verified.copy(bms = base.copy(availability = MobileReadbackAvailabilityDto.UNAVAILABLE)))
                check(texts(panel).contains(activity.getString(R.string.pack_unavailable)))
                panel.render(verified.copy(bms = base.copy(
                    availability = MobileReadbackAvailabilityDto.AVAILABLE,
                    topology = MobileBmsTopologyDto("Unknown", null, null, 0u, 0u, MobileBmsTopologyConfidenceDto.UNVERIFIED),
                )))
                check(texts(panel).contains(activity.getString(R.string.pack_unknown_topology)))
            }
        } finally { instrumentation.runOnMainSync { activity.finish() } }
    }

    @Test
    fun liveRustGroupValuesRenderHistoryResistanceAndNeighborNavigation() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            instrumentation.runOnMainSync {
                val replay = productionReplayPresentation()
                check(replay.session?.connection?.readiness == MobileConnectionReadinessDto.VERIFIED) {
                    "Production replay must establish a verified shell connection before BMS presentation is tested"
                }
                val descriptor = replay.copy(bms = snapshot())
                val panel = PackPanel(activity)
                activity.setContentView(panel)
                panel.render(descriptor)
                val first = panel.findViewWithTag<Button>("pack.group.1")
                    ?: error("The BMS map did not render its first observed group")
                first.performClick()
                check(texts(panel).contains(activity.getString(R.string.pack_resistance)))
                check(texts(panel).contains(activity.getString(R.string.pack_recent_history)))
                check(texts(panel).contains("3.670–3.680 V"))
                check(texts(panel).contains("3.700 V"))
                check(buttons(panel).any { it.text == activity.getString(R.string.pack_next) })
                buttons(panel).single { it.text == activity.getString(R.string.pack_next) }.performClick()
                check(texts(panel).contains(activity.getString(R.string.pack_group, 2)))
                check(buttons(panel).any { it.text == activity.getString(R.string.pack_previous) })
            }
        } finally { instrumentation.runOnMainSync { activity.finish() } }
    }

    @Test
    fun sameShapeBmsReadingsUpdateExistingMetricViews() {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val activity = instrumentation.startActivitySync(Intent(instrumentation.targetContext, MainActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        try {
            instrumentation.runOnMainSync {
                val panel = PackPanel(activity)
                activity.setContentView(panel)
                val replay = productionReplayPresentation()
                fun voltage(value: Int) = VoltageReading(
                    Voltage(value), MobileValueSourceDto.REPORTED,
                    MobileValueQualityDto.KNOWN, MobileVerificationStatusDto.SOURCE_VERIFIED,
                )
                panel.render(replay.copy(bms = snapshot().copy(voltage = voltage(48_000))))
                val value = checkNotNull(panel.findViewWithTag<TextView>("pack.metric.${activity.getString(R.string.voltage)}"))
                val initial = value.text.toString()
                panel.render(replay.copy(bms = snapshot().copy(voltage = voltage(49_250))))
                check(panel.findViewWithTag<TextView>("pack.metric.${activity.getString(R.string.voltage)}") === value) {
                    "A same-shape BMS update rebuilt the screen hierarchy"
                }
                check(value.text.toString() == "49.25 V" && value.text.toString() != initial) {
                    "The existing metric view did not update to the latest Rust reading"
                }
            }
        } finally { instrumentation.runOnMainSync { activity.finish() } }
    }

    private fun snapshot(): MobileBmsSnapshotDto {
        fun reading(mv: Int) = VoltageReading(Voltage(mv), MobileValueSourceDto.REPORTED,
            MobileValueQualityDto.KNOWN, MobileVerificationStatusDto.SOURCE_VERIFIED)
        fun group(index: Int, mv: Int, latest: Int, resistance: Int) = MobileBmsGroupSnapshotDto(
            index.toUShort(), 1u, index.toUShort(), null, reading(mv), Voltage(latest),
            listOf(Voltage(mv - 10), Voltage(mv)), listOf(1uL, 2uL), null, Resistance(resistance.toUShort()),
            false, MobileBmsAlertLevelDto.NOMINAL, null,
        )
        return MobileBmsSnapshotDto(
            2u, 2u, MobileReadbackAvailabilityDto.AVAILABLE,
            MobileBmsTopologyDto("2S verified pack", 2u, 1u, 1u, 1u, MobileBmsTopologyConfidenceDto.VERIFIED),
            null, null, null, null, null, null, null, null, null,
            VoltageDeltaReading(VoltageDelta(10), MobileValueSourceDto.CALCULATED,
                MobileValueQualityDto.KNOWN, MobileVerificationStatusDto.SOURCE_VERIFIED),
            1u, null, emptyList(), null, null, null, null, null,
            listOf(group(1, 3680, 3700, 12), group(2, 3690, 3690, 14)), emptyList(), emptyList(), null, null,
        )
    }

    private fun texts(root: View): List<String> = descendants(root).filterIsInstance<TextView>().map { it.text.toString() }.toList()
    private fun buttons(root: View): List<Button> = descendants(root).filterIsInstance<Button>().toList()
    private fun descendants(root: View): Sequence<View> = sequence {
        yield(root)
        if (root is ViewGroup) for (i in 0 until root.childCount) yieldAll(descendants(root.getChildAt(i)))
    }
}
