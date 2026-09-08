## `PoolState::actor_for` in programs/vara-pool/app/src/lib.rs (L474-L480)

**Purpose:** Resolve who a message acts for: the sender itself, or, if the sender is an accepted session key, the owner who registered it. Every user-facing `Pool` command routes its identity through this (L687, L705, L729, L743); `Vft` commands do not (L564, L572, L579), nor do `fund_rewards`, `create_session`, `accept_session`, `revoke_session`.

**Inputs & Assumptions:**
- `sender` (ActorId): `Syscall::message_source()` at the call sites. Trust: the runtime's sender, trusted as identity.
- `action` (SessionAction): the calling handler's action; trusted constant.
- `now_ms` (u64): clock.
- Implicit state read: `session_keys`, `sessions`.
- Preconditions:
  - `session_keys[key] == owner` implies `sessions[owner].key == key`: established by `accept_session` writing both (L500-L501) after removing the previous binding (L499), and `revoke_session` removing both (L507-L508). No other writers. L476 handles the inconsistent case defensively as `SessionExpired`.

**Outputs & Effects:**
- `Ok(sender)` when the sender holds no session key (L475); `Ok(owner)` when a live, permitted session exists (L479); `Err(SessionExpired)` / `Err(SessionNotAllowed)` otherwise (L476-L478). No writes.

**Block-by-Block:**

```rust
// L475
let Some(owner) = self.session_keys.get(&sender) else { return Ok(sender) };
```
- **What:** Plain senders act for themselves.
- **Establishes:** a key address that is *proposed but not accepted* is a plain sender (it is not in `session_keys` until `accept_session` L501; test L1195-L1197).

```rust
// L476-L479
let session = self.sessions.get(owner).ok_or(PoolError::SessionExpired)?;
if now_ms >= session.expires_at { return Err(PoolError::SessionExpired); }
if !session.actions.contains(&action) { return Err(PoolError::SessionNotAllowed); }
Ok(*owner)
```
- **What:** Once a sender is a bound key, it is *only* the owner's agent: there is no fallback to `Ok(sender)`.
- **Establishes:** while bound (even after expiry, until the owner revokes or replaces the session), the key address cannot stake, unstake, unbond or claim for itself; an expired binding returns `SessionExpired` for every action. Only the owner can clear it (`revoke_session` uses `message_source` as owner, L882-L884; `accept_session` of a new key removes the old mapping, L499). For `stake`, the VARA attached by the key is credited to the owner (L687) and an `Err` refunds it to the key (L695).
- **Depended on by:** all four routed handlers.

**Cross-Function Dependencies:**
- No callees.
- Callers: `Pool::stake` L687, `Pool::unstake` L705, `Pool::request_unbond` L729, `Pool::claim` L743.
- Shared state: `session_keys` and `sessions`, written by `accept_session` and `revoke_session` only.
- Invariant couplings: payouts in `unstake`/`claim` go to the resolved `owner` (L710, L746), never to the key (comment L80).

**Open Questions:**
- unclear; whether `session_keys` entries for expired sessions are meant to persist until the owner acts. Nothing garbage-collects them, and `session_owner` (L902-L906) hides expired ones from view while `actor_for` still treats them as bindings.
