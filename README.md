# Simple Voice Chat for PumpkinMC

This plugin implements the backend compatibility needed to host the [Simple Voice Chat](https://modrinth.com/plugin/simple-voice-chat) mod on a [PumpkinMC](https://github.com/Pumpkin-MC/Pumpkin) server. It allows players connecting with modern Minecraft clients (Fabric, Forge, NeoForge) to use proximity voice chat and dynamically created voice groups.

## Key Features

- **Proximity Chat**: Accurately simulates dimensional audio using 3D vector coordinates sent directly to your game client.
- **Group Channels**: Full support for the GUI group lifecycle (creating, publishing, joining, leaving, and removing empty groups), including passwords and all three upstream group types.
- **Dynamic Audio Categories**: Create custom audio categories via configuration to differentiate audio streams (e.g. Radio, Global Broadcast) with custom names and descriptions.
- **Packet Rate Limiting**: Built-in protection against network flooding using a leaky-bucket limiter, with separate budgets for UDP voice traffic and TCP plugin messages.
- **Permissions Support**: Fully respects the native PumpkinMC permission node trees, with cooldown-limited on-screen feedback when speaking or listening is denied.
- **Translations**: A JSON translation registry discovered by file name — `en_us` and `zh_cn` ship built in, and admins can add or override any language from the plugin data folder without recompiling.
- **Optimized Transport**: Connects entirely over UDP with lightweight `AES-128-GCM` encryption for optimal performance.

---

## Tech Stack

- **Language**: Rust (edition 2024), compiled to a `wasm32-wasip2` WebAssembly component
- **Framework**: [`pumpkin-plugin-api`](https://github.com/Pumpkin-MC/Pumpkin) (PumpkinMC Plugin SDK)
- **Networking**: non-blocking `std::net::UdpSocket` driven by the host scheduler (no async runtime inside the WASI sandbox)
- **Cryptography**: `aes-gcm` (AES-128-GCM) suite for packet serialization matching JVM mod signatures
- **Configuration**: `serde` / `toml`

---

## Prerequisites

Before setting up the plugin, make sure you have the following installed on your machine:
- The [Rust Toolchain](https://rustup.rs/) (`cargo`, `rustc`).
- A built and running instance of the [PumpkinMC](https://github.com/Pumpkin-MC/Pumpkin) Server.
- A Minecraft Client with the [Simple Voice Chat Mod](https://modrinth.com/plugin/simple-voice-chat/versions) installed.

---

## Getting Started

### Download Pre-Release Binaries

We provide pre-built WASM components under the Releases tab.

1. Download the latest `pumpkin_voice.wasm` from the Releases page.
2. Place the downloaded `.wasm` file directly into your PumpkinMC server's `plugins/` directory.

### Build from Source (Rust)

If you prefer to compile the plugin yourself or are contributing to development:

1. **Clone the Repository**
   ```bash
   git clone https://github.com/hmdnnrmn/PumpkinVoice.git
   cd PumpkinVoice
   ```

2. **Install the WASM Target**
   Ensure you have the WebAssembly target installed:
   ```bash
   rustup target add wasm32-wasip2
   ```

3. **Build the Plugin**
   Compile the plugin to a WASM component:
   ```bash
   cargo build --release --target wasm32-wasip2
   ```

4. **Run the Unit Tests** (optional)
   The test binary is a WASM component that imports the Pumpkin host interfaces, so it cannot be instantiated by a bare `wasmtime`. Run the pure-Rust logic tests on the host target instead:
   ```bash
   cargo test --target x86_64-unknown-linux-gnu
   ```

5. **Deploy the Executable**
   Once compiled, move the output WASM file into your server's plugin pool:
   ```bash
   cp target/wasm32-wasip2/release/pumpkin_voice.wasm /path/to/pumpkin/plugins/
   ```

### Adjust Server Configurations & Connect

The first time you boot the server, the plugin will construct a default configuration file at `plugins/data/pumpkin_voice/config.toml`.

Only the current complete configuration format is accepted. Missing required fields and unknown fields (including obsolete settings) are errors; there is no legacy configuration migration or historical plugin-data import. Defaults are generated only when the file does not exist. Existing invalid files are reported and left untouched, not silently repaired or overwritten. Category descriptions remain optional.
By default, the plugin will span out a UDP listener concurrently running on port `24454`.

Connect via your Minecraft client. Look at the bottom left of your screen, you should see no "Unplugged" symbol. Press <kbd>V</kbd> to open up the Simple Voice Chat UI to guarantee that the UI says "Voice Chat Connected".

---

## Commands

PumpkinMC directly delegates commands to the plugin via the Brigadier argument mapping interface. `/voicechat` also answers to `/vc`. Use the following commands in-game:

| Command | Description | Permission Node |
|---------|-------------|-----------------|
| `/voicechat` or `/voicechat help` | Shows the localized command summary. | command permission only |
| `/voicechat status` | Shows protocol compatibility, UDP authentication/connection, group, disabled state, and speak/listen permissions without exposing secrets or addresses. **No upstream equivalent** — upstream has no `status` subcommand. | command permission only |
| `/voicechat join <group_name or UUID> [password]` | Joins a group; quote names containing spaces. Accepts a UUID (as sent by invitations) or an exact name; ambiguous names are rejected. | `pumpkin_voice:groups` |
| `/voicechat leave` | Leaves your active voice group. Like upstream, it does **not** require the group permission, so a permission change cannot trap a player in a group. Unlike upstream, it still works when `enable_groups=false`, so an existing membership can always be cleaned up. | command permission only |
| `/voicechat invite <target>` | Sends a localized, clickable join command, with a manual text fallback. | `pumpkin_voice:groups` |

Upstream registers the same five subcommands (`help`, `test`, `invite`, `join`, `leave`) but ships **no** `status`; this build adds it. Upstream's `/voicechat test` is not implemented here (see [the comparison](#-not-implemented-server-side)). Upstream additionally aliases nothing — `/vc` is specific to this build.

---

## Architecture

This codebase acts as an extremely rapid buffer bridging Minecraft Plugin Messages (TCP) and the secure stream bounds (UDP/Datagram). 

### Directory Structure

```text
build.rs               # Scans lang/*.json and generates the embedded catalog list
lang/                  # Translation catalogs (en_us, zh_cn) — the source of truth
src/
├── commands/          # Brigadier command interfaces (/voicechat branch)
├── config/            # TOML layout, strict validation, and initial injection maps
├── handlers/          # Event interceptors (Player Join/Leave, GUI Custom Payloads, Visibility)
├── net/               # Networking logic
│   ├── udp/           # UDP socket, cryptography, and packet handling
│   ├── custom_payloads.rs # TCP Custom payload definitions
│   ├── sync.rs        # Registry/state synchronization order for clients
│   └── voice_packets.rs   # Audio specific byte arrays mimicking `FriendlyByteBuf`
├── state/             # Shared asynchronous connection cache logic (Groups, Players)
├── util/              # Byte buffer extensions, bounded payload reader, permission notices
├── i18n.rs            # JSON registry lookup, fallback chain, and placeholder interpolation
└── lib.rs             # Plugin Entrypoint. Registers macro hooks and routes exports
```

### Request Lifecycle

1. **Player Connection and Compatibility Check**:
    - Trigger: `PlayerJoinEvent`, followed by the client's `voicechat:request_secret` payload.
    - Action: Join creates the upstream-compatible default disconnected state. After compatibility version `20` is confirmed, the server sends the player-state, category, and group registries in that order, followed by `SecretPacket`. These packets are intentionally not sent before the client registers its plugin channels. This order matches the official **Bukkit/Paper plugin** (`states → categories → groups`); the single-player/LAN `common` mod path orders these `states → groups → categories` instead.
2. **UDP Handshake Authentication**:
    - Trigger: Client triggers a `AuthenticatePacket` to `udp_server.rs:24454`.
    - Action: Server validates the outer and inner player UUID, the expected `Secret`, and the UDP source address. `ConnectionCheck` then promotes the pending socket to a connected voice state and broadcasts that state.
3. **Continuous Audio Delivery**:
    - Trigger: Player pushes to talk. Client issues `MicPacket` encoded datagrams.
    - Action: `udp_server.rs` assesses constraints (distance, group ID). If the checks pass, it routes via `PlayerSoundPacket` or `GroupSoundPacket`. Audio bleeding between different worlds is prevented by comparing Pumpkin world IDs.
4. **Heartbeat and Reconnection**:
    - Trigger: Periodic `KeepAlive` packets and the client's `KeepAlive` response.
    - Action: Responses refresh the authenticated connection timestamp. After `10 × keep_alive` without a response, the state is marked disconnected, a fresh secret is generated, registries are resynchronized, and the client is asked to authenticate again.

### Deep Permission Integration

The plugin registers native permission nodes via `pumpkin_plugin_api::permission::Permission`. Adjust these directly inside your primary Pumpkin engine deployment!

- `pumpkin_voice:command.voicechat`: Required to view the commands layout inside chat.
- `pumpkin_voice:speak`: Prevents sending encrypted UDP `MicPackets` outbound.
- `pumpkin_voice:listen`: Prevents receiving encrypted `PlayerSoundPackets` inside loops.
- `pumpkin_voice:groups`: Enables UI access to channels.

---

### ✅ Implemented (server-side parity)

| Upstream feature | Status here |
| ---------------- | ----------- |
| Dedicated UDP voice port with per-player AES-128-GCM secrets | ✅ |
| `SecretPacket` handshake (port, codec, MTU, distance, keep-alive, groups flag, voice host, recording flag) | ✅ |
| Compatibility-version gate plus upstream Bukkit `states → categories → groups → secret` order | ✅ exact field and order parity verified against `SniffedSecretPacket` |
| UDP `Authenticate` / `AuthenticateAck` with secret verification | ✅ |
| Proximity audio with `max_voice_distance`, `whisper_distance`, `broadcast_range`, same-dimension filter | ✅ |
| Groups: create / join / leave via GUI (`set_group`, `create_group`, `leave_group`) with password protection | ✅ |
| Group & player-state synchronization (`add_group`, `remove_group`, `joined_group`, `state`, `states`, `update_state`) | ✅ |
| Volume categories from configuration (`add_category`) | ✅ (no category icons; upstream accepts a 16×16 RGBA icon array) |
| Keep-alive heartbeat, timeout detection, disconnected-state broadcast, and fresh-secret reconnect | ✅ |
| `force_voice_chat` + `login_timeout` kick for unmodded clients | ✅ |
| `allow_pings` Simple Voice Chat discovery-ping response | ✅ |
| `ConnectionCheck` / `ConnectionCheckAck` | ✅ |
| `spectator_interaction` with positional `LocationSoundPacket` audio | ✅ |
| `spectator_player_possession` | ✅ sends private audio only to the spectated player; runtime client validation pending |
| Group-type routing (`NORMAL` / `OPEN` / `ISOLATED`) | ✅ matches upstream `processMicPacket`: only `OPEN` groups also broadcast by proximity |
| Group input validation matching upstream `GROUP_REGEX` (`^[^\p{C}\s][^\p{C}]{0,23}$`, plus UTF-16 wire limits) | ✅ |
| Bounds-checked TCP/UDP packet decoding and authenticated-source enforcement | ✅ |
| Player quit state removal via `voicechat:remove_state` | ✅ (upstream sends `remove_state` only on quit; the disconnected `state` packet is reserved for online-but-disconnected players) |
| Pumpkin `hide_player` / `show_player` visibility synchronization and filtered initial state | ✅ mirrors upstream `PlayerStateManager.onPlayerHide` / `onPlayerShow` |
| `/voicechat join` by UUID or quoted name, with Pumpkin server-side suggestions | ✅ upstream `GroupNameSuggestionProvider` only quotes names containing spaces; this build also sorts, deduplicates, and escapes quotes/backslashes |
| `allow_recording`, `codec`, `mtu_size`, `voice_host` passthrough to clients | ✅ |
| Permission nodes (`listen` / `speak` / `groups`) enforced on the audio path | ✅ (renamed `pumpkin_voice:*`; upstream is `voicechat:*`) |
| Permission-denial on-screen feedback with per-permission cooldown | ✅ action-bar notice; **this build uses 10 s for speak and 30 s for listen** — upstream uses 30 s for both |
| Disabled/disconnected receiver filtering | ✅ |
| Offline-mode encryption identity warning via Pumpkin server API | ✅ |
| Localized player-facing messages, descriptions, and console logs | ✅ plugin-owned JSON registry — `en_us` + `zh_cn` built in, data-folder additions/overrides (see [Translations](#translations)) |
| `/voicechat status` diagnostic command | ➕ beyond upstream (upstream has no `status` subcommand) |
| Bedrock clients (kicked under `force_voice_chat`, skipped for Java payloads) | ➕ beyond upstream (Bukkit plugin only handles Java players) |
| UDP packet rate limit (`max_packets_per_second`) | ➕ beyond upstream: the official UDP path applies **no** per-player limit; only TCP plugin messages are limited |

### ⚠️ Behavior differences worth knowing

| Area | Upstream behavior | Current behavior |
| ---- | ------------------ | ----------------- |
| TCP packet rate limit | `tcp_rate_limit` (default `16`) **kicks** the player: `Kicked for exceeding packet rate limit`; the budget refills over a 5-second window | Silently drops the offending plugin message and keeps the connection; the budget refills over a 1-second window |
| Speak-denial cooldown | Hard-coded 30 s (`Server.java:361`) | 10 s here, alongside a 30 s listen cooldown |
| Group name suggestions | `GroupNameSuggestionProvider` only wraps names containing a space in quotes | Also filters hidden groups, sorts and deduplicates case-insensitively, and escapes embedded quotes/backslashes |
| Empty-group cleanup | Runs after every leave/quit | Same, and the flag is honored, but nothing here can mark a group persistent |
| Persistent groups | Created through the addon API (`createGroup(name, password, persistent)`); survive becoming empty | The flag is honored during empty-group cleanup, but no command or API here creates a persistent group |
| Hidden groups | Marked so clients omit them from public listings; creatable through the addon API | State and synchronization preserve the hidden flag, and suggestions filter them out, but nothing here creates a hidden group |
| `mtu_size` default | `1275` (`AudioUtils.MAX_OPUS_PAYLOAD_SIZE`) | `1024` |
| Category icons | `VolumeCategory.getIcon()` accepts a 16×16 RGBA array, serialized when non-null | No icon is ever sent (the presence byte is always `0`) |
| Category definitions | Registered at runtime through the addon API and unregisterable | Loaded from `config.toml` at startup only; no runtime registration path |
| Compatibility rejection | Rejects with a version-specific message (`≤6` gets a different string) | Single generic message naming the compatible release |
| Admin ping test | `/voicechat test <target>` with retry/timeout reporting, backed by `PingManager` | Not implemented; inbound `0x07` pongs are consumed and discarded because no server-initiated test exists yet |

### ❌ Not implemented (server-side)

**Groups & audio**
- Persistent/hidden group creation and persistence across server restarts

**Commands & permissions**
- `/voicechat test <target>` (admin connection ping test) and the `voicechat.admin` permission node (upstream's only other node besides `listen`/`speak`/`groups`)

**Integrations & extensibility**
- The addon/plugin API (`VoicechatServerApi`): 29 concrete event interfaces (18 of them server-side), audio channels (`Static` / `Locational` / `Entity`), `AudioPlayer`, Opus encoder/decoder, MP3 encoder/decoder, replaceable `VoicechatSocket` implementations, and runtime volume-category registration
- PlaceholderAPI placeholders and ViaVersion compatibility layer
- `use_natives` / `threaded_server_support` config options (not portable to WASM/Pumpkin — intentionally omitted)

---

## Translations

The plugin owns a small JSON translation registry so languages are discovered by file name and are not coupled to a hand-maintained Rust locale enum:

1. `build.rs` scans every `lang/*.json` file and embeds it in the WASM component at build time.
2. On load, the plugin scans `<plugin data folder>/lang/*.json` and merges those files after the embedded catalogs. A data-folder file may define a new locale or override only selected keys.
3. Player-facing messages use `player.get_locale()`. Registration-time descriptions and console logs use the `language` value from `config.toml`.
4. `%s` and indexed `%N$s` placeholders are substituted by the plugin. Missing keys fall back to `en_us`, then to the raw key.

The files currently shipped under `lang/` provide `en_us` (the fallback) and `zh_cn`. That list is intentionally not duplicated in Rust source: the directory is the source of truth.

### Adding or overriding languages at runtime

Server admins can add a language or override any built-in string without recompiling: drop a `lang/<locale>.json` file into the plugin data folder (e.g. `plugins/data/pumpkin_voice/lang/de_de.json`) and reload the plugin. Data-folder files are loaded after the embedded ones, so they win.

```json
{
  "command.join.joined": "Gruppe %s beigetreten",
  "kick.voice_chat_required": "Du musst den Simple Voice Chat Mod installiert haben!"
}
```

To ship a new language **built in**, add `lang/<locale>.json` at the crate root and rebuild. No Rust source change or locale registration is needed.

`lang/en_us.json` is the key reference. It includes player messages, plugin/command/permission descriptions, default category labels, errors, and console log templates. Tests reject embedded catalogs with invalid JSON or a key set that differs from `en_us`.

Pumpkin requests plugin metadata before it provides the plugin data-folder path or loads `config.toml`, so the metadata description uses the embedded `en_us` fallback. Command and permission descriptions are registered later and therefore use the configured server language.

---

## Velocity / BungeeCord / Waterfall

PumpkinVoiceX is compatible with the official Simple Voice Chat proxy plugins. The proxy plugin owns the public UDP socket, observes the `voicechat:request_secret` and `voicechat:secret` plugin messages, replaces the backend port/host advertised to the client, and creates one UDP bridge per player to this Pumpkin backend.

> **Module availability caveat.** Upstream keeps the proxy code in `velocity/`, `bungeecord/`, and `common-proxy/`. Those trees exist in the `26.2` commit this plugin was verified against (`c482f1a5`) and in `26.3`, but upstream commit `d2d1293` ("Remove bukkit and proxy submodules") removed them from the `26.2` branch tip. Take an upstream revision that still contains `common-proxy/src/main/java/de/maxhenkel/voicechat/sniffer/SniffedSecretPacket.java` when you build or verify a proxy, and use a release that still ships the proxy artifact.

1. Install the official Simple Voice Chat **Velocity** or **BungeeCord/Waterfall** plugin on the proxy. Use the same `26.2` release family as the client and this backend protocol.
2. Keep PumpkinVoiceX installed on every Pumpkin backend that should provide voice chat.
3. The proxy must forward the normal `voicechat:*` plugin messages between client and backend. Do not install a second public UDP bridge on the backend.
4. Expose the proxy's configured voice UDP port publicly. Backend UDP ports only need to be reachable from the proxy host.
5. Configure the proxy plugin's `voice_host` when the public voice hostname differs from the Minecraft hostname. The official proxy rewrites PumpkinVoiceX's compatible `SecretPacket` before it reaches the client.

The backend accepts the proxy bridge's UDP source address during the normal authenticated handshake. Its response packets return to that same per-player bridge socket, so no client IP preservation is required for UDP. A regression test locks the `SecretPacket` field order read by the official proxy's `SniffedSecretPacket` parser: secret (16 bytes), port (`i32`), player UUID, codec (`u8`), MTU (`i32`), distance (`f64`), keep-alive (`i32`), groups flag, `voice_host` (UTF), recording flag. The proxy also rejects compatibility versions below `10` and exactly `19`, and `patch()` rewrites only the port and voice host.

**Verification status:** the byte layout and a local UDP relay are covered by tests, but no real Velocity/BungeeCord process, backend switch, or public-NAT deployment has been validated. Treat proxy support as protocolled, not field-proven.

### Visibility / vanish synchronization

PumpkinVoiceX listens for Pumpkin's `PlayerHideEntityEvent` and `PlayerShowEntityEvent` and mirrors Simple Voice Chat's Bukkit behavior:

- Initial `voicechat:states` contains only players visible to the receiving player according to `Player::can_see`.
- Normal state broadcasts are sent only to receivers that can see the state owner.
- Hiding a player sends `voicechat:remove_state` only to that observer.
- Showing a player sends the current `voicechat:state` only to that observer.
- Cancelled visibility events do not alter voice state.
- The pinned Pumpkin host does not emit these events from `hide_player`/`show_player`. A 20-tick reconciliation pass checks `can_see` and sends changed states only. Visibility therefore converges within about one second at 20 TPS. This requires O(players²) visibility checks per pass.

This synchronizes voice HUD/player-state visibility. As in the upstream server, it does not automatically mute audio solely because a player is vanished; voice delivery continues to follow group, world, distance, connection and permission rules.

---

## Environment Variables / Configuration

Here is a breakdown of the standard `config.toml` structure dynamically dropped upon deployment:

| Variable | Description | Default |
| -------- | ----------- | ------- |
| `language` | Server-side language for command/permission descriptions and console logs. Player-facing messages instead follow each player's own locale. | `en_us` |
| `port` | UDP bind port. `-1` currently falls back to 24454 because this SDK exposes no game-port getter. | `24454` |
| `bind_address` | String address the UDP socket clamps to. | `""` (0.0.0.0) |
| `max_voice_distance` | Range cap for dimensional fading audios. | `48.0` |
| `whisper_distance` | Range cap specifically for whispering clients. | `24.0` |
| `codec` | Opus codec compression parameter strings (`VOIP`, `AUDIO`, `RESTRICTED_LOWDELAY`). | `VOIP` |
| `mtu_size` | Maximum audio packet size forwarded to clients. Upstream defaults to `1275`. | `1024` |
| `keep_alive` | Millisecond trigger interval looping connection verifications. Minimum accepted value is `1000`. | `1000` |
| `enable_groups` | Allow or reject GUI `voicechat:create_group` payloads. `/voicechat leave` deliberately ignores it so memberships stay clearable. | `true` |
| `voice_host` | Hostname clients should use to reach the voice server. | `""` (game host) |
| `allow_recording` | Whether clients may record voice chat audio. | `true` |
| `spectator_interaction` | Use positional LocationSound packets for spectators; disabling this does not mute them. | `false` |
| `spectator_player_possession` | Spectator proximity audio goes only to the player being spectated. | `false` |
| `force_voice_chat` | If `true`, non-modded clients are immediately dropped using a kick constraint. | `false` |
| `login_timeout` | Grace period before `force_voice_chat` kicks unmodded clients (ms). Minimum accepted value is `100`. | `10000` |
| `max_packets_per_second` | Maximum UDP packets allowed per player per second before throttling. **Not an upstream option** — the official UDP path has no rate limit. Non-positive values disable it. | `500` |
| `tcp_rate_limit` | Maximum voice-chat plugin messages accepted per player per second. Upstream **kicks** on excess; this build drops the message instead. Non-positive values disable the limit. | `16` |
| `allow_pings` | Whether to respond to UDP ping packets from clients. | `true` |
| `broadcast_range` | Maximum range for audio broadcast. A negative value uses `max_voice_distance + 1`, matching upstream `getBroadcastRange`. | `-1.0` |
| `categories` | Array of volume categories; see below. Optional `description`. | one `radio` entry |

Every field above is required except each category's `description`. Unknown or obsolete keys are rejected rather than ignored.

### Categories Configuration

You can define custom categories in the `config.toml`:

```toml
[[categories]]
id = "radio"
name = "Radio Team"
description = "Global broadcast"
```

`id` must match upstream's `^[a-z_]{1,16}$`; the name is limited to 16 UTF-16 code units and the description to 32767, exactly as upstream serializes them. Duplicate IDs are rejected. Upstream registers categories at runtime through the addon API and can attach a 16×16 RGBA icon per category; this plugin loads them from `config.toml` at startup and never sends an icon.

---

## Troubleshooting

### Connection Timeouts / GUI Shows Unplugged
**Error:** Connecting prints "Voice Chat not found!" or times out aggressively.
**Solution:** 
1. Determine if the UDP port `24454` is exposed in your cloud firewall (e.g., UFW/AWS/OCI panels). UDP acts alongside TCP constraints but requires dedicated protocol openings.
2. Check the server console for `Voice chat UDP server listening on ...` — a UDP bind failure now aborts plugin loading with a localized error.
3. Check for `Rate limiting player ...` warnings in the server console; if seen, increase `max_packets_per_second` in `config.toml`.

### Invite Links That Fail to Join
**Error:** An invitation link yields "group does not exist" even though the group is visible in the UI.
**Solution:** Invitations carry the group **UUID**, and `/voicechat join` resolves either a UUID or an exact name. If you type a name by hand and the server has two groups sharing it, the command refuses with an ambiguity message — join by UUID or rename one group. Older builds that looked groups up by name only could not follow an invitation at all; that is fixed, so make sure the deployed WASM is current.

### Group Join Discarding
**Error:** User selects a correct password but receives "Invalid Password."
**Solution:** Verify the password is entered exactly; it is validated server-side against the stored group password. If the GUI misbehaves, `/voicechat join <name or UUID> <password>` bypasses the UI path.

### Config Write Errors (WASI)
**Error:** `Failed to create config folder ... (os error 44)` or `Operation not permitted`.
**Solution:** This typically indicates a permission or preopen mismatch in the WASI environment. Ensure the plugin metadata requests `fs.read.data` and `fs.write.data` (default in recent versions). The plugin now uses absolute-style relative paths to ensure compatibility with Pumpkin's virtual filesystem.
