## `Admin::mint` in programs/demo-token/app/src/lib.rs (L286-L298)

**Purpose:** Exported `Mint(to, value) -> bool throws TokenError` (IDL L86). Privileged supply creation for the admin or any granted minter.

**Inputs & Assumptions:**
- `to` (ActorId), `value` (U256): payload. Trust: untrusted arguments from an authorized caller.
- Implicit: `caller = Syscall::message_source()` (L289); `admin`, `minters`, `paused`, `balances`, `total_supply`.
- Preconditions: caller is admin or minter — established by `ensure_minter` L292.

**Outputs & Effects:**
- Success: `balances[to] += value`, `total_supply += value` (L293 -> L124-L126); VFT `Transfer { 0, to, value }` under route 1 (L295); `Minted { to, value }` under the Admin route (L296); reply `true` (L297).
- Errors, all before any write: `Unauthorized` (L292), `Paused` (L121), `ZeroAddress` (L122), `ZeroAmount` (L123), `Overflow` (L124). Delivered as a structured panic via `unwrap_result` (L287; exposure.rs L97-L104).

**Block-by-Block:**

```rust
// L290-L294
{
    let mut s = self.state.borrow_mut();
    s.ensure_minter(caller)?;
    s.mint(to, value)?;
}
```
- **What:** authorize, then mint, under one borrow.
- **Why here:** authorization precedes the state transition; the borrow ends before the two emits.
- **Assumes:** `minters`/`admin` reflect the intended set (see token-state-ensure-minter.md).
- **Establishes:** supply invariant preserved; caller authorized.
- **Depended on by:** the two events at L295-L296, which report the same `to`/`value`.

```rust
// L295-L297
emit_vft_transfer(self.state, ActorId::zero(), to, value);
self.emit_event(AdminEvent::Minted { to, value }).expect("event");
Ok(true)
```
- **What:** two events, fixed `true`.
- **Why here:** after the write; an emit failure panics and takes the write with it (atomicity assumption, token-vft-transfer-from.md open question).
- **Assumes:** `VFT_ROUTE_IDX` is correct (token-emit-vft-transfer.md).
- **Establishes:** a mint is observable both as a VFT `Transfer` from zero and as `Minted`.
- **Depended on by:** indexers; not the vault.

**Cross-Function Dependencies:**
- Callee `ensure_minter` (internal, L159-L161).
- Callee `TokenState::mint` (internal, L120-L128).
- Callee `emit_vft_transfer` (internal, L247-L252); `emit_event` (external-source-available).
- Callers: admin or minters (tests/gtest.rs L110-L135 covers grant, pause, revoke).
- Shared state: `balances`, `total_supply`, `minters`, `admin`, `paused`.
- Invariant couplings: no cap on `value` or on cumulative minted supply other than `U256` overflow (L124). Paused blocks minting (L121).

**Open Questions:**
- None.
