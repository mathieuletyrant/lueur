# Changelog

## [0.8.0](https://github.com/mathieuletyrant/lueur/compare/v0.7.0...v0.8.0) (2026-11-02)


### ✨ Features

* **jellyfin:** follow playback through Jellyfin's WebSocket ([5f9b3d9](https://github.com/mathieuletyrant/lueur/commit/5f9b3d941eacca143ac4b1ff10dc792ab5ee0302))
* skip trailers, samples and extras with editable keywords ([c040a69](https://github.com/mathieuletyrant/lueur/commit/c040a69c5f2e40e2cb5a395f12e9d05c8a3585d5))

## [0.7.0](https://github.com/mathieuletyrant/lueur/compare/v0.6.0...v0.7.0) (2026-11-02)


### ✨ Features

* **web:** sync offset on the Now page, with ±0.1 s buttons, out of Look ([fa337ef](https://github.com/mathieuletyrant/lueur/commit/fa337ef9347a29d9f3946ab9388b9c15c367593b))


### 🐛 Fixes

* **jellyfin:** extrapolate the position from when the report changed ([88ed7c6](https://github.com/mathieuletyrant/lueur/commit/88ed7c69e8ac7171fe7c641338a86869ad87777a))
* **look:** sync offset up to ±10 s ([25351f2](https://github.com/mathieuletyrant/lueur/commit/25351f2b188c83aad396ca1ffeeefa0c2fe980da))
* **web:** draw lights where they read the screen, around the TV instead of over it ([f9c0320](https://github.com/mathieuletyrant/lueur/commit/f9c0320ffd05f07f03fb7f92cb70ddd4adc163e3))

## [0.6.0](https://github.com/mathieuletyrant/lueur/compare/v0.5.0...v0.6.0) (2026-11-02)


### ✨ Features

* **server:** extract a frame of a video and hold it on the lights ([658b1f4](https://github.com/mathieuletyrant/lueur/commit/658b1f4fa84f95dc5983b76966314c25775a0294))
* **web:** test the Look on a frame of a film, held on the lights ([1b04031](https://github.com/mathieuletyrant/lueur/commit/1b04031b16ed68149d487b186e10508bd1048e94))

## [0.5.0](https://github.com/mathieuletyrant/lueur/compare/v0.4.0...v0.5.0) (2026-11-02)


### ✨ Features

* **web:** GPU / CPU badge on running analyses ([dd3b1f2](https://github.com/mathieuletyrant/lueur/commit/dd3b1f2696354b86ba0561fb69b449ddac3ddf26))


### 🐛 Fixes

* **analyze:** decode AV1 on the GPU, run ffmpeg at the lowest priority ([e714ce3](https://github.com/mathieuletyrant/lueur/commit/e714ce32456c59f1699dd04f6cc34e627cef144f))

## [0.4.0](https://github.com/mathieuletyrant/lueur/compare/v0.3.0...v0.4.0) (2026-11-02)


### ✨ Features

* **look:** spread and fit to area settings, applied live ([075e863](https://github.com/mathieuletyrant/lueur/commit/075e86307ebb6571fd48a170f74cf5eb80e0cc8a))
* **server:** auto scan, orphan track cleanup, track picked up during playback ([eedf435](https://github.com/mathieuletyrant/lueur/commit/eedf435b20ce52358cbf3ffa7f536828fd0fa37d))
* **web:** Automation card in Library, Analyze missing, reordered page ([b19082e](https://github.com/mathieuletyrant/lueur/commit/b19082e22fb849f5dad79e6394af7995f4d5ccf9))


### 🐛 Fixes

* **player:** stretch channel positions to the area and sample near each edge ([388efed](https://github.com/mathieuletyrant/lueur/commit/388efeda178d61b66a1a02783eaa24ac328e74f6))

## [0.3.0](https://github.com/mathieuletyrant/lueur/compare/v0.2.0...v0.3.0) (2026-11-02)


### ✨ Features

* **server:** stop the scan, analyze one video next, blink one light ([6b2ab15](https://github.com/mathieuletyrant/lueur/commit/6b2ab15d01432b30d022d80184ec09eebcdc3937))
* **web:** stop button, searchable films and episodes list, clickable lights in the area view ([ae523d7](https://github.com/mathieuletyrant/lueur/commit/ae523d7c644eb8126fb8caca08d5f96d85bb0e3d))


### 🐛 Fixes

* **jellyfin:** list known devices from /Devices, /Sessions only has connected ones ([cf23705](https://github.com/mathieuletyrant/lueur/commit/cf237051608e88b213836ccfb26a2a920c54c600))

## [0.2.0](https://github.com/mathieuletyrant/lueur/compare/v0.1.0...v0.2.0) (2026-11-02)


### ✨ Features

* **server:** add `lueur serve` with an HTTP API, analysis queue and live status ([bb25d98](https://github.com/mathieuletyrant/lueur/commit/bb25d98af4b15603e8ca0255e1be353290a849e3))
* **web:** add the React UI (Now, Library, Look, Setup) ([a3ed6ad](https://github.com/mathieuletyrant/lueur/commit/a3ed6ad9febcf40d2371f35d8e0562a1a7770d1a))
