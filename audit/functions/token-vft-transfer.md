## `Vft::transfer` in programs/demo-token/app/src/lib.rs (L196-L202)

**Purpose:** Exported VFT `Transfer(to, value) -> bool throws TokenError` (IDL L27). The vault's `push_underlying` (vault lib.rs L725-L732) uses it to pay out redemptions, unbond claims and fee/admin withdrawals (vault L772, L810, L907), with the vault program as `from`.

**Inputs & Assumptions:**
- `to` (ActorId), `value` (U256): message payload. Trust: untrusted.
- Implicit: `from = Syscall::message_source()` (L198). When the vault calls, `from` is the vault program id. `paused`, `balances`.
- Preconditions: none.

**Outputs & Effects:**
- Success: balances moved (L199 -> L101-L102), `Transfer { from, to, value }` emitted (L200), reply `true` (L201). `false` is never produced.
- Error: `Paused`, `ZeroAddress`, `ZeroAmount`, `InsufficientBalance` (L98-L101) become a structured panic via `unwrap_result` (L196; exposure.rs L97-L104; macros.rs L307-L325). Nothing was written on any of these paths (see token-state-transfer.md).

**Block-by-Block:**

```rust
// L198-L201
let from = Syscall::message_source();
self.state.borrow_mut().transfer(from, to, value)?;
self.emit_event(VftEvent::Transfer { from, to, value }).expect("event");
Ok(true)
```
- **What:** identity, move, event, reply.
- **Why here:** temporary borrow dropped at end of L199 before the emit.
- **Assumes:** `message_source()` identifies the payer — true for both user messages and program-to-program messages.
- **Establishes:** balance move plus matching event.
- **Depended on by:** the vault's `push_underlying` match (vault L727-L731): `Ok(Ok(true))` is success; `Ok(Ok(false))` is unreachable from this code; `Ok(Err(e))` is any `TokenError` (decoded from the panic payload, client/mod.rs L456-L461); `Err(e)` is a transport failure (out of gas within `TOKEN_CALL_GAS`, vault L46; program unavailable; undecodable reply).

**What the vault relies on:**
- Return value: exactly `true` on success (L201). The vault treats `false` as rejection (vault L728) but the token never returns it.
- Atomicity: the token handler is synchronous with no `.await`, no cross-program call other than the event send, so there is no interleaving inside a single `transfer`.
- Pause: a paused token yields `Ok(Err(Paused))` at the vault, which the vault maps to `TokenRejected` (vault L729). In `redeem`, the vault has already burned shares before the call and reverts them on error (vault L771, L779-L783). The vault does not query `is_paused` before calling (no call site in vault lib.rs; the client method exists at token_client.rs L271).
- Events: the vault does not consume the `Transfer` event.
- Balance: the token checks the vault's balance (L101); the vault does not read it back before or after.

**Cross-Function Dependencies:**
- Callee `TokenState::transfer` (internal, L97-L103).
- Callee `emit_event` (external-source-available, events.rs L92-L101).
- Callers: any actor; vault `push_underlying` vault L726.
- Shared state: `balances`, `paused`.
- Invariant couplings: none beyond token-state-transfer.md.

**Open Questions:**
- Shared atomicity question, see token-vft-transfer-from.md.
