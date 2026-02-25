# 007 — On-Chain Programs

Solana on-chain programs for **007**, a commit-reveal tactical shooter built on [BOLT ECS](https://github.com/magicblock-labs/bolt) and [Magicblock Ephemeral Rollups](https://docs.magicblock.gg).

## Overview

Players choose actions simultaneously each turn (Shoot, Protect, Reload, Mirror, or Noop), commit a hash of their action on-chain, then reveal it. The protocol resolves all actions atomically — no server, no trust.

Game state lives in Magicblock Ephemeral Rollups for near-instant finality (~400ms), then settles back to Solana devnet.

---

## Repository Structure

```
.
├── Anchor.toml
├── Cargo.toml
├── programs/
│   └── lobby/               # Entry-fee escrow & match queue
└── programs-ecs/
    ├── components/
    │   ├── match-config/    # Match parameters (durations, caps, modes)
    │   ├── match-state/     # Live state (phase, alive mask, deadlines, transcript)
    │   ├── players/         # Per-seat inventories (bullets, mirrors, locks)
    │   └── turn-data/       # Per-turn commit hashes, reveals, actions, salts
    └── systems/
        ├── create-match/    # Initialise all components, seed, first commit deadline
        ├── commit-action/   # Store SHA-256 commit hash for a seat
        ├── reveal-action/   # Verify hash pre-image, store plaintext action
        ├── resolve-turn/    # Apply all actions, compute deaths, advance turn
        └── end-match/       # Assert match is finished (cleanup hook)
```

---

## Game Mechanics

### Actions (per turn)

| Code | Name    | Effect |
|------|---------|--------|
| 0    | Noop    | Do nothing |
| 1    | Protect | Block incoming shots this turn (locked for N turns after being hit) |
| 2    | Reload  | Gain 1 bullet (up to `bullets_cap`) |
| 3    | Mirror  | Reflect a shot back at the shooter (consumes 1 mirror) |
| 4    | Shoot   | Fire at a target seat (consumes 1 bullet) |

### Turn Flow

```
Commit phase  →  Reveal phase  →  Resolve phase  →  (next turn or end)
```

1. **Commit** — each alive player submits `SHA256(action || target || salt || match_id || turn || pubkey)`
2. **Reveal** — each player submits `(action, target, salt)`, verified against the stored hash
3. **Resolve** — anyone calls `resolve_turn`; actions apply in deterministic order:
   - Missed commits/reveals default to Noop
   - Protect/Mirror flags computed
   - Reloads applied (ascending seat order)
   - Shots resolved; mirror reflections handled
   - Deaths computed: `death_threshold = 2 + SHA256(seed || pubkey)[0] % 5`
   - Turn counter incremented; next commit deadline set

### Win Condition

Last seat(s) standing (`win_mode = 0: LastAlive`). Winner pubkeys written to `match_state.winners`.

---

## Components

All components use `#[component(delegate)]` (delegatable to Ephemeral Rollups).

### `MatchConfig`
```
min_players, max_players
commit_duration_secs, reveal_duration_secs
protect_lock_turns, bullets_cap, mirrors_cap
loot_mode, win_mode
```

### `MatchState`
```
match_id, status (0=Init 1=Running 2=Finished 3=Cancelled)
phase (0=Commit 1=Reveal 2=Resolve), turn
alive_mask (bitset), alive_count
seed [u8;32]
commit_deadline, reveal_deadline
committed_count, revealed_count
winners [Pubkey;4], winner_count
transcript_hash, final_state_hash
```

### `Players`
```
players [Pubkey;4], player_count
bullets [u8;4], mirrors [u8;4]
hits_received [u8;4], protect_lock [u8;4]
```

### `TurnData`
```
commit_hashes [[u8;32];4]
committed (bitset), revealed (bitset)
actions [u8;4], targets [u8;4], salts [[u8;32];4]
```

---

## Deployed Addresses (Devnet)

| Program        | Address |
|----------------|---------|
| `match_config` | `35GEdGYpCaZfafmtQoea5pgz4Y2G2ciYQXjwDm7dCGYv` |
| `match_state`  | `AKnxJJqzHKy99bD2gqM5YdjSHMEbwDYrk69mzd2pb52Y` |
| `players`      | `2mwqV9NyisXsRgKFQC62Ne1JhNLSYKxD2GuD75W7Rgcz` |
| `turn_data`    | `DCwYD9hZjpL3RhnhYo3fLWby6YxahjeQ5Gd9ggsp9Mhi` |
| `create_match` | `EaXW3WKMewu7hzeDCJX4phpQj75uWtNse8aaqyAsguPy` |
| `commit_action`| `EtF5LQ9tAfbBD7ez4GxP3S4AbTRFZupdw6PFLB4qtfL4` |
| `reveal_action`| `3QSqmCakjVv3qGY4oD7hNzTHHE2LQQByU6Yj5iybkmxh` |
| `resolve_turn` | `DjN7EguTF3dhZ3S1ueFZDFNB4VeEXruPRdVJQ9LbMqZ`  |
| `end_match`    | `63aHmRtezLwdybGnSLDYU679CTFUB9JMhQfbst1zA5sb` |
| `lobby`        | `4Uu75QspEnoCzdDp8QWQkMCfbL5aY6xkk14mccziPtdB` |

---

## Build & Deploy

**Prerequisites:** Rust, Solana CLI, Anchor CLI, bolt-lang 0.2.4

```bash
# Build all programs
anchor build

# Deploy a specific program
anchor deploy --provider.cluster devnet --program-name <name>

# If buffer size error on deploy
solana program extend <PROGRAM_ID> 10000 --url devnet

# Clean stale buffers
solana program close --buffers --url devnet
```

> After changing a component struct, all programs that reference it must be redeployed.

---

## Tech Stack

- [BOLT ECS](https://github.com/magicblock-labs/bolt) v0.2.4 — on-chain entity-component-system framework
- [Magicblock Ephemeral Rollups](https://docs.magicblock.gg) — delegated state for low-latency game loops
- Anchor / Solana Program Library
