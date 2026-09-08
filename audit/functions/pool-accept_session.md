## `PoolState::accept_session` in programs/vara-pool/app/src/lib.rs (L494-L503)

**Purpose:** The proposed key (the sender, L874) activates the session `owner` proposed for it, replacing whatever session `owner` had. From here on `actor_for` maps the key to the owner.

**Inputs & Assumptions:**
- `key` (ActorId): the sender. Trust: trusted identity.
- `owner` (ActorId): untrusted parameter naming whose proposal to accept.
- `now_ms` (u64): clock.
- Implicit state read: `pending_sessions`, `session_keys`, `sessions`.
- Preconditions:
  - `pending_sessions[owner].key == key` (L495): the only thing tying the sender to the proposal.
  - `key != owner`: established by `propose_session` L484, not re-checked here.

**Outputs & Effects:**
- Writes: removes `pending_sessions[owner]` (L498); removes the owner's previous `sessions` entry and its `session_keys[old.key]` (L499); inserts `sessions[owner]` and `session_keys[key]` (L500-L501).
- Errors before any write: `BadSession` (L495, L497, L498), `SessionExpired` (L496).
- Postcondition: `session_keys[key] == owner` and `sessions[owner].key == key`; the old key (if different) is unbound and reverts to acting for itself.

**Block-by-Block:**

```rust
// L495-L497
let proposed = self.pending_sessions.get(&owner).filter(|s| s.key == key).ok_or(PoolError::BadSession)?;
if now_ms >= proposed.expires_at { return Err(PoolError::SessionExpired); }
if let Some(other) = self.session_keys.get(&key) { if *other != owner { return Err(PoolError::BadSession); } }
```
- **What:** Proposal exists for this key, is not expired, and the key is not bound elsewhere.
- **Why here:** The L497 re-check covers the window between proposal and acceptance in which the key could have accepted a different owner's proposal (`propose_session` L486 checked only at proposal time).
- **Establishes:** on fall-through, `session_keys.get(&key)` is `None` or `Some(owner)`.

```rust
// L498-L502
let session = self.pending_sessions.remove(&owner).ok_or(PoolError::BadSession)?;
if let Some(old) = self.sessions.remove(&owner) { self.session_keys.remove(&old.key); }
self.sessions.insert(owner, session.clone());
self.session_keys.insert(key, owner);
Ok(session)
```
- **What:** Consume the proposal, unbind the owner's old key, bind the new one.
- **Assumes:** if `old.key == key` (re-accepting the same key), L499 removes and L501 re-inserts the same mapping; consistent.
- **Establishes:** the two-map consistency `actor_for` relies on (L475-L476).
- **Depended on by:** `actor_for`, `session_owner` query L902-L906.

**Cross-Function Dependencies:**
- No callees.
- Callers: `Pool::accept_session` L875; emits `SessionAccepted` L876.
- Shared state: `sessions`/`session_keys` also written by `revoke_session` L507-L508; `pending_sessions` by `propose_session` L488 and `revoke_session` L506.
- Invariant couplings: acceptance is the consent step that stops an owner from binding an address they do not control (comment L81-L82, test L1195-L1200). Nothing here checks that `key` is not itself an owner with an active session (see `propose_session`).

**Open Questions:**
- none.
