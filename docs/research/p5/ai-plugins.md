I now have complete coverage of both subsystems. Here is the exhaustive map.

<analysis>
**Literal Request**: file:line map of AI services and the JS plugin runtime in `D:\Dev\open-canvas\web` (+ `plugins/canvas/*`), covering request shapes, config resolution, local models, plugin contract/loading, LOC triage, and Rust-port feasibility.
**Actual Need**: Decide, per subsystem, (a) what is a plain HTTP+JSON port, (b) what needs a native inference stack, (c) whether to embed a JS engine or define a native plugin ABI or drop plugins.
**Success Looks Like**: A concrete inventory with endpoint/body/response/auth/poll/retry facts, a model-resolution algorithm, weight-fetch/cache mechanics, the plugin eval+contract surface, and an explicit feasibility verdict.
</analysis>

# AI Services + Plugin Runtime — Port Map

All paths absolute. Line numbers from current files. No tests referenced.

---

## 1. AI Services

### 1.1 Shared transport layer (CORE)
`D:\Dev\open-canvas\web\src\services\api\api-error.ts` (71 LOC)
- `apiText()` L5 — i18n message lookup (`apiErrors.*`).
- `readApiErrorMessage()` L13 — unwraps string / JSON-string / HTML / `{msg|message|error|detail}`.
- `statusMessage()` L45 — 401/403→auth, 429→rate, 404, 502, 503 mapping.
- `readAxiosError()` L55 — prefers response body, then status, then axios message; handles cancel/abort.

Auth model is uniform: `Authorization: Bearer <apiKey>` (`image.ts:314`, `video.ts:86`, `model-plugin.ts:56`, `openrouter-account.ts:35`). URL builder `buildApiUrl` normalizes `baseUrl` to end in `/v1` (`use-config-store.ts:406-411`). No global axios timeout is set anywhere → effectively infinite; only `AbortSignal` cancels.

### 1.2 Image + Vision/OCR — `image.ts` (610 LOC, CORE)

| Function | Line | Transport | Endpoint | Request body | Response |
|---|---|---|---|---|---|
| `requestGeneration` | 465 | axios POST | `{baseUrl}/v1/images` | `{model: IMAGE_MODEL, prompt, n, quality?, size?\|aspect_ratio?, background?, output_format:"png"}` | `{data\|images\|results: [{b64_json\|url}]}` |
| `requestEdit` | 516 | axios POST | `{baseUrl}/v1/images` | `{model: IMAGE_MODEL, prompt, n:1, ..., input_references:[{type:"image_url",image_url:{url}}], output_format:"png"}` | same |
| `requestImageQuestion` | 571 | fetch SSE | `{baseUrl}/v1/responses` | `{model, input:[...], stream:true, reasoning:{effort}?}` | SSE `response.output_text.delta/done`, `response.completed` |
| `requestStreamingResponse` (helper) | 431 | fetch+ReadableStream | `/responses` | adds `stream:true`, `Accept:text/event-stream` | parsed via `consumeResponseStreamBlock` L389 |
| `fetchGenerationCost` | 277 | axios GET | `/v1/generation?id=<id>` | — | `{data|total_cost|usage}` |
| `readGenerationId` | 271 | — | reads `x-generation-id` response header | | |

Key details:
- Image ops **force the model** to the constant `IMAGE_MODEL = "openai/gpt-image-2.5-sunburst"` (`image.ts:466,517`; constant `use-config-store.ts:72`), regardless of selected channel model — the channel only supplies baseUrl/apiKey.
- Text/vision resolves `config.model || config.textModel` (L572).
- `n` clamped 1–10 (L467,518).
- Size/quality mapping: `normalizeQuality` L138, `resolveSize` L150 (pixels from `imageSizePresets`), `resolveImageRequestOptions` L220, validation `validateImageSize` L198 (step 16, min/max pixels 655360–8294400, max edge 3840, max ratio 3).
- System prompt is prepended **into the user prompt string**, not as a real system role, for images (`withSystemPrompt` L305); for text a real `system` role is prepended (`withSystemMessage` L321).
- Response parsing is multi-shape: `data | images | results`, item `b64_json`→dataURL or `url` (`parseImagePayload` L244, `resolveImageSource` L233).
- Cost resolution priority: `usage.cost` → `/generation` lookup → token estimate (`resolveImageCost` L294).
- Scripted models: if `resolveModelScript` returns non-empty, the call is delegated to `runModelPlugin` (L470-491, L522-544, L575-593) instead of native HTTP.
- No retries inside this file. Retry/concurrency live in `canvas-generation-helpers.ts`: `GENERATION_CONCURRENCY=2`, `GENERATION_MAX_ATTEMPTS=3`, `GENERATION_RETRY_DELAY_MS=2000` (L153-155), `createGenerationQueue` L157, `runGenerationTaskWithRetry` L187.
- OCR is not a separate endpoint: `canvas-ocr.ts` builds a vision message and reuses `requestImageQuestion` (`canvas-ocr.ts:21-35`).

