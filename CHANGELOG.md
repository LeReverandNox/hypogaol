# Changelog

## 0.1.0 (2026-07-28)


### Features

* **1.1:** project scaffolding & Nix devShell ([#2](https://github.com/LeReverandNox/tomb-fido2/issues/2)) ([d7db5f8](https://github.com/LeReverandNox/tomb-fido2/commit/d7db5f8144a2af0851fa942a8417029c3a58d17d))
* **1.2:** CI runs the mocked unit test suite ([#4](https://github.com/LeReverandNox/tomb-fido2/issues/4)) ([50e604a](https://github.com/LeReverandNox/tomb-fido2/commit/50e604a5ce4a7289513942cd639012579a43abfe))
* **1.3:** release automation (release-please + cargo-dist) ([#6](https://github.com/LeReverandNox/tomb-fido2/issues/6)) ([852bd91](https://github.com/LeReverandNox/tomb-fido2/commit/852bd91c4d9c6c293c12c106b58dec77846f2c96))
* **1.4:** dependency preflight check ([#8](https://github.com/LeReverandNox/tomb-fido2/issues/8)) ([7ce3dc1](https://github.com/LeReverandNox/tomb-fido2/commit/7ce3dc1436964f7155a21acc2c6659190bd55295))
* **1.5:** create a file-backed tomb ([#11](https://github.com/LeReverandNox/tomb-fido2/issues/11)) ([4d7c0c2](https://github.com/LeReverandNox/tomb-fido2/commit/4d7c0c245cd23d971656261826655a4b24f30a49))
* **1.6:** create a device-backed tomb ([#14](https://github.com/LeReverandNox/tomb-fido2/issues/14)) ([9395b10](https://github.com/LeReverandNox/tomb-fido2/commit/9395b1096890585237a4af449d561e85a165235a))
* **1.7:** unlock and mount a tomb ([#16](https://github.com/LeReverandNox/tomb-fido2/issues/16)) ([e797d3c](https://github.com/LeReverandNox/tomb-fido2/commit/e797d3c3e0ea76956fb580d1d66c5cae761b25e9))
* **1.8:** unified CLI dispatch & plain-language errors ([#18](https://github.com/LeReverandNox/tomb-fido2/issues/18)) ([09b9d1d](https://github.com/LeReverandNox/tomb-fido2/commit/09b9d1db220fcd6130bcd4c2306158a991e4bfa7))
* **1.9:** mount UX & ownership hardening ([#20](https://github.com/LeReverandNox/tomb-fido2/issues/20)) ([9065e2c](https://github.com/LeReverandNox/tomb-fido2/commit/9065e2c542e74c76478174762f3d47fb14398f93))
* **2.1:** enroll an additional FIDO2 key ([#23](https://github.com/LeReverandNox/tomb-fido2/issues/23)) ([20cd6f5](https://github.com/LeReverandNox/tomb-fido2/commit/20cd6f5ade674998f264eaa62079a703af78381e))
* **2.2:** revoke a FIDO2 key, guarded against last-keyslot lockout ([#25](https://github.com/LeReverandNox/tomb-fido2/issues/25)) ([88fbc0c](https://github.com/LeReverandNox/tomb-fido2/commit/88fbc0ce9211e7a11ece3e19fae18cb353262db1))
* **3.1:** close an unlocked tomb ([#27](https://github.com/LeReverandNox/tomb-fido2/issues/27)) ([fd6b5cf](https://github.com/LeReverandNox/tomb-fido2/commit/fd6b5cf9b843e06dd133b90dab29b52f85c4476b))
* **3.2:** grow an existing tomb's capacity ([#29](https://github.com/LeReverandNox/tomb-fido2/issues/29)) ([11723e9](https://github.com/LeReverandNox/tomb-fido2/commit/11723e9307775ffba214e822fc60b6318afa85bf))
* **3.3:** unlock a tomb read-only ([#31](https://github.com/LeReverandNox/tomb-fido2/issues/31)) ([cb845a9](https://github.com/LeReverandNox/tomb-fido2/commit/cb845a9facd9219c469f773780925d5fc2e1ce26))
* **4.1:** view a tomb's enrolled keys (info) ([#33](https://github.com/LeReverandNox/tomb-fido2/issues/33)) ([5afa585](https://github.com/LeReverandNox/tomb-fido2/commit/5afa585bfac5a55a72a2981ab8e3d31c675e67cd))
* **4.2:** real progress reporting for create & resize ([#35](https://github.com/LeReverandNox/tomb-fido2/issues/35)) ([c8bd06d](https://github.com/LeReverandNox/tomb-fido2/commit/c8bd06df37c8885c29238ef1274a3d98f43b10c8))
* **4.3:** enroll a FIDO2 key with user-verification ([#38](https://github.com/LeReverandNox/tomb-fido2/issues/38)) ([b774927](https://github.com/LeReverandNox/tomb-fido2/commit/b774927d46df5c45328d198ef82fc4ff10399fb5))
* **4.4:** per-tomb bind-hooks & exec-hooks automation ([#40](https://github.com/LeReverandNox/tomb-fido2/issues/40)) ([6d482dd](https://github.com/LeReverandNox/tomb-fido2/commit/6d482dd8b6c458898456240df8ea66b9cb58fc13))
* **4.5:** close every open tomb (close-all) ([#42](https://github.com/LeReverandNox/tomb-fido2/issues/42)) ([ef7cadf](https://github.com/LeReverandNox/tomb-fido2/commit/ef7cadfcaf54ebdfef874f810da59826afbe8d85))
* **4.6:** emergency slam command ([#44](https://github.com/LeReverandNox/tomb-fido2/issues/44)) ([0a7826f](https://github.com/LeReverandNox/tomb-fido2/commit/0a7826faf3672193fe600680aaf8f33a802380fc))


### Bug Fixes

* **1.5:** apply code review findings from PR [#11](https://github.com/LeReverandNox/tomb-fido2/issues/11) ([#12](https://github.com/LeReverandNox/tomb-fido2/issues/12)) ([f985288](https://github.com/LeReverandNox/tomb-fido2/commit/f985288937103ddfbea7a096bc955dd3c18074c9))
* **1.9:** apply code review findings from PR [#20](https://github.com/LeReverandNox/tomb-fido2/issues/20) ([#21](https://github.com/LeReverandNox/tomb-fido2/issues/21)) ([c005771](https://github.com/LeReverandNox/tomb-fido2/commit/c005771f67b3436dc31480a168ccc959501ee05a))
* **4.2:** apply code review findings from PR [#35](https://github.com/LeReverandNox/tomb-fido2/issues/35) ([#36](https://github.com/LeReverandNox/tomb-fido2/issues/36)) ([7488f53](https://github.com/LeReverandNox/tomb-fido2/commit/7488f539147e1139e21cc1bcb7804eeb25053908))
* reject undersized tombs and fix root-owned lost+found ([626c074](https://github.com/LeReverandNox/tomb-fido2/commit/626c0746e6db9726c10b531a621e2c7d3252e1cf))
* surface failed rollback close instead of silently swallowing it ([38ac76a](https://github.com/LeReverandNox/tomb-fido2/commit/38ac76ae8d31a5bdd379e6a18c432a960b278a3c))
