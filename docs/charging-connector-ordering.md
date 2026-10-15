# Connector menu ordering

The charging editor ranks its complete fixed connector catalogue from the rider's GPS-derived country. A manually selected country controls national-default suggestions separately. Device language, locale and the pin's coordinates do not determine menu order.

The mapping is compiled into `cutout-ride-maps` and requires no directory request. Country resolution uses the existing native reverse-geocoding path; that lookup may require Apple services. If location or country resolution is unavailable, malformed or unsupported, the menu uses its general catalogue order.

The initial mapping uses coarse groups: local household outlets, regional EV connectors, then the remaining catalogue. It does not estimate market share or rank connectors within a group. Stable sorting preserves the catalogue order for ties. Unknown stays available at the end, and custom connector labels retain their separate editor control. Reordering does not select, confirm or modify a supply.

Household groups cover US/Canada, Mexico/Japan, Britain/Ireland, Germany/Austria, France/Belgium, Australia/New Zealand, Brazil and India. Regional EV groups cover US/Canada and the listed European countries. Other countries retain the general order until their mapping is added. Subnational prevalence and live usage statistics are outside this initial mapping.

The household groups follow the manufacturer's [Bel world plug guide](https://www.belfuse.com/resource-library/tech-paper/world-plug-and-input-voltage-guide). NEMA 5-15 remains alongside Type A/B in the US/Canada group, matching the existing national-default connector vocabulary. The regional EV groups follow [Quebec's connector guide](https://www.quebec.ca/en/transports/electric-transportation/charging/connectors) and [ChargePoint's regional connector guide](https://www.chargepoint.com/en-gb/drivers/support/faqs/what-are-different-public-ev-charging-connector-types). These sources identify regional connector families; they do not establish exact prevalence percentages. Legacy CHAdeMO remains available in the general group.

The native menu renders the Rust-ordered array directly, so visible and accessibility encounter order share the same source. Location context used for ranking is not part of a charging-spot write or sync payload.

Behavior tests cover representative local prefixes, regional EV priority, stable ties, full catalogue preservation, malformed/missing/unsupported location, the Swift FFI boundary, separation from manual suggestion country and preservation of explicit supplies.
