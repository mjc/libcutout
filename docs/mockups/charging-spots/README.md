# Charging spots on Map

Revised design draft, October 12, 2026. [Epic LIBCU-409](https://lific.mjc.lol/LIBCU/issues/LIBCU-409), [plan LIBCU-PLAN-26](https://lific.mjc.lol/LIBCU/plans/127), [mockups DOC-27](https://lific.mjc.lol/LIBCU/pages/170), [pin voting LIBCU-934](https://lific.mjc.lol/LIBCU/issues/LIBCU-934).

Open [index.html](index.html) for the screen set and **Try the flow**. The browser prototype uses illustrative geography/venues and in-memory outcomes. It has no network assets, accounts, durable data or backend writes.

## Dark appearance

Use the **Dark mode** toggle in the screen-set header or open [the dark screen set](index.html?theme=dark). The same prototype state and actions work in both appearances; switching colors does not reset the draft or vote.

[Dark contact sheet](dark-contact-sheet.png), [map and pin voting](dark-browse.png), [minimum form](dark-edit.png), [venue details](dark-coffee.png), [list](dark-list.png), [placement](dark-place.png) and [EV details](dark-ev.png) cover the main screens. Dark versions of the country/EV/second-supply form examples, selected-pin vote and larger-text details use the matching `dark-` filename prefix.

Black page/navigation surfaces, raised iOS-style `#1c1c1e`/`#2c2c2e` panels, readable secondary text, CutOut's bright yellow dark accent, and a dedicated dark map palette follow the current source's dynamic color direction. Free/Paid pins keep readable labels; check/X votes retain selected state and independent symbols/counts. Optional form controls use dark native browser controls. These remain illustrative browser comps rather than rendered SwiftUI.

The capture command renders both light and dark sets and runs the existing interaction checks in each, including the appearance toggle, minimum save, country defaults, pin voting and responsive layout. Both appearances passed on October 12, 2026, with zero external requests/page errors.

## User-confirmed behavior

- Only a valid map pin, nonblank access note and explicit Free/Paid are needed to save a new spot.
- Every Save creates a community submission. No sharing checkbox, private-save option or sharing-review gate.
- Name, venue category, paid conditions, voltage and plugs are optional behind **Add details (optional)**.
- The plug picker starts with the national outlet standard for the country the user is currently in. It remains editable. Country unknown/catalogue gaps stay Unknown; manual overrides survive country changes. EV connectors do not inherit domestic defaults.
- U.S. outlet voltage still suggests editable 120 V. Skipping optional power details does not submit defaults as confirmed observations.
- Tap an existing map pin for **✓ / ×** voting and counts directly on that pin. Details is a separate action. A repeated choice removes the vote; choosing the other switches it.

Community submission is the normal Save behavior; the form has no sharing notice. Queued submission and actual publication remain separate states. Existing legacy local records, ride history/device data and private bookmarks are not automatically uploaded.

## Screen assets

| Asset | Interaction |
| --- | --- |
| [browse.png](browse.png) | Labelled pins and anchored check/X voting on the selected pin |
| [coffee.png](coffee.png) | Optional venue/power information, access note, Directions and Bookmark |
| [list.png](list.png) | Equivalent accessible search/filter list |
| [place.png](place.png) | Position and confirm the pin |
| [edit.png](edit.png) | Minimum form: pin, Free/Paid and access note |
| [ev.png](ev.png) | EV connectors with unknown voltage |
| [pin-voting.png](pin-voting.png) | Selected map-pin vote state without opening details |
| [edit-free.png](edit-free.png) | Minimal free-spot form |
| [edit-uk.png](edit-uk.png) | Optional current-country plug default example |
| [edit-ev.png](edit-ev.png) | Optional EV form with Unknown power |
| [edit-more.png](edit-more.png) | Optional second power supply |
| [detail-larger-text.png](detail-larger-text.png) | Bounded scrollable detail sheet at larger browser text |

[Contact sheet](contact-sheet.png). The older sharing-review image has been removed from the current repository design set; its earlier Lific attachment is superseded.

## Visual and interaction direction

Propose **Live / History / Charging** within the existing Devices / Map / More shell. Third-mode navigation remains a draft. The light iOS system palette uses white, grouped gray, readable Map amber and yellow actions. Free/Paid text and glyphs carry meaning alongside color. Native form/sheet/button equivalents are expected in SwiftUI.

Map and list use the same bounded sample records. Pin-vote buttons have Vote up / Vote down accessible names, counts, selected state and 44-pixel targets. They do not imply electrical compatibility or automatically hide a disliked pin. Report a problem is separate.

Optional supply groups pair voltage with connectors. Country suggestions are draft values; the prototype submits them only after the user chooses **Include voltage and plugs**. Changing those values or replacing their defaults requires confirmation again. Missing optional name uses a neutral location label in production, not an invented venue. All newly saved three-field records are valid across domain/storage/FFI/API layers.

## Country catalogue reference

The mockup demonstrates US outlet labels, UK Type G, Germany Type F/C and Australia Type I. Country plug families were checked against the manufacturer's [Bel World Plug and Input Voltage Guide](https://www.belfuse.com/resource-library/tech-paper/world-plug-and-input-voltage-guide). This small demonstration table is not a complete production country catalogue. LIBCU-921 must finalize supported countries, multiple-standard defaults, national label mappings and maintenance from primary sources. The attempted IEC World Plugs page was inaccessible during this check.

Current physical-country context or user correction determines the initial suggestion; UI language/locale alone is insufficient. If the pin is known to be elsewhere, the user can correct its suggestion. Never replace a deliberate plug selection silently.

## UI copy and tracker posts

UI copy was reviewed with unslop on October 12, 2026. The required access-note helper no longer repeats its placeholder. Optional power controls say **Include voltage and plugs**. The save message says **Spot saved. Waiting to share.** The sharing notice was removed at the user’s request; community submission is assumed app behavior. Units, connector labels, required fields and the practical charger/adapter instruction are preserved.

The reviewed light/dark images are attached to [the epic](https://lific.mjc.lol/LIBCU/issues/LIBCU-409#comment-11978), [map discovery/details](https://lific.mjc.lol/LIBCU/issues/LIBCU-924#comment-11979), [placement/editor](https://lific.mjc.lol/LIBCU/issues/LIBCU-925#comment-11980) and [pin voting](https://lific.mjc.lol/LIBCU/issues/LIBCU-934#comment-11981). [DOC-27](https://lific.mjc.lol/LIBCU/pages/170) indexes these posts and current images. Native UI should use the same short, literal wording.

The preservation gate and banned-phrase check passed. Cadence/repetition warnings were reviewed in the context of separate UI strings. The capture harness passed both appearances after the copy changes.

## Remaining mockup work

Actual non-map address/coordinate editor, complete connector catalogue, optional price/hours fields, edit/delete/report dialogs, offline/publication/vote failures and native Dynamic Type/RTL/assistive technology need further design/acceptance. Illustrated destination actions are labelled as simulations. Current source grounding remains RideMapRouteView, RideMapLiveContentView, RideMapControlsView, PevScreenTheme and PevDashboardComponents; no native app change is included.

## Capture and check

Run from the repository root in the existing Devenv environment with Node, Playwright and Chrome:

```sh
node docs/mockups/charging-spots/check-interactions.cjs
node docs/mockups/charging-spots/capture.cjs
```

When using a desktop-bundled Playwright, set CUTOUT_MOCKUP_PLAYWRIGHT to its package directory. The interaction check covers filter reset, selected states, keyboard focus, live search, saving after typing an access note and explicit power confirmation, including changed defaults and accessible confirmation labels. The capture check renders the assets and exercises minimum community Save with optional fields omitted, blank-note/missing-cost/missing-pin rejection, country plug defaults/manual override, EV Unknown state, direct-pin vote set/switch/remove, filters/details, larger text and a 375-pixel layout. Zero external requests/page errors were observed. This is browser design evidence; native/backend/device acceptance remains open.
