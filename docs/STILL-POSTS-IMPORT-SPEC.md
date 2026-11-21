# Story Studio KH → Wire Studio KH: Still Posts import spec

Scope: read-only mapping checked 2026-11-21. `$STORY` = `/build/Developer/My Own Apps/story-studio-kh`; `$WIRE` = `/build/Developer/My Own Apps/wire-studio-kh`; `$DATA` = `~/Library/Application Support/Story Studio KH`; `$PLAN` = `/build/Developer/_orchestra/youtube-stack/wire-studio-scope/PLAN-V2.md`.

## 1. Storage map

Story has **no SQLite table for Still Posts** — JSON + PNG only (`$STORY/electron/server/services/still-posts.ts:253-290`). Wire today has `posts(id TEXT PRIMARY KEY, text TEXT NOT NULL, created_at TEXT NOT NULL)` and `deliveries(...)` (`$WIRE/src-tauri/src/queue/store.rs:17-44`).

Resolution (global vs per-channel):
- Data dir: `STORY_STUDIO_DATA_DIR` else Electron `app.getPath('userData')` else `$DATA` (`$STORY/electron/server/services/project-manager.ts:302-309`).
- Library root: `app-settings.json → library_root` (legacy `projects_root_override`) else `<dataDir>/projects`; env override wins (`project-manager.ts:245-280,249-251`).
- Channel = `<libraryRoot>/<slug>/` (`project-manager.ts:2001-2004`); settings `<channel>/config.json` (`project-manager.ts:1400-1403`); manifest `<channel>/project.json` (`project-manager.ts:599-600,849`).

Per-channel files (exact paths; all under `<channel>/`):
- `still-posts/posts/post-NNNN/post.json` — post record: `id, batch_id, created_at, updated_at, format_id, option_id, language, character_ids, caption, panels, finals, aspects, art_mode?, font_id?, approved_at?, redraws?, published_at?` (`services/still-posts.ts:65-67,86-105,253-260`; `contracts/still-posts.ts:209-249`).
- `still-posts/posts/post-NNNN/p<N>.png` (single mode); `p<N>-1x1.png`, `p<N>-9x16.png`, `p<N>-16x9.png` (per-size) (`services/still-posts.ts:107-110`).
- `still-posts/posts/post-NNNN/final-p<N>-<1x1|9x16|16x9>.png` — lettered finals; record fields `aspect, panel_id, filename, rendered_at, text, style_hash, font_id` (`services/still-posts.ts:112-115,943-979`; `contracts/still-posts.ts:197-207`).
- `still-posts/font-pool-cursor.json` — `{"cursors":{"<script>":{"last_font_id":string|null,"last_index":int|null}}}` (`services/still-posts.ts:455-489,500-518`).
- `config.json → still_post_presets[].data.font_pools`, `still_post_last_preset` — pools keyed by ISO-15924 script: `{usage_mode:"serial"|"weighted", allow_repeat:bool, fonts:[{font_id,weight}]}` (`services/still-posts.ts:1010-1031,1040-1064`; `contracts/still-post-fonts.ts:44-72`).
- `config.json → still_posts{character_ids(1–10),logo_path,handle}`, `social_accounts`, `social_platforms`, `channel_profile.handle`, `channel_logo_path` (`contracts/still-posts.ts:261-275`; `services/still-posts.ts:142-198`; `services/social-connection-store.ts:298-324`).
- `still-posts/posts/post-NNNN/publish.json` — `facebook{post_id,permalink?,file,file_sha256,posted_at}`, `instagram?`, `last_failure?` (`services/still-post-publish.ts:49-66,88-100,191-212`).
- `<channel>/_trash/still-posts/redraws/post-NNNN/<stamp>/<file>` and `_trash/still-posts/post-NNNN/` — replaced art/removed posts, **not import input** (`contracts/still-posts.ts:251-259`; `services/still-posts.ts:344-362,386-408`).
- `<libraryRoot>/.character-library/_channels/<slug>/<characterId>/<file>` (legacy `<libraryRoot>/.character-library/<characterId>/<file>`) — reference images named by `character_ids` / `image.reference_character_ids` (`services/character-manager.ts:69-75`; `generators/still-posts.ts:548-560`).

Global (per Mac):
- `<dataDir>/social/connections.json` — `schema_version:1`, `applications<meta|tiktok>{application_id,client_id,redirect_uri,configured_at,updated_at}`, `accounts["<platform>:<remote_id>"]{platform,provider,application_id,remote_id,remote_name,page_id?,connected_at,updated_at,status}` (`services/social-connection-store.ts:34-49,80-103,138-146`).
- `<dataDir>/app-secrets.json` (0600) — `social.applications[provider].client_secret`, `social.account_tokens["<platform>:<remote_id>"]{application_id,access_token,refresh_token,expires_at,scope?,page_token?,page_token_expires_at?}` (`services/app-secrets-store.ts:37-61,97-125,316-321`; `services/social-connection-store.ts:230-282`).
- Font catalogue: bundled `<repo>/bundled/fonts/fonts.json`, packaged `Contents/Resources/fonts/fonts.json`; entries carry `id,label,scripts,weights,role,license,source,file,sha256,family,license_file,source_url` (`electron/server/lib/thumbnail-fonts.ts:39-59,66-104`; file inspected).
- Niches: built-in `<repo>/electron/server/niches/still-posts.json` (packaged `Contents/Resources/niches/`), user `<libraryRoot>/.niches/<id>.json` (`services/niche-loader.ts:7-9,83-97`); channel picks `project.json → niche_id` (+ `topic_pack_ids`, `niche_overrides`) (`niche-loader.ts:728-752`; `project-manager.ts:849`).

