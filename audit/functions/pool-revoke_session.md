## `PoolState::revoke_session` in programs/vara-pool/app/src/lib.rs (L505-L510)

**Purpose:** Drop the caller's pending proposal and active session, unbinding the key. The only way (besides replacement via `accept_session` L499) an expired or unwanted key leaves `session_keys`.

**Inputs & Assumptions:**
- `owner` (ActorId): the sender (L883-L884). Trust: trusted identity. A key cannot revoke a session it is bound into; only the owner can.
- Implicit state read: `pending_sessions`, `sessions`.

**Outputs & Effects:**
- Removes `pending_sessions[owner]` (L506); if an active session existed, removes `sessions[owner]` and `session_keys[old.key]` (L507-L508).
- Returns `true` if either existed (L507 returns `pending` when there was no active session; L509 returns `true` otherwise).

**Block-by-Block:**

```rust
// L506-L509
let pending = self.pending_sessions.remove(&owner).is_some();
let Some(old) = self.sessions.remove(&owner) else { return pending };
self.session_keys.remove(&old.key);
true
```
- **What:** Remove pending first, then active plus its key mapping.
- **Assumes:** `session_keys[old.key] == owner` (two-map consistency established by `accept_session` L500-L501). If some other owner had since bound the same key, that binding would be removed here instead; `accept_session` L497 and `propose_session` L486 prevent a key bound to `owner` from being bound to anyone else while the mapping exists, so this cannot happen.
- **Establishes:** `old.key` becomes a plain sender again (`actor_for` L475).

**Cross-Function Dependencies:**
- No callees.
- Callers: `Pool::revoke_session` L884; emits `SessionRevoked` only when the return is `true` (L885).
- Shared state: the three session maps.

**Open Questions:**
- none.
