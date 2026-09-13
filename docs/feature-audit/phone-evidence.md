# Phone diagnostic evidence

Retrieved directly from the connected physical phone on 2026-09-13. Device: iPhone 15 Pro Max (iPhone16,2), iOS 27.0 beta, build 24A5430a. Installed app reports CutOut 1.0 (1). Source audited: `34b6309d7db2ffe2aadbce7bfca839a3e187d95b`. The installed app's exact Git revision is **not established** by its version number.

51 Cutout app/extension reports were copied read-only; two UI-test-runner reports were excluded. This comprises 27 termination reports (9 tab-color traps, 3 Spotify delegate traps, 11 watchdogs, 3 missing-framework launches, 1 CPU kill) and 24 nonfatal resource reports (21 extension CPU, 3 disk-write). These are report counts, not 51 distinct bugs. Older incidents remain open; no phone retest or crash fix was performed in this audit.

The latest small capture header identifies NOSFET Aero, with previously confirmed model ID 43. That is app-recorded identity evidence, not a new wheel interrogation. Saved Lighting preferences identify a MELK-OC21 accessory profile with confirmation unknown; they do not prove a physical command succeeded. The phone file listing reports `ride.sqlite` at 1,139,482,624 bytes (about 1.06 GiB). The database was not copied or modified.

Raw reports and two small captures are outside Git at `/tmp/cutout-feature-audit-phone/`. This folder is temporary. The portable inventory below retains original filenames, binary UUIDs and SHA-256 hashes for matching reports before the temporary copies expire. Device serials, account tokens, full preferences and raw capture payloads are intentionally absent from these documents.

Crash report stacks establish the observed failing execution path. They do not establish which tap preceded it, whether every historical build matches current source, or whether an OS contribution exists. A mitigation in current source is insufficient to close a phone failure.

## Report inventory

