# CutOut website mockup

Run from the repository root:

```sh
devenv shell -- python3 -m http.server 4186 --bind 127.0.0.1 --directory docs/mockups/cutout-site
```

Open <http://127.0.0.1:4186/?page=home&lang=en>. Routes are `home`, `guide`, `support`, `about`, `signup`, `privacy`, and `remove-request`; languages are `en` and `es`.

Edit copy in `locales/en.json` and `locales/es.json`, layout in `site.js`, and styling in `site.css`. Keep locale keys and array shapes aligned. The preview uses local assets and no package installation. The icon comes from the checked-in iOS app asset catalog.

This interactive site is served by cutout-server, with the accepted mockup design. Screenshot areas are labeled placeholders, and English and Spanish copy use the same structure. Screenshot capture and the installation destination remain separate work. When deploying changes, update the CSS and module version in index.html; locale requests inherit the module version so cached assets stay aligned.

The production signup page is available at `/?page=signup&lang=en` (or `lang=es`) and is linked from the homepage alpha calls to action. `?page=privacy` shows the signup privacy notice and links to a generic, operator-reviewed removal request at `?page=remove-request`. The signup form calls `/v1/alpha-signup/requests`; the removal form calls `/v1/alpha-signup-removals`. Both are same-origin endpoints. The public page contains no Lific destination or credential.

For local browser review without sending a real challenge or signup, run the isolated fixture:

```sh
devenv shell -- python3 docs/mockups/cutout-site/alpha-signup-preview.py
```

Open <http://127.0.0.1:4187/?page=signup&lang=en>. It injects a clearly labeled local Turnstile mock using Cloudflare’s published test sitekey and returns a mocked API response for signup and removal requests. The “Local response” selector at the bottom-right switches among `202`, `422`, `429`, and `503` without restarting the server; the environment variable `ALPHA_SIGNUP_PREVIEW_STATUS` still sets its initial value. This script is not routed by cutout-server, does not call Cloudflare Siteverify, and neither saves nor logs submitted fields.

Content direction: [LIBCU-DOC-36](https://lific.mjc.lol/LIBCU/pages/188). Scope and design decisions: [LIBCU-975](https://lific.mjc.lol/LIBCU/issues/LIBCU-975). PEV coverage and protocol policy: [LIBCU-DOC-30](https://lific.mjc.lol/LIBCU/pages/177#project-wide-pev-coverage-and-protocol-policy).

## Alpha signup mockups

Open `testflight.html?view=form&lang=en` on the local preview server. Other states: `received`, `retry`, and `lific`; both `en` and `es` are available. Submit reserved example.com addresses to exercise the form. This prototype sends no submissions, stores no browser data and sends no email. Its files are not routed by cutout-server.

Feature contract and current decisions: [LIBCU-979](https://lific.mjc.lol/LIBCU/issues/LIBCU-979), [LIBCU-DOC-37](https://lific.mjc.lol/LIBCU/pages/189).