### 1.3 Video — `video.ts` (525 LOC, CORE)

Three providers selected at `createVideoGenerationTask` L136-156: `plugin` | `openrouter` | `openai` (L140).

| Provider | Create | Poll | Download |
|---|---|---|---|
| openai (multipart) | `createOpenAIVideoTask` L225: `POST /v1/videos` FormData `model,prompt,seconds,size,resolution_name,generate_audio,watermark,mode,(first_frame\|last_frame)\|image[],video[],audio[]` | `pollOpenAIVideoTask` L256: `GET /v1/videos/{id}`; on `status==="completed"` → `GET /v1/videos/{id}/content` (blob) | inline |
| openrouter (JSON) | `createOpenRouterVideoTask` L284: `POST /v1/videos` `{model,prompt,duration,resolution,aspect_ratio,frame_images?,input_references?,generate_audio?}` → `{id,polling_url}` | `pollOpenRouterVideoTask` L385: `GET {polling_url}\|/v1/videos/{id}`; terminal states completed/failed/cancelled/expired | `GET /v1/videos/{id}/content?index=0` (blob), fallback to `unsigned_urls[0]` (`downloadOpenRouterVideo` L402) |
| plugin | `createPluginVideoTask` L169: runs `runModelPlugin({capability:"video"})` once (create+poll inside script) | `pollVideoGenerationTask` L159: reads in-memory `pluginVideoResults` map L34 | result blob/url |

- Poll loop `waitForVideoGenerationTask` L97: **120 attempts × 2500 ms = 300 s max** (L98, L121); abortable `delay` L509.
- OpenRouter polling URLs cached in `openRouterVideoPollUrls` map L37, resolved L417.
- Reference normalization: mode `frames` vs `reference` (`resolveVideoMode` L455); OpenRouter frame slots `openRouterFrameImages` L313, mixed refs `openRouterInputReferences` L327; size/audio caps L23-25 (video 50 MB, audio 15 MB, request 64 MB).
- Error envelope unwrap `unwrapEnvelope` L479 (`code===0`), video failure error named `"VideoTaskFailed"` L130/126.
- Capabilities/gating in `D:\Dev\open-canvas\web\src\lib\video-generation.ts` (128 LOC): `openRouterVideoModels` L37 (hailuo-3-max / hailuo-3 with resolution/duration/audio/frame limits), `isOpenRouterVideoModel` L96, `videoModelCapability` L104, `supportedVideoResolution` L118, `videoModelDuration` L126.

### 1.4 Model-plugin script execution — `model-plugin.ts` (204 LOC, CORE)
- `runModelPlugin` L127: `new Function("prompt","images","videos","audios","messages","params","model","baseUrl","apiKey","systemPrompt","reasoningEffort","http","request","poll","sleep","signal","onDelta", "use strict; return (async () => { <script> })();")` — user-authored JS executed via `Function` constructor (no sandbox).
- Injected helpers: `createPluginHttp` L48 (`http.get/post` auto-Bearer, `http.url`), `createPluginRequest` L70 (raw axios, script controls method/url/headers), `createPoll` L95 (default interval 2500 ms, timeout 300000 ms), abortable `sleep` L77.
- `normalizePluginImages` L188 accepts string / `{dataUrl|url|b64_json}`.

