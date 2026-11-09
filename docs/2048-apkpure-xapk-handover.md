# APK Handover: 2048 APKPure XAPK 1.7.0

**Purpose:** persistent Stride test-target specification so the user does not need to re-upload the same sample just to explain its requirements.

## Intake identity

- Original file: `2048_1.7.0_APKPure.xapk`
- Package: `com.tpcstld.twozerogame`
- Display name: `2048`
- Version name/code: `1.7.0` / `1013`
- XAPK format: v2
- File size: 3,453,701 bytes (about 3.29 MiB)
- SHA-256: `e3a9c160ea34bfa916e0022dcacb3da07732042c1f2912f52b7bbdc4485ec3a7`
- Inspection date: 2026-11-09
- Inspection type: static ZIP/DEX inspection only; not installed or executed.

## XAPK and split structure

Archive has 21 entries:
- Base APK: `com.tpcstld.twozerogame.apk` (2,990,995 bytes).
- 18 config split APKs: `config.ko`, `config.es`, `config.fr`, `config.hi`, `config.pt`, `config.zh`, `config.ar`, `config.in`, `config.my`, `config.tr`, `config.xxhdpi`, `config.de`, `config.it`, `config.th`, `config.vi`, `config.en`, `config.ja`, `config.ru`.
- `icon.png` and `manifest.json`.

XAPK manifest metadata:
- min SDK 23 (Android 6.0).
- target SDK 37.
- Declared XAPK permission: `com.tpcstld.twozerogame.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION`.
- This must be handled as a split package: base plus relevant configuration APKs. Never try to launch a config split by itself.

## DEX and native inventory

Base APK has exactly one DEX:
- `classes.dex`, DEX version 035.
- 7,059,288 bytes.
- 45,835 string IDs; 7,882 type IDs; 12,059 proto IDs; 17,860 field IDs; 51,183 method IDs; 6,502 class definitions.
- No `classes2.dex`.
- No packaged `lib/**/*.so` was found in base or config splits.
- Asset `assets/ClearSans-Bold.ttf` (269,792 bytes).
- Baseline profile: `assets/dexopt/baseline.prof` and `baseline.profm`.

No app-packaged ELF library was detected. Therefore native ARM64 execution, Bionic symbol resolution, and JNI native-library loading are not first-order requirements for this exact sample unless runtime diagnostics reveal a dynamically supplied dependency.

## App-specific class and API profile

App classes identified in DEX:
- `MainActivity`, `MainGame`, `MainView`
- `Grid`, `Tile`, `Cell`
- `AnimationGrid`, `AnimationCell`
- `InputListener`
- `SnapshotManager`, `SnapshotData`
- `DebugTools`

Key app method names:
- Activity: `onCreate`, `onResume`, `onPause`, `onSaveInstanceState`, `onKeyDown`, `load`, `save`.
- Game logic: `move`, `moveTile`, `spawnTile`, `addRandomTile`, `addStartTiles`, `checkLose`, `gameWon`, `gameLost`, `newGame`, `undo` state save/restore helpers.
- View/rendering: `onDraw`, `onSizeChanged`, `drawBackground`, `drawBackgroundGrid`, `drawCells`, `drawCellText`, `drawScoreText`, `drawUndoButton`, `drawNewGameButton`, `tick`.
- Input: `onTouch`, `isTap`, `pathMoved`.

DEX references `android.graphics.Canvas`. The class/method inventory strongly indicates a custom Android View with Canvas-based 2D drawing, gesture input, game-loop animation, and state persistence. This is a DEX/framework/Canvas test, not a Unity, Cocos2d-x, OpenGL, or native-game test.

## Stride implementation requirements

### P0 — real launch and playable core
1. Parse the XAPK manifest and assemble base + selected config splits.
2. Decode base APK binary AXML manifest and resolve the exact launcher Activity, theme, orientation, exported state, app metadata, and component declarations. The exact launcher component is not yet confirmed by this static scan.
3. Load the one DEX and preserve its exact string/type/proto/field/method table indices.
4. Resolve app classes and their framework/library method references.
5. Execute Activity construction and lifecycle: create/start/resume, pause/stop, save/restore, and back/key handling.
6. Implement View creation, dimensions, invalidation, content attachment, and `onDraw(Canvas)`.
7. Implement the Canvas methods actually invoked by `MainView`: rectangles, text, colors, bitmaps, text metrics/alignment, clipping, transforms, and density-aware coordinates. Confirm exact signatures through full disassembly before coding.
8. Deliver touch events with correct action, pointer coordinates, ordering, and gesture movement so swipe and button input work.
9. Implement animation tick/frame scheduling without stalling or excessive CPU use.
10. Resolve resources: strings, colors, dimensions, styles, drawables, density variants, and the bundled font.
11. Persist and restore board state, score, high score, and undo state through the app's save/snapshot path.
12. Handle target SDK 37 behaviors when they affect receiver registration, permissions, window insets, or component rules.

### P1 — secondary features / integration
- DEX contains AndroidX Activity, AppCompat/Core, Lifecycle, SavedState, Startup, and ProfileInstaller references. Implement/bridge only reached methods and preserve their expected semantics.
- Google Play Services Auth and Games/leaderboard-related references are present. Determine whether login/leaderboards are optional. Core offline gameplay should not depend on successful Play Games login.
- Kotlin/coroutine-related references are present in the DEX inventory; do not assume a full coroutine runtime is needed before tracing actual call paths.
- Verify system bar/window inset handling, configuration changes, animation pacing, and memory use.

### Not detected / not a current requirement
- Packaged JNI native libraries or ELF shared objects.
- OpenGL ES/Vulkan/SurfaceView renderer.
- App-specific audio engine.
- These are absence-of-packaged-evidence findings, not proof that no dynamic path could ever request such APIs.

## Acceptance test plan

1. XAPK split parsing and correct base/config selection.
2. DEX parse and class inventory; identify and instantiate the real launcher Activity.
3. Confirm the displayed screen is produced by this APK's own code — never a Stride fake/demo screen.
4. Confirm first board renders through guest `MainView.onDraw(Canvas)`.
5. Test all four swipe directions, tile merge/movement, score updates, undo, new game, win/lose overlays, and animation.
6. Pause/resume, force close/relaunch, and verify board/high-score restoration.
7. Verify offline play independently of Google Play Services.
8. Export launch-to-exit diagnostics: missing classes/methods/APIs, unsupported DEX opcodes, Canvas API coverage, frame timing/FPS, and memory.
9. Add every fixed issue to regression tests.
10. Mark compatibility only after the real third-party APK is playable end-to-end on an actual iPhone. Record APK hash, iPhone/iOS version, and test result.

## Compatibility classification

- XAPK structure: base + 18 configuration splits; split handling required.
- Native execution: low burden from packaged contents (no `.so` found).
- DEX/framework: moderate-to-high burden (large single DEX plus AndroidX and Google Play Services references).
- Graphics/input: Canvas 2D custom View and touch gestures are central.
- Recommended role: early real-APK integration target for DEX execution, Activity lifecycle, View/Canvas rendering, touch input, resources, and persistence.
- Status: **statically inspected only; not confirmed runnable in Stride**.

## Limitations of this scan

The binary AndroidManifest.xml was not fully decoded with aapt/apktool and no complete JADX/Androguard decompilation was performed. Exact launcher component, full manifest permission list, exact external API call paths, and exact Canvas signatures remain to be confirmed in a deeper disassembly. DEX string/type references do not prove every referenced SDK feature is mandatory.
