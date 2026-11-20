# Changelog

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