### 1.5 OpenRouter account — `openrouter-account.ts` (56 LOC, SECONDARY)
- `fetchOpenRouterKeyUsage` L30: `GET https://openrouter.ai/api/v1/key` (Bearer) → `{data:{usage,usage_daily,usage_weekly,usage_monthly,limit,limit_remaining,limit_reset,is_free_tier}}`; 401/403→authError, offline→networkError.

### 1.6 Audio/Speech — NOT IMPLEMENTED
The config model declares `audioModel`/`speechModel` and presets list `fish-audio/s2.1-pro` (speech) and `google/lyria-3-pro-preview` (audio) (`provider-presets.ts:29-30`), but grep over `web/src/services` finds **no** `audio.ts`/`speech.ts`, no `/audio/speech`, no `/chat/completions`. Audio/speech are config-only today. This is a scope note, not a port task.

---

## 2. Provider / Model Config

`D:\Dev\open-canvas\web\src\stores\use-config-store.ts` (411 LOC, CORE)
- Capability enum L11; `ModelChannel` L20 `{id,name,baseUrl,apiKey,apiFormat:"openai",models:[{name,capability,script?}]}`; `AiConfig` L29 (channelMode, per-capability default model fields, generation params); `ModelRequestConfig = AiConfig & {baseUrl,apiKey}` L60.
- Storage: zustand `persist`, key `"open-canvas:ai_config_store"` L69, **localStorage** (no custom storage) L207.
- `defaultConfig` L74; channels seeded from `PROVIDER_PRESETS` L77.
- Model option encoding is namespaced: `encodeChannelModel` → `"<channelId>::<model>"` L319, `decodeChannelModel` L323, `modelOptionName` L329.
- **Resolution algorithm** for a request:
  1. `resolveModelChannel(config, value)` L356 — decode `channelId` or find owner channel by model name, else `channels[0]`.
  2. `resolveModelRequestConfig(config, value)` L364 → `{...config, model, baseUrl: channel.baseUrl||OPENROUTER_BASE_URL, apiKey: channel.apiKey}`.
  3. Capability default picker `resolveModelForCapability` L161; selectable list `selectableModelsByCapability` L169; script lookup `resolveModelScript` L175.
- Capability auto-guess from keyword lists L125-132 / `guessCapability` L135; text-only baseUrl forced to `text` L256,263.
- Credential import `importChannelCredentials` L196 → `upsertChannelCredentials` L290 (matches preset by normalized baseUrl, else creates channel).
- `buildGenerationConfig` (`canvas-generation-helpers.ts:71`) merges node metadata overrides then per-capability default before calls.

`D:\Dev\open-canvas\web\src\lib\provider-presets.ts` (55 LOC, CORE)
- `PROVIDER_PRESETS` L14: OpenRouter, DeepSeek (textOnly), Matilda `/matilda/v1` (textOnly). `isTextOnlyBaseUrl` L52.

Cost model: `D:\Dev\open-canvas\web\src\lib\canvas\generation-cost.ts` (66 LOC) — `MODEL_PRICES` L13, `MODEL_TOKEN_PRICES` L25, `estimateGenerationCost` L39, `estimateTokenCost` L46.

---

## 3. Local Models

### 3.1 Background removal — imgly ISNet
`D:\Dev\open-canvas\web\src\services\background-removal.ts` (48 LOC, SECONDARY) — module-level worker singleton; `removeImageBackground(source,onProgress)` L42 posts `{id,source}`.
`D:\Dev\open-canvas\web\src\services\background-removal.worker.ts` (16 LOC, SECONDARY) — imports `@imgly/background-removal`, calls `removeBackground(source,{model:"isnet_quint8", progress})` L8.
- Weights/assets: **not fetched by app code** — delegated to `@imgly/background-removal` (dependency `^1.7.0`, `web/package.json:17`), which downloads ONNX weights + wasm runtime internally and runs in the worker.
`D:\Dev\open-canvas\web\src\stores\use-local-model-store.ts` (117 LOC, CORE for this feature) — status/percent state, `READY_KEYS` in localStorage L27, `warmUps` L32, `warmUpBackgroundRemoval` L49 (runs a 64×64 dummy to force load), `prepareModel` L65, `clearModel` L91, `listLocalModels` L98.
Call site: `D:\Dev\open-canvas\web\src\pages\canvas\project.tsx:1530-1543`.

