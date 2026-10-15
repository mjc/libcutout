# Charging plug names

The connector picker should use labels a rider can recognize from the outlet
face. Put a short everyday name first and the standardized identifier in
secondary text. Do not use electrical equipment names as connector names:
different outlet shapes can power the same kind of equipment, and the same
outlet can be used for different equipment. A connector name describes the
shape only; it does not establish that an installation is safe or compatible
with a particular vehicle or charger.

## United States

Keep the common household choices recognizable as **US 3 Prong** (NEMA 5-15)
and **US 2 Prong** (Type A). Put US 3 Prong ahead of US 2 Prong in the general
US fallback order because its grounding contact is more useful for PEV
charging. The standardized family may appear in secondary text when the user
needs to identify an exact shape.

Keep each larger US connector as its own choice. Suggested display text:

| Connector ID | Primary label | Secondary identification |
| --- | --- | --- |
| `Nema5_15` | US 3 Prong | NEMA 5-15 · 120 V · 15 A |
| `Nema14_50` | US 4 Prong · 50 A | NEMA 14-50 · 120/240 V · straight neutral |
| `Nema14_30` | US 4 Prong · 30 A | NEMA 14-30 · 120/240 V · L-shaped neutral |
| `Nema10_30` | US 3 Prong · 30 A | NEMA 10-30 · 120/240 V · L-shaped neutral |
| `Nema10_50` | US 3 Prong · 50 A | NEMA 10-50 · 120/240 V · straight neutral |
| `Nema6_50` | US 3 Prong · 50 A · Grounded | NEMA 6-50 · 250 V · U-shaped ground |

The 10-series is an older three-wire, non-grounding configuration for nominal
120/240 V supplies. Its receptacles are rated 125/250 V; the 6-50 is a grounded
250 V configuration. The 10-30 has an L-shaped neutral and the 10-50 has a
straight neutral. These three-contact faces differ from the 6-50. The 14-30
and 14-50 both have four contacts, but their keyed neutral
blades differ as well as their current ratings. Include the secondary
identifier and rating so users can tell each option apart; do not collapse
these into a generic “240 V outlet” choice.
Leviton's product specifications describe the [10-30](https://store.leviton.com/products/30-amp-125-250-volt-surface-mounting-receptacle-straight-blade-nema-10-30r-3p-3w-industrial-grade-non-grounding-side-wired-steel-strap-5054), [10-50](https://leviton.com/products/5050-p00), [6-50](https://leviton.com/products/5378), [14-30](https://leviton.com/products/278-s00), and [14-50](https://leviton.com/products/55050-p00) with these ratings, wire counts, and grounding distinctions.

The 14-50 is a useful first suggestion among these options for PEVs: Tesla
recommends a 240 V 14-50 receptacle for its Mobile Connector, and sells
separate adapters for 14-30, 10-30, and 6-50. These manufacturer materials
support that these are distinct PEV-relevant connector choices, but they do
not establish population prevalence. Do not infer that an outlet supports a
specific charging rate from its reported connector alone.
[Tesla 14-50 guidance](https://www.tesla.com/sites/default/files/pdfs/NEMA_14-50_Overview.pdf),
[Tesla Mobile Connector adapter list](https://www.tesla.com/support/charging/mobile-connector).

## Shape assets

Original monochrome front-face vector drawings live in the app asset catalog at
`swift/CutoutMobile/Apps/CutoutApp/Assets.xcassets/ChargingConnector_*.imageset/`.
They simplify shell proportions while preserving contact count and arrangement
that distinguishes each connector. International Type A through O geometry is
referenced from Bel's [Figure 1 plug-type guide](https://www.belfuse.com/resource-library/tech-paper/world-plug-and-input-voltage-guide).
US household connector geometry is referenced from Leviton's [NEMA 5-15/5-20
receptacle documentation](https://leviton.com/products/5325-w). The added
240 V shapes follow Leviton's [10-30](https://store.leviton.com/products/30-amp-125-250-volt-surface-mounting-receptacle-straight-blade-nema-10-30r-3p-3w-industrial-grade-non-grounding-side-wired-steel-strap-5054),
[10-50](https://leviton.com/products/5050-p00),
[6-50](https://leviton.com/products/5378),
[14-30](https://leviton.com/products/278-s00), and
[14-50](https://leviton.com/products/55050-p00) product-face illustrations
and specifications. Hubbell's [straight-blade receptacle catalog](https://hciapps.hubbell.com/hcipunchout/LiteraturePDFS/3B/Original_BSS57.pdf)
separately lists 6-50 as 250 V and 14-30/14-50 as 125/250 V, three-pole
four-wire grounded receptacles.

EV connector silhouettes are original simplified representations of the
manufacturer's public connector diagrams from [Tesla Mobile Connector support](https://www.tesla.com/support/charging/mobile-connector)
and [ChargePoint's connector guide](https://www.chargepoint.com/en-gb/drivers/support/faqs/what-are-different-public-ev-charging-connector-types).
These small illustrations are recognition aids, not dimensioned engineering
drawings.

## Catalogue ordering

Keep all distinct physical choices and stable ties. When GPS identifies the
US, put **US 3 Prong** (NEMA 5-15) before Type A for PEV usefulness. Keep US
regional EV connectors available in their existing group. If local database
prevalence provides a useful ranking, that measured ordering takes priority;
otherwise use this GPS-country fallback. This is a coarse presentation order,
not market-share data or an electrical compatibility check.
