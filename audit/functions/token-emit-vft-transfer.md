## `emit_vft_transfer` in programs/demo-token/app/src/lib.rs (L247-L252)

**Purpose:** Lets the `Admin` and `Faucet` services emit a VFT-standard `Transfer` event under the `Vft` service's route, so mints and burns look like `Transfer` from/to the zero address to wallets and indexers (module docs L7-L10, L20-L22).

**Inputs & Assumptions:**
- `state` (&RefCell<TokenState>): passed through to a fresh `Vft` (L250); not borrowed. Trust: trusted.
- `from`, `to`, `value`: copied into the event (L251). Trust: whatever the caller established.
- Implicit: `VFT_ROUTE_IDX == 1` (L22).
- Preconditions: no outstanding `RefMut` on `state` is required, because `Vft::new` only stores the reference (L181-L183) and `emit_event` does not touch state. All three callers have already dropped their borrow block (L294, L308, L443).

**Outputs & Effects:**
- Sends one event message. On wasm32 `emit_event` builds a header from the `Vft` interface id, the event's entry id and `route_idx`, then `gstd::msg::send_bytes(ActorId::zero(), payload, 0)` (sails-rs-1.0.1/src/gstd/events.rs L92-L101). On failure `.expect("event")` panics (L251), which aborts the whole handler.

**Block-by-Block:**

```rust
// L250-L251
let vft = Vft::new(state).expose(VFT_ROUTE_IDX);
vft.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
```
- **What:** builds an exposure with a hard-coded route index and emits.
- **Why here:** the `Admin`/`Faucet` exposures carry their own route index (2, 3); an event emitted through them would decode as an `AdminEvent`/`FaucetEvent`, not a VFT `Transfer`.
- **Assumes:** the `vft` service is the program's first service. Established by: `Program` declares `vft` first (L487-L489, comment L486); the sails program macro assigns service `entry_id` in declaration order (sails-macros-core-1.0.1/src/program/mod.rs L166-L181) and `route_idx = entry_id + 1` (L695-L700); the generated vault client agrees (`ROUTE_ID_VFT = 1`, vault token_client.rs L7). Nothing at compile time or runtime ties `VFT_ROUTE_IDX` to the actual position.
- **Establishes:** an event that decodes as `VftEvents::Transfer` on clients (tests/gtest.rs L63-L64 checks this for a faucet claim).
- **Depended on by:** indexers and wallets; not by the vault (vault lib.rs never listens to token events — no `listen` call in the grep of L714-L732 and surrounding code).

**Cross-Function Dependencies:**
- Callee `Vft::new` (internal, L181-L183): no failure path.
- Callee `expose` (macro-generated, source in sails-macros-core): sets the route index; no failure path observed.
- Callee `emit_event` (external-source-available, events.rs L92-L101): fails only if `send_bytes` fails (gas/mailbox). The `expect` at L251 converts that into a panic.
- Callers: `Admin::mint` L295, `Admin::burn` L309, `Faucet::claim` L444 — each after its state write and before its own service event.
- Shared state: none read or written.
- Invariant couplings: because a failed emit panics, an event-send failure discards the preceding state write together with the handler (Gear runs a message atomically; see open question in token-vft-transfer-from.md). The state write and the event therefore land together or not at all.

**Open Questions:**
- unclear; need to inspect the macro-generated `expose` for `Vft` to confirm it stores exactly the passed `route_idx` and the `Vft` interface id (only the test-only `DummyExposure` at sails-rs-1.0.1/src/gstd/macros.rs L427-L447 was read).