### 3.2 MobileSAM
`D:\Dev\open-canvas\web\src\lib\image\mobile-sam.ts` (176 LOC, CORE for this feature)
- `onnxruntime-web` (`web/package.json:41`).
- Encoder/decoder URLs: HuggingFace `PulpCut/mobilesam-onnx` L9-10; **Cache Storage** bucket `"mobilesam-onnx-v1"` L11; `fetchModel` L53 uses Cache API (cache-first, downloads with progress, `cache.put`).
- Config: `ort.env.wasm.numThreads=1` L15, `wasmPaths` = jsDelivr onnxruntime-web L16.
- `loadMobileSam` L67 creates two wasm InferenceSessions; `isMobileSamLoaded` L85; `resetMobileSam` L89 (releases + deletes cache).
- `segmentImageWithPoints` L139: preprocess (letterbox to 1024², RGB) `preprocess` L99 → `encoder.run({input_image})` → cached embedding L147-152 → `decoder.run({image_embeddings,point_coords,point_labels,mask_input,has_mask_input,orig_im_size})` L161 → argmax mask + composite (`buildMaskDataUrl` L121). Runs on main thread (no worker).
`D:\Dev\open-canvas\web\src\lib\image\mobile-sam-math.ts` (31 LOC, CORE, trivially portable): `MOBILE_SAM_INPUT_SIZE=1024` L1, `samResizeScale` L5, `samModelPoint` L10, `pickBestMaskIndex` L14, `clampSamScore` L22, `binarizeMaskLogits` L27.
Call site: `D:\Dev\open-canvas\web\src\components\canvas\canvas-node-segment-dialog.tsx:39,63`.

### 3.3 Native equivalent needs
- ONNX runtime native (`ort` crate) or `tract`; feed the same tensor names/dims (`input_image`, `image_embeddings`, `point_coords`, `point_labels`, `mask_input`, `has_mask_input`, `orig_im_size`).
- Weight acquisition + local cache (replace Cache Storage with a filesystem/model cache); ISPack for ISNet is internal to imgly so you need to source the ISNet ONNX yourself.
- Image decode/letterbox + mask compositing via `image` crate; SIMD/CoreML/CUDA EPs optional.
- `mobile-sam-math.ts` ports verbatim (pure float math).

---

## 4. Plugin System

### 4.1 Contract — `D:\Dev\open-canvas\web\src\types\canvas-plugin.ts` (147 LOC, CORE)
- `CanvasNodeResource` L9 `{kind:"image"|"video"|"text", text?, url?}`.
- `CanvasPluginAi` L22 — host-injected `generateImage/generateVideo/generateText/listModels/defaultModel` (plugins never see API keys).
- `CanvasNodeContext` L42 — node, theme, scale, isSelected, `updateMetadata/updateNode`, graph accessors, `applyOps(CanvasAgentOp[])`, `emit/on`, `ai`, `openPanel/closePanel`, `storage`.
- `PluginStorage` L70 — async get/set/remove.
- `CanvasPluginHost` L77 — node-id-based host capabilities.
- `CanvasNodeDefinition` L101 — type/title/icon/defaultSize/metadata, `Content?`, `Panel?`, `toolbar?`, `onDoubleClick?`, `resource?`, `useBuiltinPanel?`, `interactionToggle?`, `transparentBackground?`, etc.
- `CanvasPlugin` L138 `{id,name,version,description?,minAppVersion?,css?,nodes[],setup?(app)}`; `CanvasPluginApp` L129.
- Note: `plugin-registry.ts` imports `i18n` at the **bottom** of the file (L50) and uses it at L19 — quirk to preserve/clean during port.