Counts: no persisted counter. Channel count = readable `post-*` folders (`services/still-posts.ts:263-280`); batch `total/complete_count/complete/missing` are derived from records + files (`services/still-posts.ts:657-686`; `contracts/still-posts.ts:357-367`).

## 2. Record shape (one stored post)

Enums/defaults: aspects `1:1|9:16|16:9` (`contracts/still-posts.ts:23-30`); languages `en|km|zh` → scripts `latn|khmr|hani` (`56-72`); panel roles `single|setup|punchline` (`124`); art mode `single|per-size`, absent = `single` (`43-47,226-230`); new posts have `image:null`, `finals:[]`, `published_at:null`, caption ≤2000, no `approved_at` (`generators/still-posts.ts:243-266`; `services/still-posts.ts:296-331`).

```json
{
  "id": "post-0001", "batch_id": "batch-20261101-REDACTED",
  "created_at": "2026-11-01T01:02:03.000Z", "updated_at": "2026-11-01T02:03:04.000Z",
  "format_id": "quiet-ink", "option_id": "centred-quote", "language": "km",
  "character_ids": ["char-REDACTED"], "caption": "REDACTED SHORT CAPTION",
  "panels": [{
    "id": "panel-0", "role": "single", "text": "REDACTED KHMER LINE",
    "scene_prompt": "REDACTED SCENE PROMPT",
    "image": {
      "filename": "p0.png", "rendered_at": "2026-11-01T01:30:00.000Z",
      "sha256": "REDACTED_HEX", "model_id": "REDACTED_MODEL_ID",
      "model_label": "REDACTED MODEL LABEL", "prompt": "REDACTED FULL PROMPT",
      "reference_character_ids": ["char-REDACTED"], "reference_names": ["REDACTED NAME"]
    }
  }],
  "finals": [{
    "aspect": "1:1", "panel_id": "panel-0", "filename": "final-p0-1x1.png",
    "rendered_at": "2026-11-01T01:40:00.000Z", "text": "REDACTED KHMER LINE",
    "style_hash": "REDACTED_HEX", "font_id": "kantumruy-pro"
  }],
  "aspects": ["1:1", "9:16"], "font_id": "kantumruy-pro", "published_at": null
}
```
Illustrative ids/labels only; real ids come from `$STORY/electron/server/still-posts/catalog.ts`. Images may also carry `mode?`, `aspect?`, `generation_action?` (`contracts/still-posts.ts:154-177`) and are written at `generators/still-posts.ts:548-576`.

## 3. Mapping to Wire (`$PLAN` §4/§6)

- **Post/recipe:** `StillPost → wire.db posts(id, text=caption, created_at)`; put format look + option layout/text boxes + lettering + branding + text_scale + aspects + art_mode + batch/approval/publish state in `recipes/<id>.json`; never in deliveries (`contracts/still-posts.ts:86-150,209-249,279-325`; `PLAN-V2.md:317-325,342-344`). Wire has no recipe/import implementation (`queue/store.rs:17-44`; no `recipe`/`import` found in `$WIRE/src-tauri/src` or `$WIRE/ui/app.js` on 2026-11-21).
- **Images:** copy `p<N>*.png` + `final-p<N>-*.png`; pool metadata `provenance:"imported:story"`, SHA-256, `rendered_at`, `model_id/model_label`, exact prompt, reference character ids/names, aspect, art mode, `style_hash/font_id` (`generators/still-posts.ts:548-576`; `services/still-posts.ts:943-979`; `PLAN-V2.md:317-325,342-344`). Verify bytes against `image.sha256`; skip `_trash`. Wire `pool/index.json` is planned, not implemented.
- **Fonts:** import per-script `font_pools` in pool order plus the channel cursor as one-time state, and copy coverage metadata (`scripts`, `weights`, `role`, `license`, source `sha256`) from `fonts.json`; never invent a missing face (`contracts/still-post-fonts.ts:44-72,93-121`; `services/still-posts.ts:462-518,528-550`; `thumbnail-fonts.ts:48-59,96-104`; `PLAN-V2.md:322,343`). Wire `fonts/` is planned, not implemented.
- **Niches → idea presets:** import only the `still-posts` niche (built-in and any channel override): `id/name/description`, `exampleTopics`, `ideaSystemPromptRole`, `ideaGuidanceText`, `scriptContext`, `stillPostsFormat{writeSystemPromptRole,quoteGuidance}`, `narrativeQualityContract` (`niches/still-posts.json:1-14,43-60,147-150`; `generators/still-posts.ts:93-101,137-146`; `niche-loader.ts:728-752`). Wire idea presets are planned, not implemented (`PLAN-V2.md:339-350`).
- **Social accounts:** import public records (`platform, provider, application_id, remote_id, remote_name, page_id?, connected_at, updated_at, status`) and per-channel `social_accounts/social_platforms` into Wire account records (`contracts/social-publishing.ts:77-110`; `social-connection-store.ts:34-39,298-324`). Move tokens into Wire's one 0600 `app-secrets.json` under a new `social` record, never `wire.db`; current Wire `Secrets` only has `telegram` and `operator_token` (`$WIRE/src-tauri/src/secrets.rs:13-25`). An idempotent `imports/` ledger is required but does not exist yet (`PLAN-V2.md:339-347`).

