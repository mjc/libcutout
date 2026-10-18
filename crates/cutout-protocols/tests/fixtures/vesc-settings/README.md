# VESC 7.00 configuration fixtures

`vesc-7.00-motor-default.bin` and `vesc-7.00-app-default.bin` are complete serialized defaults generated from [`vedderb/bldc` revision `c835e9f10989f217269efb4ec943dfea7d280dfd`](https://github.com/vedderb/bldc/tree/c835e9f10989f217269efb4ec943dfea7d280dfd), using its `confgenerator_set_defaults_mcconf` / `confgenerator_set_defaults_appconf` and `confgenerator_serialize_mcconf` / `confgenerator_serialize_appconf` functions, linked with the same revision's `util/buffer.c`. Field metadata was checked against [`vedderb/vesc_tool` revision `58d9d26835784ad010e99dc8cf21cefb7bf05e71`](https://github.com/vedderb/vesc_tool/tree/58d9d26835784ad010e99dc8cf21cefb7bf05e71), under `res/config/7.00/parameters_{mcconf,appconf}.xml`. The harness and a minimal hardware include shim lived under `/tmp`; no source code was copied into this repository. `HW_DEFAULT_ID` was set to `0` because the defaults API depends on a hardware ID, so the APP fixture is a source-generated generic-ID baseline, not a claim about a particular controller.

The firmware serializer produced exactly 488 MC bytes and 284 APP bytes, including the four-byte signatures. The pinned source manifest reports no variable-length fields for either record. These fixture bytes independently exercise the codec against firmware serialization; they do not assert that any physical device has these defaults.

SHA-256:

- APP: `50456c926ccddf4b2ead000ed78fd696c8af7c3c27bd81472520af1a64fa592c`
- Motor: `2ac07675cb5a262ca72bbd0498cc1dbb1351028df341316309dc2f37a7b71203`