### 4.2 Loading/eval — `D:\Dev\open-canvas\web\src\lib\canvas\plugin-loader.ts` (170 LOC, CORE)
- `evaluatePluginSource` L11: `new Blob([source])` → `URL.createObjectURL` → **dynamic `import(/* @vite-ignore */ url)`** (native ESM eval, no `eval`/`Function` for plugin code). Export is `default` or `.plugin`; if a function, called with `getPluginRuntime()` as a factory.
- `assertPlugin` L25; `activatePlugin` L31 (register nodes + `injectCSS` + `setup` cleanup), `deactivatePlugin` L42.
- `installPluginFromUrl` L61, `updatePlugin` L70 (cache-bust), `setPluginEnabled` L75, `uninstallPlugin` L87.
- `ensurePluginsLoaded` L95: rehydrate store → `loadLocalPlugins` L117 (fetch `/plugins/index.json`) → activate enabled (`record.local` refetches) → `loadDevPlugins` L153 (`VITE_DEV_PLUGINS`).
- Runtime bridge `D:\Dev\open-canvas\web\src\lib\canvas\plugin-runtime.ts` (42 LOC): `getPluginRuntime` L28 exposes **host `React`, `jsx`, `Fragment`, `injectCSS`, `version`, `emit`, `on`** on `window.OpenCanvasRuntime` L39.
- Node registry `D:\Dev\open-canvas\web\src\lib\canvas\node-registry.ts` (61 LOC): register/unregister/list, `getNodePluginId` L38, `useNodeRegistryVersion` L12.
- Official registry `D:\Dev\open-canvas\web\src\lib\canvas\plugin-registry.ts` (50 LOC): `fetchOfficialPlugins` L17 (fetch `PLUGIN_REGISTRY_URL`), `hasUpgrade` L47 (numeric semver). Registry URL constant: `D:\Dev\open-canvas\web\src\constant\env.ts:4` (jsDelivr `official-plugins.json`).
- Persisted installs `D:\Dev\open-canvas\web\src\stores\canvas\use-plugin-store.ts` (45 LOC): key `"open-canvas:plugin_store"`, **localforage** JSON storage; `InstalledPlugin` keeps `source` (cached bundle) + `url`.
- Event bus + storage `D:\Dev\open-canvas\web\src\lib\canvas\canvas-event-bus.ts` (47 LOC): `emitCanvasEvent` L9 / `onCanvasEvent` L19 (in-memory `Map<string,Set<Handler>>`); `createPluginStorage` L32 → `localforage.createInstance({name:"open-canvas-plugins", storeName: pluginId})` (per-plugin namespace).
- Context builder `D:\Dev\open-canvas\web\src\lib\canvas\plugin-node-context.ts` (30 LOC): `buildNodeContext` L8 wires host + namespaced storage.
- Host hook `D:\Dev\open-canvas\web\src\pages\canvas\hooks\use-plugin-host.tsx` (155 LOC, CORE): `pluginAi` L42 (generateImage L53, generateVideo L68, generateText L79, listModels L87, defaultModel L88 — all route into §1 services), `pluginHost` L92, `renderPluginPanel` L117, `buildNodeToolbarItems` L128 (+ host interaction toggle L136), `ensurePluginsLoaded()` on mount L150.

### 4.3 SDK (mirror of contract) — `D:\Dev\open-canvas\plugins\canvas\sdk\`
- `src/types.ts` (327 LOC) — public, self-contained mirror of the host contract.
- `src/runtime.ts` (37 LOC) — lazy global bridge; re-exports React hooks (`useState`…`useId`) forwarded to host React.
- `src/jsx-runtime.ts` (38) / `jsx-dev-runtime.ts` (16) — automatic JSX → host `React.createElement`.
- `src/define-plugin.ts` (18) — identity helper.
- `src/index.ts` (8) — barrel.
- `build.mjs` (75 LOC) — shared esbuild helper (ESM, `jsxImportSource:@open-canvas/plugin-sdk`, react external, css→text, syncs to `web/public/plugins` + writes `index.json`).

### 4.4 Official plugins — `D:\Dev\open-canvas\plugins\canvas\*`
Build orchestrator `registry/build.mjs` (69 LOC) builds all + emits `official-plugins.json`; `registry/package.json` registers the 5 official (template excluded).

