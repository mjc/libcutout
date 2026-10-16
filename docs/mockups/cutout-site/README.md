# CutOut website mockup

Run from the repository root:

```sh
devenv shell -- python3 -m http.server 4186 --bind 127.0.0.1 --directory docs/mockups/cutout-site
```

Open <http://127.0.0.1:4186/?page=home&lang=en>. Routes are `home`, `guide`, `support`, and `about`; languages are `en` and `es`.

Edit copy in `locales/en.json` and `locales/es.json`, layout in `site.js`, and styling in `site.css`. Keep locale keys and array shapes aligned. The preview uses local assets and no package installation. The icon comes from the checked-in iOS app asset catalog.

This interactive site is served by cutout-server, with the accepted mockup design. Screenshot areas are labeled placeholders, the download control explains current alpha availability, and external support/privacy links retain their existing destinations. English and Spanish copy use the same structure. Screenshot capture and the installation destination remain separate work. When deploying changes, update the CSS and module version in index.html; locale requests inherit the module version so cached assets stay aligned.

Content direction: [LIBCU-DOC-36](https://lific.mjc.lol/LIBCU/pages/188). Scope and design decisions: [LIBCU-975](https://lific.mjc.lol/LIBCU/issues/LIBCU-975). PEV coverage and protocol policy: [LIBCU-DOC-30](https://lific.mjc.lol/LIBCU/pages/177#project-wide-pev-coverage-and-protocol-policy).

## Alpha signup mockups

Open `testflight.html?view=form&lang=en` on the local preview server. Other states: `received`, `retry`, and `lific`; both `en` and `es` are available. Submit reserved example.com addresses to exercise the form. This prototype sends no submissions, stores no browser data and sends no email. Its files are not routed by cutout-server.

Feature contract and current decisions: [LIBCU-979](https://lific.mjc.lol/LIBCU/issues/LIBCU-979), [LIBCU-DOC-37](https://lific.mjc.lol/LIBCU/pages/189).
