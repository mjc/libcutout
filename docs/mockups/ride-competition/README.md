# Connected leaderboard mockups

Open [index.html](index.html) for the interactive prototype and light/dark screen set. Data and service responses are fictional and reset on reload.

The product plan, provider research, architecture, acceptance criteria and design decisions live in [Lific’s design index](https://lific.mjc.lol/LIBCU/pages/182). [DOC-34](https://lific.mjc.lol/LIBCU/pages/181) has the mockup specification and attached review images.

To regenerate the images and check the browser flows, run from the repository root in its Devenv environment with Node, Playwright and Chrome:

```sh
node docs/mockups/ride-competition/capture.cjs
```

Set `CUTOUT_MOCKUP_PLAYWRIGHT` to the Playwright package directory when the runtime supplies it separately. The script checks both appearances, provider/period switching, submission and recovery states, and narrow layout. These are browser prototype checks; provider and native app acceptance are tracked in Lific.
