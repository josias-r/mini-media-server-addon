# Changelog

## [0.4.1](https://github.com/josias-r/mini-media-server-addon/compare/v0.4.0...v0.4.1) (2026-11-24)


### Bug Fixes

* **placeholder:** resume polling when session restarts within idle grace ([9ff267d](https://github.com/josias-r/mini-media-server-addon/commit/9ff267d08020b00219164a4ee9c1aac8a615301f))
* **trigger:** route cached trigger clicks directly to stream or live state ([56ac11f](https://github.com/josias-r/mini-media-server-addon/commit/56ac11fea0aeb852217342139b6c0f5c4429dc63))

## [0.4.0](https://github.com/josias-r/mini-media-server-addon/compare/v0.3.0...v0.4.0) (2026-11-23)


### Features

* **streams:** filter out sample and extra video files from available streams ([4fb6c99](https://github.com/josias-r/mini-media-server-addon/commit/4fb6c9996963ee7bd9397307509b408a2bafb377))


### Bug Fixes

* **play:** prefetching next episode should not be dependent on placeholder state ([c217c58](https://github.com/josias-r/mini-media-server-addon/commit/c217c582dfc7bffdc47e937174ddb5d75da15190))
* **qbit:** invalidate cached session cookie and retry on 403 Forbidden ([0aae638](https://github.com/josias-r/mini-media-server-addon/commit/0aae638ef5b7562ecfe04be3a7ed2d25d2f3ca82))
* queue next episode on watch in full season torrent ([887178f](https://github.com/josias-r/mini-media-server-addon/commit/887178f341b0d60b3a87558bafc16a7d0fc60baf))
* **state:** evict triggered torrent and file keys when torrents are deleted ([058d2fd](https://github.com/josias-r/mini-media-server-addon/commit/058d2fd23c1cbf22f14d4c03037b5aa0712bf6ff))
* **trigger:** preserve in-progress and existing files when setting season pack priorities ([a23c984](https://github.com/josias-r/mini-media-server-addon/commit/a23c9842d6efc1d6398bc1e7a857942d5d70674e))
* update placeholder messages for download and episode requests ([1e96fd5](https://github.com/josias-r/mini-media-server-addon/commit/1e96fd58c4442b1035532c0f68a2bca77964a220))


### Performance Improvements

* **streams:** parallelize Jackett and qBittorrent queries and add Jackett timeout ([9119a19](https://github.com/josias-r/mini-media-server-addon/commit/9119a194ed362ebeda719ea92820bd3fe0cf045a))


### Code Refactoring

* **clients:** deduplicate qBittorrent auth requests, SQLite queries, and Jackett/Cinemeta helpers ([8e97f3e](https://github.com/josias-r/mini-media-server-addon/commit/8e97f3ed63e279f235106ad6e9f49e14cfce6c19))
* **domain:** extract pure domain layer and simplify parser and matcher ([41b99a6](https://github.com/josias-r/mini-media-server-addon/commit/41b99a65a941b802b8220ebb786a5bde01a54afe))
* **placeholder:** split fMP4 box builder from live session and range streaming ([580e212](https://github.com/josias-r/mini-media-server-addon/commit/580e2120d5c4afc56f87c5395cffaf2b62f70ac1))
* **routes:** consolidate route handlers and compact integration test fixtures ([5656a2c](https://github.com/josias-r/mini-media-server-addon/commit/5656a2cffd162f09a3b73f10578c3e6b8bf6e509))
* **state:** move AppState initialization into config and unify stream builder ([95f0dc0](https://github.com/josias-r/mini-media-server-addon/commit/95f0dc05567ae2b8adf3260eebb769b1412258bc))

## [0.3.0](https://github.com/josias-r/mini-media-server-addon/compare/v0.2.1...v0.3.0) (2026-11-20)


### Features

* add 'files missing' placeholder state and related functionality ([0703b93](https://github.com/josias-r/mini-media-server-addon/commit/0703b9367564b2f4ef1904bc3679f51fe76a04c7))
* stream download state in live chunks ([bacf2e7](https://github.com/josias-r/mini-media-server-addon/commit/bacf2e78058337152de67af029b042f582a99edf))


### Bug Fixes

* fix admin overflow styling issue on desktop ([3af5649](https://github.com/josias-r/mini-media-server-addon/commit/3af56494863d93104c1d843a4800bcd8453ac3d8))
* improve state messages ([48a162d](https://github.com/josias-r/mini-media-server-addon/commit/48a162d715a8005ab1afd87caa0b2ad88fd76c12))

## [0.2.1](https://github.com/josias-r/mini-media-server-addon/compare/v0.2.0...v0.2.1) (2026-11-19)


### Bug Fixes

* **compatible-clients:** update platform listings and update harbor link ([ba318b4](https://github.com/josias-r/mini-media-server-addon/commit/ba318b4ba4a68b608ad5ae17791eefe02555cee0))
* **onboarding:** update onboarding steps to include client installation as a seperate step ([846adb0](https://github.com/josias-r/mini-media-server-addon/commit/846adb0213652d72fba322d3a4ec2c33981d839a))

## [0.2.0](https://github.com/josias-r/mini-media-server-addon/compare/v0.1.4...v0.2.0) (2026-11-18)


### Features

* **catalog:** add user and community stremio catalog endpoints ([92636e3](https://github.com/josias-r/mini-media-server-addon/commit/92636e32cb9808eece561cd0f471e2518ab4c9c5))
* **db:** track media watch history and cache cinemeta metadata ([bf5ae8a](https://github.com/josias-r/mini-media-server-addon/commit/bf5ae8a7c2c76d197423c23da221283d9a1f6679))
* **placeholders:** add state-specific placeholder mp4 videos ([e847093](https://github.com/josias-r/mini-media-server-addon/commit/e847093f53f90390a4b83e68bc4b720e9a101091))
* **qbit:** tag torrents with media type and requested episode ids ([d5ad00d](https://github.com/josias-r/mini-media-server-addon/commit/d5ad00d0684047ed330b1a1306ddf96ecd1b086f))
* **stream:** serve immediate state-specific placeholders with stream readiness threshold ([90bb892](https://github.com/josias-r/mini-media-server-addon/commit/90bb8927895ca61565a208f7fafeb13c760ce9c6))

## [0.1.4](https://github.com/josias-r/mini-media-server-addon/compare/v0.1.3...v0.1.4) (2026-11-18)


### Bug Fixes

* add iOS App Store warning and web app guidance to install steps ([c6c9e0a](https://github.com/josias-r/mini-media-server-addon/commit/c6c9e0aef4cf3902fd6fdc8dd01cc400554fcbd7))


### Code Refactoring

* extract reusable UI components and modularize app pages ([34dca60](https://github.com/josias-r/mini-media-server-addon/commit/34dca60dcca6fdbe3e2b67a7439d35669d8384bc))

## [0.1.3](https://github.com/josias-r/mini-media-server-addon/compare/v0.1.2...v0.1.3) (2026-11-18)


### Bug Fixes

* enhance onboarding UX flow ([c967713](https://github.com/josias-r/mini-media-server-addon/commit/c9677133c7dba9c896fc4d411e3d731cb9e7e9e8))
* step-by-step user instructions ([c8a8302](https://github.com/josias-r/mini-media-server-addon/commit/c8a830219823edabcae4c370935f7c4f32dd3108))

## [0.1.2](https://github.com/josias-r/mini-media-server-addon/compare/v0.1.1...v0.1.2) (2026-11-18)


### Bug Fixes

* improve UI on mobile ([bde6105](https://github.com/josias-r/mini-media-server-addon/commit/bde6105cfee26715ea45aae391d12b4bb7932541))
* move user admin section to its own page ([d574e61](https://github.com/josias-r/mini-media-server-addon/commit/d574e6151c37d0dfb7e9bbae77c74b4341a6d155))
* update client compatibility details ([29a7369](https://github.com/josias-r/mini-media-server-addon/commit/29a7369619256c52aa68ff3411dafadfc04de757))

## [0.1.1](https://github.com/josias-r/mini-media-server-addon/compare/v0.1.0...v0.1.1) (2026-11-13)


### Bug Fixes

* handle qBittorrent startup readiness in retention worker ([8b70016](https://github.com/josias-r/mini-media-server-addon/commit/8b700160e499824eb5a3a840669f0d9bc674f7b4))
* harden runtime RETENTION_DAYS env parsing ([06ad479](https://github.com/josias-r/mini-media-server-addon/commit/06ad47960389502fa04a899ba54b0f0d603b04e5))
* normalize info_hash case in watch history and qBittorrent requests ([9787be6](https://github.com/josias-r/mini-media-server-addon/commit/9787be62804c8942fa9f0109442ea278f49ffb38))
* resolve torrent info_hash and prioritize matched episode file ([f7a5cbc](https://github.com/josias-r/mini-media-server-addon/commit/f7a5cbc2d065df4df40433e267a48938307f694f))
* use slice::as_chunks in sha1_hex for Rust 1.100 clippy ([590b70f](https://github.com/josias-r/mini-media-server-addon/commit/590b70fbaf0d80565cc1dbe10e49c158793ccebb))

## [0.1.0](https://github.com/josias-r/mini-media-server-addon/compare/v0.0.20...v0.1.0) (2026-11-13)


### Features

* add environment configuration and local development instructions ([00e3560](https://github.com/josias-r/mini-media-server-addon/commit/00e3560847b530c98949b764eb8f4c84a89e3b18))
* implement release automation with GitHub Actions and update ver… ([0fd836f](https://github.com/josias-r/mini-media-server-addon/commit/0fd836f0df7b69d768f5e5b42afd6ff36481dbba))
* implement release automation with GitHub Actions and update versioning ([c69a274](https://github.com/josias-r/mini-media-server-addon/commit/c69a2741d932f0c20b05ad5c9e61dff846102dfa))
* init new web app ([92f8f56](https://github.com/josias-r/mini-media-server-addon/commit/92f8f569141767493f5370fe2a492fcbbe89283b))
