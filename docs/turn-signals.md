# Automatic turn signals

The lean detector is a Rust-owned intent classifier for a future left/right
lighting mode. It consumes typed wheel speed and optional roll telemetry; it
does not choose an RGB-controller zone or emit a Bluetooth write. The MELK
controller's strip topology and left/right addressing still require a separate
physical capture.

The default detector uses a five-mph speed gate (`2,235 mm/s`), requires eight
degrees of roll for 250 ms before activation, and releases below four degrees.
The roll polarity is configurable because the current protocol evidence does
not establish which sign corresponds to the rider's left side. The defaults
are deliberately provisional and should be calibrated from a controlled ride.

Telemetry coverage is not uniform: the Refloat codec exposes `imu.roll`, while
the current Veteran telemetry decoder exposes pitch but no roll field. A
Veteran automatic turn-signal mode therefore remains unavailable until a
source-backed roll field is added and validated.

Required physical checks before enabling output include sustained left/right
leans above the speed gate, low-speed balance corrections, bump rejection,
polarity calibration, and independent confirmation that the controller's left
and right strips can be addressed separately.