| Plugin | LOC | Node type(s) | Capabilities | CDN deps |
|---|---|---|---|---|
| html | 138 | `html:render` | sandbox iframe `srcDoc`, `{{input}}` upstream injection, editor, `interactionToggle` | none |
| markdown | 125 | `markdown:doc` | edit/render MD, `resource.kind=text`, injects `css` | `https://esm.sh/marked@14` (L18) |
| panorama | 311 | `panorama:viewer` | equirectangular viewer, upload, `useBuiltinPanel` image-gen w/ promptPrefix, `resource.kind=image` | `https://esm.sh/three@0.180.0` (L10) |
| sticky-note | 161 | `sticky-note:note` | color palette, edit, custom metadata field `pluginColor` | none |
| svg | 97 | `svg:vector` | edit/render SVG, transparent bg, upstream SVG adoption, `resource.kind=text` | none |
| template | 69 | `template:node` | demo of `getUpstream` + `applyOps` | none |

Note: **no official plugin calls `ctx.ai`** today — grep finds `ctx.ai` only in the SDK types. Plugins rely on `useBuiltinPanel` (panorama) and host `ctx.updateMetadata`/`applyOps`.

---

## 5. Feasibility Notes

**Trivially portable (plain HTTP+JSON, no browser DOM beyond base64/multipart):**
- Image generate/edit (`POST /images`), text/vision SSE (`/responses`), OCR (same as text), video create/poll for `openai` and `openrouter`, generation-cost lookup, OpenRouter account.
- Port needs: `reqwest` + `serde_json`, an SSE line parser (`consumeResponseStreamBlock`), multipart (video openai), `AbortSignal`→`tokio::select!`/cancellation token, a polling loop (120×2.5 s; plugin poll 300 s), and a retry wrapper (3 attempts / 2 s / concurrency 2). Model resolution (§2) is pure data logic and ports directly; `encodeChannelModel` (`::`) and `/v1` URL normalization must be preserved for on-disk config compatibility, or migrated deliberately (project stores config in localStorage).

**Needs a native inference stack:**
- ISNet background removal and MobileSAM are ONNX. Options: `ort` (ONNX Runtime bindings) or `tract`. Must replace Cache Storage with a local model cache and the imgly convenience with a self-hosted ISNet ONNX. `mobile-sam-math.ts` ports verbatim.

**Plugin runtime — three-way decision:**
- The contract is **React + DOM-heavy**: `Content`/`Panel` are `React.ComponentType`, icons are `ReactNode`, `setup` receives host React, `injectCSS` touches `document.head`, storage is `localforage`, event payloads are arbitrary JS, plugins dynamically `import()` ESM from blobs/CDN (marked, three).
- In Slint there is no React and no DOM. To run existing JS plugins you would need an embedded JS engine (e.g. `rquickjs`/Boa) **plus** a DOM/React shim — high cost and fragile for the CDN `import()` and React rendering.
- Realistic paths: (a) **native plugin/node ABI** in Rust mirroring `CanvasNodeDefinition`/`CanvasNodeContext` (Content→Slint widget/`ComponentHandle`, Panel, toolbar, resource, storage, event bus) and port the 5 official plugins natively — medium effort, best fit; (b) embed a JS engine only for the `model-plugin` **request scripts** (`model-plugin.ts`, which is pure `fetch`+JSON, no DOM) — low risk, worth doing with `rquickjs`; (c) drop JS plugin UI entirely and keep only a native version of the most valuable nodes (markdown/svg/html/sticky-note are cheap natively; panorama needs a 3D renderer).
- Do **not** try to host React plugins inside Slint; the `window.OpenCanvasRuntime` + blob-`import()` design is inherently browser-bound.

**Cross-cutting:** the model-plugin script runner and plugin UI runtime are separate concerns with very different port costs. Decouple them.

---

## 6. LOC Triage

CORE (must port to keep parity):