## 4. Conflicts, unknowns, exclusions

1. **Fidelity gap:** Story's look is a composed layer stack (lettering, text panel, handle/logo, canvas, style_hash) with per-size drawing and fitted finals; Wire has no renderer, recipes, fonts, pool or idea presets (`contracts/still-posts.ts:86-120,137-150,197-230`; `queue/store.rs:17-44`; `PLAN-V2.md:317-350`). Import original PNGs + opaque metadata, mark recipe "unrenderable by Wire", not "ready".
2. **Token/schema gap:** Story has a Meta client secret plus separate user and Page tokens (`app-secrets-store.ts:97-125`); Wire's secrets schema has no account/token fields (`secrets.rs:13-25`). No token may be flattened into posts or logs.
3. **State ownership:** font cursor, approvals, `publish.json` receipts and `_trash` redraws are tied to exact bytes (`services/still-posts.ts:421-446,455-518`; `still-post-publish.ts:49-66`). Copy the cursor once under the ledger; do not import redraws or treat receipts as Wire deliveries (Instagram publishing is refused, `still-post-publish.ts:131-141`).

Story-side change: none for read-only reads/byte copy; exact re-render or atomic multi-file snapshot would need a Story export endpoint (format/option catalogue is source code, not channel data: `generators/still-posts.ts:421-429`). Must NOT import: video channels/episodes/jobs, YouTube or other OAuth providers, other projects/niches, `_trash`, browser caches/cookies, another Mac's files, or plaintext tokens into non-0600 files. Retirement of Story channels is out of scope until Wire's import report is spot-checked on the Production Mac (`PLAN-V2.md:347-350`). Unknown/not found here: live channel inventory, `test-channel-kh`, any channel `config.json`/`post.json`/`font-pool-cursor.json` under `$DATA` or `_orchestra` test data (§6).

## 5. Read-only proof for the importer gate

Only `stat()` + reads; fail unless `unchanged` is true and files > 0. Before/after comparison covers mtime_ns, size and SHA-256 of every file the importer reads.

```sh
STORY_CHANNEL_DIR='/path/to/libraryRoot/<slug>' python3 -I - <<'PY'
import hashlib, json, os, pathlib
root = pathlib.Path(os.environ['STORY_CHANNEL_DIR'])
paths = sorted(p for p in root.rglob('*') if p.is_file() and (
    'still-posts' in p.parts or p.name in ('config.json', 'project.json', 'publish.json')))
def snap():
    return {str(p): (p.stat().st_mtime_ns, p.stat().st_size,
                     hashlib.sha256(p.read_bytes()).hexdigest()) for p in paths}
before = snap()
for p in paths:
    if p.suffix == '.json':
        json.loads(p.read_text('utf-8'))
after = snap()
print(json.dumps({'channel': str(root), 'files': len(paths), 'unchanged': before == after}))
raise SystemExit(0 if before == after and paths else 1)
PY
```

## 6. Inventory on THIS Mac

- `$DATA` listed 2026-11-21: only Electron browser/cache/log folders (`Cache/`, `Code Cache/`, `GPUCache/`, `Local Storage/`, `Session Storage/`, `logs/`, Cookies/Preferences). No `projects/`, `app-settings.json`, `social/` or `app-secrets.json`, so the Library root cannot be resolved here (`project-manager.ts:245-280,302-309`).
- `test-channel-kh`: not found under `/build/Developer` or `/build/Desktop`.
- Only local test data found: `/private/var/folders/qp/64pbc0jn23x2ytbmpzp0rxgm0000gn/T/story-e2e-1416-WoTnhn/` (`projects/e2e-project/project.json`, `jobs.json`, `app-settings.json`), with no `still-posts/` folder.
- Live Still Posts channels discoverable here: **not found (0)**. Production's real inventory is on `story-mini` and cannot be enumerated from this Mac (`PLAN-V2.md:347-350`). The gate must pass a source library root explicitly, never guess one.
