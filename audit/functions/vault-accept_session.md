## `VaultState::accept_session` in programs/vault/app/src/lib.rs (L536-L545)

**Purpose:** The key accepts the session `owner` proposed for it; from then on the key's messages act for `owner` (via `actor_for`). Replaces any active session the owner had, freeing the old key (comment L534-L535, test L1256-L1260). Called by `Vault::accept_session` L936 with `key = message_source()`.

**Inputs & Assumptions:**
- `key` (ActorId): the sender. Trust: trusted identity — this is the whole point of the accept step: only the account that controls `key` can bind `key` (regression test L359-L371).
- `owner` (ActorId): payload. Trust: untrusted; validated by the lookup at L537.
- `now_ms` (u64): trusted.
- Implicit state read/write: `pending_sessions`, `sessions`, `session_keys`.

**Outputs & Effects:**
- Reads then writes, all after the last `Err`: `pending_sessions.remove(owner)` L540; `sessions.remove(owner)` and `session_keys.remove(old.key)` if there was one L541; `sessions[owner] = session` L542; `session_keys[key] = owner` L543.
- Returns the `Session`, or `BadSession` (L537: no proposal for `owner`, or its key is not the sender; L539: the sender is already bound to another owner; L540: unreachable after L537), or `SessionExpired` (L538).

**Block-by-Block:**

```rust
// L537-L538
let proposed = self.pending_sessions.get(&owner).filter(|s| s.key == key).ok_or(VaultError::BadSession)?;
if now_ms >= proposed.expires_at { return Err(VaultError::SessionExpired); }
```
- **What:** There must be a proposal by `owner` naming exactly the sender, and it must not have expired.
- **Why here:** Binds the sender's identity to the proposal before any write.
- **Establishes:** `proposed.key == key`, live.
- **Depended on by:** L540-L543.

```rust
// L539
if let Some(other) = self.session_keys.get(&key) { if *other != owner { return Err(VaultError::BadSession); } }
```
- **What:** The sender may not already be an accepted key for a different owner.
- **Why here:** Re-checks L528 at acceptance time, since another owner's proposal for this key may have been accepted in between.
- **Establishes:** after L543, `session_keys[key]` maps to exactly one owner.
- **Depended on by:** `actor_for` L517 (unambiguous lookup).

```rust
// L540-L544
let session = self.pending_sessions.remove(&owner).ok_or(VaultError::BadSession)?;
if let Some(old) = self.sessions.remove(&owner) { self.session_keys.remove(&old.key); }
self.sessions.insert(owner, session.clone());
self.session_keys.insert(key, owner);
Ok(session)
```
- **What:** Consume the proposal, drop the owner's previous session and its key, install the new one.
- **Why here:** Removing `old.key` before inserting `key` keeps `session_keys` and `sessions` consistent even when `old.key == key` (a same-key renewal: removed then re-inserted).
- **Assumes:** `old.key` is not also some *other* owner's key — guaranteed by the one-owner-per-key rule (L528/L539), so removing it cannot orphan another owner's session.
- **Establishes:** `sessions[owner].key == key` and `session_keys[key] == owner` — the precondition `actor_for` relies on.
- **Depended on by:** `actor_for`; `SessionAccepted` event L937.

**Cross-Function Dependencies:**
- Callees: none.
- Callers: `Vault::accept_session` L936 (sender is the key).
- Shared state: `pending_sessions`, `sessions`, `session_keys` with `propose_session`, `revoke_session`, `actor_for`.
- Invariant couplings: acceptance is synchronous, so it can land between the two segments of any async handler; those handlers captured `owner` pre-await and are unaffected. From acceptance on, the key's *own* deposits/redeems are redirected to `owner` (L517-L521) for the permitted actions and refused for the others.

**Open Questions:**
- none.