| File | LOC |
|---|---|
| `web\src\services\api\image.ts` | 610 |
| `web\src\services\api\video.ts` | 525 |
| `web\src\services\api\model-plugin.ts` | 204 |
| `web\src\services\api\api-error.ts` | 71 |
| `web\src\stores\use-config-store.ts` | 411 |
| `web\src\lib\provider-presets.ts` | 55 |
| `web\src\lib\video-generation.ts` | 128 |
| `web\src\lib\canvas\canvas-generation-helpers.ts` | 205 |
| `web\src\lib\media-size.ts` | 155 |
| `web\src\lib\canvas\generation-cost.ts` | 66 |
| `web\src\lib\image\mobile-sam.ts` | 176 |
| `web\src\lib\image\mobile-sam-math.ts` | 31 |
| `web\src\stores\use-local-model-store.ts` | 117 |
| `web\src\types\canvas-plugin.ts` | 147 |
| `web\src\lib\canvas\plugin-loader.ts` | 170 |
| `web\src\lib\canvas\plugin-runtime.ts` | 42 |
| `web\src\lib\canvas\node-registry.ts` | 61 |
| `web\src\lib\canvas\canvas-event-bus.ts` | 47 |
| `web\src\lib\canvas\plugin-node-context.ts` | 30 |
| `web\src\pages\canvas\hooks\use-plugin-host.tsx` | 155 |
| `plugins\canvas\sdk\src\types.ts` | 327 |

SECONDARY / thin:

| File | LOC |
|---|---|
| `web\src\services\api\openrouter-account.ts` | 56 |
| `web\src\services\background-removal.ts` | 48 |
| `web\src\services\background-removal.worker.ts` | 16 |
| `web\src\lib\canvas\canvas-ocr.ts` | 36 |
| `web\src\lib\canvas\plugin-registry.ts` | 50 |
| `web\src\stores\canvas\use-plugin-store.ts` | 45 |
| `web\src\lib\image-reference-prompt.ts` | 14 |
| `plugins\canvas\sdk\src\{runtime,define-plugin,index,jsx-runtime,jsx-dev-runtime}.ts` | 37/18/8/38/16 |
| `plugins\canvas\sdk\build.mjs` | 75 |
| `plugins\canvas\registry\build.mjs` | 69 |
| official plugin `index.tsx` | html 138 / md 125 / panorama 311 / sticky 161 / svg 97 / template 69 |

Supporting (referenced, not in scope but required): `web\src\services\image-storage.ts` (297), `web\src\services\file-storage.ts` (103), `web\src\components\canvas\canvas-node-generation.ts` (228), `web\src\lib\provider-presets.ts` (55), `web\src\constant\env.ts` (4).

<results>
<files>
- D:\Dev\open-canvas\web\src\services\api\image.ts - image generate/edit + text/vision SSE; forces IMAGE_MODEL; cost lookup
- D:\Dev\open-canvas\web\src\services\api\video.ts - video create/poll for openai/openrouter/plugin providers; 300s poll
- D:\Dev\open-canvas\web\src\services\api\model-plugin.ts - user JS script runner via `new Function`, injects http/request/poll/sleep
- D:\Dev\open-canvas\web\src\services\api\api-error.ts - shared error unwrap + status mapping
- D:\Dev\open-canvas\web\src\services\api\openrouter-account.ts - GET /key usage
- D:\Dev\open-canvas\web\src\stores\use-config-store.ts - channels/model config, encoding, resolveModelRequestConfig, buildApiUrl
- D:\Dev\open-canvas\web\src\lib\provider-presets.ts - 3 provider presets
- D:\Dev\open-canvas\web\src\lib\video-generation.ts - OpenRouter video capabilities/gating
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-generation-helpers.ts - buildGenerationConfig + retry/queue
- D:\Dev\open-canvas\web\src\services\background-removal.ts - imgly worker bridge
- D:\Dev\open-canvas\web\src\services\background-removal.worker.ts - @imgly/background-removal isnet_quint8
- D:\Dev\open-canvas\web\src\lib\image\mobile-sam.ts - onnxruntime-web MobileSAM, HF weights + Cache Storage
- D:\Dev\open-canvas\web\src\lib\image\mobile-sam-math.ts - pure SAM math (verbatim portable)
- D:\Dev\open-canvas\web\src\stores\use-local-model-store.ts - local model prepare/clear/status
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-ocr.ts - OCR via requestImageQuestion
- D:\Dev\open-canvas\web\src\types\canvas-plugin.ts - host plugin contract
- D:\Dev\open-canvas\web\src\lib\canvas\plugin-loader.ts - blob+dynamic import ESM eval, install/activate
- D:\Dev\open-canvas\web\src\lib\canvas\plugin-runtime.ts - window.OpenCanvasRuntime (React/injectCSS/emit/on)
- D:\Dev\open-canvas\web\src\lib\canvas\plugin-registry.ts - official manifest fetch + semver
- D:\Dev\open-canvas\web\src\lib\canvas\node-registry.ts - node definition registry
- D:\Dev\open-canvas\web\src\lib\canvas\canvas-event-bus.ts - event bus + namespaced localforage storage
- D:\Dev\open-canvas\web\src\lib\canvas\plugin-node-context.ts - buildNodeContext
- D:\Dev\open-canvas\web\src\pages\canvas\hooks\use-plugin-host.tsx - host ai/host capability injection
- D:\Dev\open-canvas\web\src\stores\canvas\use-plugin-store.ts - persisted InstalledPlugin list
- D:\Dev\open-canvas\plugins\canvas\sdk\src\types.ts - public contract mirror
- D:\Dev\open-canvas\plugins\canvas\sdk\build.mjs - shared esbuild plugin builder
- D:\Dev\open-canvas\plugins\canvas\registry\build.mjs - official plugins build + manifest
- D:\Dev\open-canvas\plugins\canvas\{html,markdown,panorama,sticky-note,svg,template}\src\index.tsx - official plugin implementations
</files>

