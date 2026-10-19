package lol.cutout.local

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import lol.cutout.ChargingConnectorArtwork
import lol.cutout.CutoutUi
import lol.cutout.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import uniffi.cutout_mobile_ffi.MobileChargingConnectorDto

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = LocalTestApplication::class)
class ChargingConnectorArtworkTest {
    @Test
    fun everyRustConnectorUsesItsSwiftAssetAndAssetsInflateAsNativeVectors() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val connectors = listOf(
            MobileChargingConnectorDto.TypeA, MobileChargingConnectorDto.TypeC,
            MobileChargingConnectorDto.TypeD, MobileChargingConnectorDto.TypeE,
            MobileChargingConnectorDto.TypeF, MobileChargingConnectorDto.TypeG,
            MobileChargingConnectorDto.TypeH, MobileChargingConnectorDto.TypeI,
            MobileChargingConnectorDto.TypeJ, MobileChargingConnectorDto.TypeK,
            MobileChargingConnectorDto.TypeL, MobileChargingConnectorDto.TypeM,
            MobileChargingConnectorDto.TypeN, MobileChargingConnectorDto.TypeO,
            MobileChargingConnectorDto.Nema515, MobileChargingConnectorDto.Nema520,
            MobileChargingConnectorDto.Nema1430, MobileChargingConnectorDto.Nema1450,
            MobileChargingConnectorDto.Nema1030, MobileChargingConnectorDto.Nema650,
            MobileChargingConnectorDto.Nema1050, MobileChargingConnectorDto.J1772,
            MobileChargingConnectorDto.Type2, MobileChargingConnectorDto.Ccs1,
            MobileChargingConnectorDto.Ccs2, MobileChargingConnectorDto.Chademo,
            MobileChargingConnectorDto.Nacs, MobileChargingConnectorDto.Unknown,
            MobileChargingConnectorDto.Other("custom label"),
        )
        val resources = connectors.map(ChargingConnectorArtwork::drawable)
        assertEquals(28, resources.distinct().size)
        assertEquals(R.drawable.charging_connector_unknown,
            ChargingConnectorArtwork.drawable(MobileChargingConnectorDto.Other("custom label")))
        val ui = CutoutUi(context)
        assertEquals("Type 2", ChargingConnectorArtwork.label(ui, MobileChargingConnectorDto.Type2))
        assertEquals("custom label", ChargingConnectorArtwork.label(ui, MobileChargingConnectorDto.Other("custom label")))
        assertEquals("Unknown connector", ChargingConnectorArtwork.label(ui, MobileChargingConnectorDto.Unknown))

        resources.distinct().forEach { resource ->
            val drawable = context.getDrawable(resource)
            assertTrue("Drawable $resource should inflate", drawable != null)
            assertEquals(24, drawable!!.intrinsicWidth)
            assertEquals(24, drawable.intrinsicHeight)
        }
    }
}
