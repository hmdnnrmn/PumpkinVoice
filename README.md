# Simple Voice Chat for PumpkinMC

This plugin implements the backend compatibility needed to host the [Simple Voice Chat](https://modrinth.com/plugin/simple-voice-chat) mod on a [PumpkinMC](https://github.com/Pumpkin-MC/Pumpkin) server. It allows players connecting with modern Minecraft clients (Fabric, Forge, NeoForge) to use proximity voice chat and dynamically created voice groups.

## Key Features

- **Proximity Chat**: Accurately simulates dimensional audio using 3D vector coordinates sent directly to your game client.
- **Group Channels**: Full support for the GUI group interfaces (creating groups, joining password-locked groups, leaving groups, managing volume).
- **Dynamic Audio Categories**: Create custom audio categories via configuration to differentiate audio streams (e.g. Radio, Global Broadcast) with custom names and icons.
- **Packet Rate Limiting**: Built-in protection against network flooding/DoS using a high-performance token-bucket rate limiter.
- **Permissions Support**: Fully respects the native PumpkinMC permission node trees.
- **Optimized Transport**: Connects entirely over UDP with lightweight `AES-128-GCM` encryption for optimal performance.

---

## Tech Stack

- **Language**: Rust
- **Framework**: `pumpkin-api` (PumpkinMC Plugin SDK)
- **Async Runtime**: `tokio` (Powers the real-time UDP connection network)
- **Cryptography**: `aes-gcm` suite for packet serialization matching JVM mod signatures
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

We provide pre-built binaries for Windows, Linux, and MacOS under the Releases tab.

1. Download the latest `.dll` (Windows), `.so` (Linux), or `.dylib` (MacOS) from the Releases page.
2. Place the downloaded library file directly into your PumpkinMC server's `plugins/` directory.

### Build from Source (Rust)

If you prefer to compile the plugin yourself or are contributing to development:

1. **Clone the Repository**
   ```bash
   git clone https://github.com/hmdnnrmn/PumpkinVoice.git
   cd PumpkinVoice
   ```

2. **Build the Plugin**
   Compile the plugin utilizing the optimal release flag for performance:
   ```bash
   cargo build --release
   ```

3. **Deploy the Executable**
   Once compiled, move the output executable library into your server's plugin pool:
   ```bash
   # Windows
   copy target\release\pumpkin_voice.dll \path\to\pumpkin\plugins\

   # Linux
   cp target/release/libpumpkin_voice.so /path/to/pumpkin/plugins/

   # MacOS
   cp target/release/libpumpkin_voice.dylib /path/to/pumpkin/plugins/
   ```

### Adjust Server Configurations & Connect

The first time you boot the server, the plugin will construct a default configuration file at `plugins/pumpkin_voice/config.toml`.
By default, the plugin will span out a UDP listener concurrently running on port `24454`.

Connect via your Minecraft client. Look at the bottom left of your screen, you should see no "Unplugged" symbol. Press <kbd>V</kbd> to open up the Simple Voice Chat UI to guarantee that the UI says "Voice Chat Connected".

---

## Commands

PumpkinMC directly delegates commands to the plugin via the Brigadier argument mapping interface. Use the following commands in-game:

| Command | Description | Permission Node |
|---------|-------------|-----------------|
| `/voicechat join <group_name> <password>` | Looks up a global group and assigns you to it. Supports passwords. | `pumpkin_voice:groups` |
| `/voicechat leave` | Disconnects you from your active group bounds. | `pumpkin_voice:groups` |
| `/voicechat invite <target>` | Sends a chat message to a player with a one-click join link. | `pumpkin_voice:groups` |

---

## Architecture

This codebase acts as an extremely rapid buffer bridging Minecraft Plugin Messages (TCP) and the secure stream bounds (UDP/Datagram). 

### Directory Structure

```text
src/
├── commands/          # Brigadier command interfaces (/voicechat branch)
├── config/            # TOML layout and initial injection maps
├── handlers/          # Event interceptors (Player Join/Leave, GUI Custom Payloads)
├── net/               # Networking logic
│   ├── udp/           # UDP socket, cryptography, and packet handling
│   ├── custom_payloads.rs # TCP Custom payload definitions
│   └── voice_packets.rs   # Audio specific byte arrays mimicking `FriendlyByteBuf`
├── state/             # Shared asynchronous connection cache logic (Groups, Players)
├── util/              # Byte buffer extensions
└── lib.rs             # Plugin Entrypoint. Registers macro hooks and routes exports
```

### Request Lifecycle

1. **Player Connection**:
    - Trigger: `PlayerJoinEvent` inside `lib.rs`.
    - Action: A new AES secret is generated in `state.rs`, embedded via a `SecretPacket`, and pushed directly over custom payloads via TCP.
2. **UDP Handshake Authentication**:
    - Trigger: Client triggers a `AuthenticatePacket` to `udp_server.rs:24454`.
    - Action: Server validates the UDP source against the expected `Secret`. Modifies `socket_addr` properties.
3. **Continuous Audio Delivery**:
    - Trigger: Player pushes to talk. Client issues `MicPacket` encoded datagrams.
    - Action: `udp_server.rs` assesses constraints (distance, group ID). If condition blocks pass, routes via `PlayerSoundPacket` or `GroupSoundPacket` directly. Audio bleeding between different worlds is prevented via strict `Arc::ptr_eq` matching against the active game universe.

### Deep Permission Integration

The plugin registers native permission nodes via `pumpkin_util::permission::Permission`. Adjust these directly inside your primary Pumpkin engine deployment!

- `pumpkin_voice:command.voicechat`: Required to view the commands layout inside chat.
- `pumpkin_voice:speak`: Prevents sending encrypted UDP `MicPackets` outbound.
- `pumpkin_voice:listen`: Prevents receiving encrypted `PlayerSoundPackets` inside loops.
- `pumpkin_voice:groups`: Enables UI access to channels.

---

## Unimplemented Features (Skipped)

As this is an ongoing backend port of the Voice Chat mod to the rapid PumpkinMC framework, the following advanced features are intentionally skipped for the current minimum viable configuration:
- **Spectator Possession & Camera Broadcasting**: Audio coordinates routed from entities a spectator actively possesses are currently unmapped.
- **Server Audio Recording API**: Hooks and custom API interceptors for third-party Pumpkin scripts to record audio natively are unavailable.
- **Plugin API / Developer Events**: Intercepting granular `VoicechatServerApi` events directly like Forge/Fabric is non-existent.
- **Microphone Loopback/Testing UDP Loops**: Emitting audio back to the local client exclusively for mic testing natively is untracked.

---

## Environment Variables / Configuration

Here is a breakdown of the standard `config.toml` structure dynamically dropped upon deployment:

| Variable | Description | Default |
| -------- | ----------- | ------- |
| `port` | The UDP Binding port. `-1` aligns directly to TCP game port. | `24454` |
| `bind_address` | String address the UDP socket clamps to. | `""` (0.0.0.0) |
| `max_voice_distance` | Range cap for dimensional fading audios. | `48.0` |
| `whisper_distance` | Range cap specifically for whispering clients. | `24.0` |
| `codec` | Opus codec compression parameter strings. | `VOIP` |
| `keep_alive` | Millisecond trigger interval looping connection verifications. | `1000` |
| `enable_groups` | Allow or reject GUI `voicechat:create_group` payloads. | `true` |
| `force_voice_chat` | If `true`, non-modded clients are immediately dropped using a kick constraint. | `false` |
| `max_packets_per_second` | Maximum UDP packets allowed per player per second before throttling. | `200` |
| `allow_pings` | Whether to respond to UDP ping packets from clients. | `true` |
| `broadcast_range` | Maximum range for audio broadcast. `-1` uses max voice distance. | `-1.0` |

### Categories Configuration

You can define custom categories in the `config.toml`:

```toml
[[categories]]
id = "radio"
name = "Radio Team"
description = "Global broadcast"
```

---

## Troubleshooting

### Connection Timeouts / GUI Shows Unplugged
**Error:** Connecting prints "Voice Chat not found!" or times out aggressively.
**Solution:** 
1. Determine if the UDP port `24454` is exposed in your cloud firewall (e.g., UFW/AWS/OCI panels). UDP acts alongside TCP constraints but requires dedicated protocol openings.
2. Review the logs to ensure the `tokio` runtime initialized successfully in the background.

### Group Join Discarding
**Error:** User selects a correct password but receives "Invalid Password."
**Solution:** Ensure the client and server code are mirrored correctly. Abandoned GUI parameters occasionally drop payload arrays if the UI bugs out locally. Validate through the standard `/voicechat join` commands as a bypass mechanic. 