<answer>
Two subsystems, clearly separable for porting:

(1) AI services are almost entirely plain HTTP+JSON over an OpenAI-compatible `/v1` surface (images, responses/SSE, videos, key usage). Port shape: reqwest+serde_json, Bearer auth, `/v1` URL normalization, SSE parser, multipart for OpenAI video, abortable poll loops (video 120×2.5s; plugin poll 300s), retry wrapper (3 attempts/2s/concurrency 2). The only non-HTTP dependency is the user-authored "model plugin" script runner (`model-plugin.ts`, `new Function` with injected http/request/poll) — pure fetch logic, so an embedded JS engine (e.g. rquickjs) would cover it without a DOM. Model resolution is pure data logic: channels carry baseUrl/apiKey/models; models are addressed as `<channelId>::<model>`; image ops hard-force `openai/gpt-image-2.5-sunburst`; text uses config.model||textModel. Audio/speech capabilities exist only in config, with no request implementation.

(2) Local models are ONNX (ISNet via the imgly library which self-hosts weights; MobileSAM via onnxruntime-web with HF weights cached in Cache Storage). Native port needs an ONNX runtime (ort/tract), a filesystem model cache, and image pre/post-processing; the SAM math module ports verbatim.

(3) The plugin system is fundamentally browser/React-bound: bundles are ESM blobs dynamically imported, the runtime injects the host React instance, node Content/Panel are React components, storage is localforage/IndexedDB, styles go to document.head, and plugins pull deps (marked, three) from CDNs at runtime. There is no DOM/React in Slint, so running these plugins as-is requires an embedded JS engine plus React+DOM shims (high cost, fragile). Recommended: define a native node/plugin ABI mirroring CanvasNodeDefinition/CanvasNodeContext and port the five official plugins natively; keep any JS engine scoped only to the DOM-free model-plugin request scripts.
</answer>

<next_steps>
1. Rust HTTP client: implement image generate/edit (`POST /images`), text/vision SSE (`POST /responses`), video create/poll (openai multipart + openrouter JSON with `polling_url`), `/generation` cost lookup, and OpenRouter `/key`. Mirror `buildApiUrl` `/v1` normalization and Bearer headers; port `api-error` unwrap rules.
2. Provider/model model: port `ModelChannel`/`AiConfig`, `::` encoding, `resolveModelRequestConfig`, capability defaults and keyword guessing. Decide whether to keep the localStorage JSON schema byte-compatible or migrate.
3. Local models: pick `ort` (ONNXRuntime) vs `tract`; self-host ISNet ONNX (imgly convenience is unavailable natively); reimplement MobileSAM encoder/decoder with a filesystem model cache; port `mobile-sam-math.ts` unchanged.
4. Plugins: prototype a native node ABI (definition + context + storage + event bus) and port markdown/svg/html/sticky-note first (panorama needs 3D); separately decide on rquickjs for `model-plugin` scripts only.
5. Confirm scope for audio/speech (config-only today) before building any client.
Ready to proceed - no follow-up needed.
</next_steps>
</results>