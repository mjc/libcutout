# CutOut website mockup

Run from the repository root:

```sh
devenv shell -- python3 -m http.server 4186 --bind 127.0.0.1 --directory docs/mockups/cutout-site
```

Open <http://127.0.0.1:4186/?page=home&lang=en>. Routes are `home`, `guide`, and `support`; languages are `en` and `es`.

Edit copy in `locales/en.json` and `locales/es.json`, layout in `site.js`, and styling in `site.css`. Keep locale keys and array shapes aligned. The preview uses local assets and no package installation. The icon comes from the checked-in iOS app asset catalog.

This is an interactive design artifact. Screenshot areas are labeled placeholders, the download control opens a preview explanation, and external support/privacy links retain their existing destinations. Spanish copy demonstrates the localization structure and still needs editorial review before publication. Production rendering, server integration, and screenshot capture are separate work.

Scope and design decisions: [LIBCU-975](https://lific.mjc.lol/LIBCU/issues/LIBCU-975). PEV coverage and protocol policy: [LIBCU-DOC-30](https://lific.mjc.lol/LIBCU/pages/177#project-wide-pev-coverage-and-protocol-policy).