| Report | Issue family | Observed signature | Binary UUID | SHA-256 |
| --- | --- | --- | --- | --- |
| `CutoutApp-2026-09-06-154708.ips` | CRH-009 | Spotify framework missing at launch | `09c1e5bf-7eac-3685-a5ca-51a0ef5954df` | `cbeec5eb6b17225547d80d32c36c365166a842c814bd5b2a20e120b0a0e067eb` |
| `CutoutApp-2026-09-06-154709.ips` | CRH-009 | Spotify framework missing at launch | `09c1e5bf-7eac-3685-a5ca-51a0ef5954df` | `d2d3394d31ee53426fd868d2f91e29bbcc3aba648604311bf02cfe46758cf100` |
| `CutoutApp-2026-09-06-154953.ips` | CRH-009 | Spotify framework missing at launch | `09c1e5bf-7eac-3685-a5ca-51a0ef5954df` | `f45e6741b15891486534f45e25804df8d846a321f688f760becbc677fbde7ea7` |
| `CutoutApp-2026-09-06-200613.ips` | CRH-001 | Tab accent dynamic UIColor executor assertion | `3d4d2282-fe5f-35eb-9e31-e8e27e23f572` | `e38707fd884d09711e3f8cd95f495d9458a34642facdd8662935fb42b8d2dccd` |
| `CutoutApp-2026-09-06-200834.ips` | CRH-001 | Tab accent dynamic UIColor executor assertion | `3d4d2282-fe5f-35eb-9e31-e8e27e23f572` | `580741dce13ee00d6c411e1d0b02007acf75518b6f32c114743eed1b8eb9eace` |
| `CutoutApp-2026-09-06-200857.ips` | CRH-001 | Tab accent dynamic UIColor executor assertion | `3d4d2282-fe5f-35eb-9e31-e8e27e23f572` | `98e680a9137529c9e14f403405abc27bc786c1e6f688aec48f1fbd61e8f111bc` |
| `CutoutApp-2026-09-06-201005.ips` | CRH-001 | Tab accent dynamic UIColor executor assertion | `3d4d2282-fe5f-35eb-9e31-e8e27e23f572` | `c72ed6c7c3868f56e19bf2e83984b5d9574d5c9540206d7c7a359858aca68840` |
| `CutoutApp-2026-09-07-181245.ips` | CRH-005 | Other scene watchdog; causal path unresolved | `4b071fc4-e629-3056-9fa7-2b54bc3d4569` | `af20ef72ca50905d9ec58214c44cec6e0440b3e9a24d675e6c793f6a18eb25c4` |
| `CutoutApp-2026-09-07-182940.ips` | CRH-005 | Other scene watchdog; causal path unresolved | `4b071fc4-e629-3056-9fa7-2b54bc3d4569` | `eb500c6cb9fcebddc9ce2a612557cd86ac4be1af8fe74af92d7ee4af6d362c39` |
| `CutoutApp-2026-09-08-185548.ips` | CRH-005 | Other scene watchdog; causal path unresolved | `cb7d6b2c-6bba-3460-86a2-9f0b93721c4b` | `71e6939016e974afe7945c811c1ab45565bc35ab65072547fff2e93852d9ddfe` |
| `CutoutApp-2026-09-08-220851.ips` | CRH-004 | Watchdog during database bootstrap | `3e13de16-ebdf-3d5d-bc5d-d4505c4b467f` | `864709d0d0834dcd21acbee559dce38a0b8509456340a213004a62c6d6600165` |
| `CutoutApp-2026-09-08-224141.ips` | CRH-004 | Watchdog during database bootstrap | `ccd67160-48e9-3d25-9757-25d84b28e3a9` | `03ae4657c3b6aa4b98ba92485672f397a011468e5ed7ceb3cbf6bbe4566c8e83` |
| `CutoutApp-2026-09-09-153801.ips` | CRH-001 | Tab accent dynamic UIColor executor assertion | `6186234f-a173-3813-8b1c-6e5cfebc20d1` | `582cc989c437869bf451a2c1e84df67e88b5ed201232160e8100bfbf53f4396d` |
| `CutoutApp-2026-09-09-153812.ips` | CRH-004 | Watchdog during database bootstrap | `6186234f-a173-3813-8b1c-6e5cfebc20d1` | `f093a980cdc21558669b96a849797f6864d62f477e27837ae90f4f3dc98247d7` |
| `CutoutApp-2026-09-09-160400.ips` | CRH-002 | Spotify authorization delegate executor assertion | `4da67938-f2c1-3e8e-88a4-bfdaccf0f7ba` | `27e1a7ef03310598f04bfef0cd2dcd456afa2223b800fe86b33c959ca20cc5d2` |
| `CutoutApp-2026-09-09-160423.ips` | CRH-002 | Spotify authorization delegate executor assertion | `4da67938-f2c1-3e8e-88a4-bfdaccf0f7ba` | `3528d192b90525e317e9f5878984d6c95d705e4f7018ac5bf104bdc34ce5efc0` |
| `CutoutApp-2026-09-09-160440.ips` | CRH-002 | Spotify authorization delegate executor assertion | `4da67938-f2c1-3e8e-88a4-bfdaccf0f7ba` | `85fb3766fb9fb895c76b2c028518b5ae73375238c77ed3b5a699ec992936e57f` |
| `CutoutApp-2026-09-09-180242.ips` | CRH-004 | Watchdog during database bootstrap | `dac41fcb-80c5-3e2b-abcf-20c18950d537` | `13e5635a2e8bac1ab5d9d0ab471ea314d485891ed7da9ed2b2a9fb2969499185` |
| `CutoutApp-2026-09-12-112108.ips` | CRH-001 | Tab accent dynamic UIColor executor assertion | `5514ae89-eafa-3291-a77e-cb6142e3d82b` | `984c6e7210efd3ad08989358c2dda81ab1091de54df46aebceb64c4b8a6193b0` |
| `CutoutApp-2026-09-12-112754.ips` | CRH-001 | Tab accent dynamic UIColor executor assertion | `5514ae89-eafa-3291-a77e-cb6142e3d82b` | `d7701149b21138aa59b5aaf5644a0ff0b14dbd6e4296914dac60b14860684de7` |
| `CutoutApp-2026-09-12-143915.ips` | CRH-004 | Watchdog during database bootstrap | `5514ae89-eafa-3291-a77e-cb6142e3d82b` | `39618743f76d089d10c3abc9a16f01519c7122dc245e1be61b3ec767ef3d9c40` |
| `CutoutApp-2026-09-12-151519.ips` | CRH-010 | Watchdog during active-route launch projection | `5514ae89-eafa-3291-a77e-cb6142e3d82b` | `217584591d32fe3070d9568f053db763cb512fcfddb2d5930488b95eba8e9fbf` |
| `CutoutApp-2026-09-12-181951.ips` | CRH-005 | Other scene watchdog; causal path unresolved | `5514ae89-eafa-3291-a77e-cb6142e3d82b` | `7c878d3d95234be0375822fc5c5d7e30cce9e830037b8d73104401800e645f0b` |
| `CutoutApp-2026-09-12-221739.ips` | CRH-003 | Launch watchdog in Apple Music synchronous XPC | `e7716fdc-0d28-393b-a60c-5a8897dd6b3e` | `ea37905d31386f765f28829e1c8dde4ac0318cf9243ba6f42ac30850f98bda4c` |
| `CutoutApp-2026-09-13-110457.ips` | CRH-001 | Tab accent dynamic UIColor executor assertion | `0a80b415-90ec-3df2-9daa-f766e13e5571` | `3b1c41576401dfd192d72d30075151bbcbc7add6d0c9afc413e779f1b63ccd79` |
| `CutoutApp-2026-09-13-110509.ips` | CRH-001 | Tab accent dynamic UIColor executor assertion | `0a80b415-90ec-3df2-9daa-f766e13e5571` | `94136ac77feda7b0aed6649e2199c81ec9228207b1bcb95ef4843d9f5f48d861` |
| `CutoutApp.cpu_resource_fatal-2026-09-12-143906.ips` | CRH-006 | CPU resource limit; process killed | `5514AE89-EAFA-3291-A77E-CB6142E3D82B` | `51460ba67eb2f9fffd18a7280e8e4b7fe36dcad8c7e11436f4f43f4a06a49639` |
| `CutoutApp.diskwrites_resource-2026-09-12-124347.ips` | CRH-008 | Disk-write resource report; no termination recorded | `5514AE89-EAFA-3291-A77E-CB6142E3D82B` | `762fbf28def47bb6e0bc75d053794c5ab5de8d7a26e0134ea1ff3eea0dde60d8` |
| `CutoutApp.diskwrites_resource-2026-09-12-173825.ips` | CRH-008 | Disk-write resource report; no termination recorded | `5514AE89-EAFA-3291-A77E-CB6142E3D82B` | `3951b27aa25b50900fd7768d6b4879ad11d1c4c0bb8774e1cc6e6fbcb9977985` |
| `CutoutApp.diskwrites_resource-2026-09-12-211748.ips` | CRH-008 | Disk-write resource report; no termination recorded | `5514AE89-EAFA-3291-A77E-CB6142E3D82B` | `f78d4e37d71a244237e63bcaf6cd8b3c1d6410605a92f62161e6fe5943ecc9ed` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-06-160716.ips` | CRH-007 | Extension CPU report; no termination recorded | `342BF126-D718-3175-8272-9C01ED01B01A` | `5faaa9f36396277df72055bb2f78a9943c5514a4c9be74618720fb109c6ef227` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-06-161450.ips` | CRH-007 | Extension CPU report; no termination recorded | `FEF8DC41-0F74-33EE-A568-465375B032D2` | `29bd76079a81c4c1d6abf957258bff299ee6b32c3697e2685eb5c86e64944d1f` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-06-162112.ips` | CRH-007 | Extension CPU report; no termination recorded | `2CECE972-F2A8-3509-A8F4-DFE885E7B0EF` | `6c80066ff25db11453a5ca1da054a37a3c676ff1348def263fcfc26760f92a1b` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-06-162733.ips` | CRH-007 | Extension CPU report; no termination recorded | `D6497E4A-6CE4-36B1-83FA-5FA3970D40ED` | `59db1fc6d0598ebb0ecd1c5142769f75fe9a983e14179612889a70cf7b9770b2` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-06-163656.ips` | CRH-007 | Extension CPU report; no termination recorded | `5ECEF24A-FFAC-3464-9C15-5344B479FCB0` | `e49613f2996082c98279f1382275d2328957603af4038561ab2bfe96f5e236b0` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-06-183141.ips` | CRH-007 | Extension CPU report; no termination recorded | `CF64891F-9F4D-308E-8558-FBC0A828BBA3` | `78b931527d2dd69cb27eb847c0a09ae9597200b065cf8d96e80cf91a77d5acbf` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-06-185746.ips` | CRH-007 | Extension CPU report; no termination recorded | `CF64891F-9F4D-308E-8558-FBC0A828BBA3` | `fe2a392ab35ee9ac7047a1341569a55f6f1c146e01cd2b453086630b10bef689` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-06-190313.ips` | CRH-007 | Extension CPU report; no termination recorded | `CF64891F-9F4D-308E-8558-FBC0A828BBA3` | `81d0b97ba8055a662f309a3fc547b5d82e23226781c2c676e3aad81adc5bbcf3` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-06-202143.ips` | CRH-007 | Extension CPU report; no termination recorded | `0ED82D74-8CE9-33B8-BA0A-737A8A03A091` | `9a14bb64280c2e33000a339bde8090423e25eed32056463f4f8bb6736541ff83` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-08-190216.ips` | CRH-007 | Extension CPU report; no termination recorded | `1B568157-2620-32FE-9AA1-37A6194E544D` | `41c5362dcf6a285bd99bf535f2ea2d5ecde5133878dc973e29a7b0b1fdac4ecc` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-08-190722.ips` | CRH-007 | Extension CPU report; no termination recorded | `1B568157-2620-32FE-9AA1-37A6194E544D` | `45318444f58ee264d94d08e3713606da4289a42eee23a46f889fdf8508a0188a` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-08-194428.ips` | CRH-007 | Extension CPU report; no termination recorded | `E86F3594-56D3-329E-9BBF-9B517D9D511C` | `3fa6ff19c7eb03fffb6482e5d628897ab1402345345ff65b5b0d59ef6126ddb7` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-08-195248.ips` | CRH-007 | Extension CPU report; no termination recorded | `E86F3594-56D3-329E-9BBF-9B517D9D511C` | `7242059f6d5df4e3315a2da25fbabd67494accc7e967c2ad5ae0f6aedbf22723` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-12-120036.ips` | CRH-007 | Extension CPU report; no termination recorded | `0EC5DCDF-F4B2-30E4-8B6F-A3A08ABA18D5` | `46ee05b74dd98934efd630401ea62401059831f09eddbd9ffe0369bb3fe3d390` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-12-122526.ips` | CRH-007 | Extension CPU report; no termination recorded | `0EC5DCDF-F4B2-30E4-8B6F-A3A08ABA18D5` | `34164345f212653fa7da7b58cd61597cc13ec33e3b3d16d77033faf6ea9797ee` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-12-123121.ips` | CRH-007 | Extension CPU report; no termination recorded | `0EC5DCDF-F4B2-30E4-8B6F-A3A08ABA18D5` | `2b4bbf18c878560a71abf3996cff0ad677ba4ef81abebabca0cf06dafd8c2734` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-12-124006.ips` | CRH-007 | Extension CPU report; no termination recorded | `0EC5DCDF-F4B2-30E4-8B6F-A3A08ABA18D5` | `d8cbea168d112e0f4b611451ea522492e3a19b48004a43257a23a05eaa7d542a` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-12-124803.ips` | CRH-007 | Extension CPU report; no termination recorded | `0EC5DCDF-F4B2-30E4-8B6F-A3A08ABA18D5` | `c1741006f683a45b181636a4022de84617d527014ab6193119f57311182c1659` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-12-191819.ips` | CRH-007 | Extension CPU report; no termination recorded | `0EC5DCDF-F4B2-30E4-8B6F-A3A08ABA18D5` | `30c66c5eb1420f39616e9cfc3dd920ec0b6a9d1d31b63d1245d8e328a22ef309` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-12-193844.ips` | CRH-007 | Extension CPU report; no termination recorded | `0EC5DCDF-F4B2-30E4-8B6F-A3A08ABA18D5` | `754dfb59f73605fae8cfd02ac45b2dd73a2d3eaaa92d5ff58940839f488441da` |
| `CutoutLiveActivityExtension.cpu_resource-2026-09-13-110618.ips` | CRH-007 | Extension CPU report; no termination recorded | `8641A77B-BC11-3BAB-898E-41BE59EE4E61` | `62ba5619c39ce975cd77afc1fb6a4141879e13bf09cc63b8b3c836cac8e93e8c` |
