## `PoolState::propose_session` in programs/vara-pool/app/src/lib.rs (L483-L490)

**Purpose:** Record a proposed session `owner -> key`; nothing binds until the key accepts (comment L79-L82). Called by `create_session` with `owner = message_source` (L864-L865).

**Inputs & Assumptions:**
- `owner` (ActorId): the sender. Trust: trusted identity.
- `key` (ActorId), `duration_secs` (u64), `actions` (Vec<SessionAction>): untrusted parameters.
- `now_ms` (u64): clock.
- Implicit state read: `session_keys`.

**Outputs & Effects:**
- Writes `pending_sessions[owner] = session` (L488), replacing any earlier pending proposal from this owner. Returns the session.
- Error `BadSession` (L484, L486) before any write.

**Block-by-Block:**

```rust
// L484
if key.is_zero() || key == owner || duration_secs == 0 || duration_secs > MAX_SESSION_SECS || actions.is_empty() { return Err(PoolError::BadSession); }
```
- **What:** Shape checks: non-zero key, key differs from owner, 1 s to 30 days, at least one action.
- **Establishes:** `key != owner`, so `accept_session` (which does not repeat this check, L494-L503) never binds an address to itself. `actions` may contain duplicates; `contains` at L478 is unaffected.

```rust
// L486
if let Some(other) = self.session_keys.get(&key) { if *other != owner { return Err(PoolError::BadSession); } }
```
- **What:** A key already bound to a different owner cannot be proposed.
- **Assumes:** `session_keys` entries for expired sessions still count as bound (nothing removes them on expiry), so an expired key of owner A cannot be proposed by owner B until A revokes.
- **Establishes:** nothing about `key` being an owner elsewhere: a `key` that is itself an owner with its own session is accepted here; once it accepts, `actor_for` routes its messages to `owner` and it can no longer act on its own position.

```rust
// L487-L489
let session = Session { key, expires_at: now_ms.saturating_add(duration_secs.saturating_mul(1000)), actions };
self.pending_sessions.insert(owner, session.clone());
Ok(session)
```
- **What:** Expiry is fixed at proposal time, not at acceptance.
- **Establishes:** the usable window after acceptance is `duration - (accept_time - propose_time)`; `accept_session` refuses once `expires_at` has passed (L496).

**Cross-Function Dependencies:**
- No callees.
- Callers: `Pool::create_session` L865; emits `SessionProposed` L866.
- Shared state: `pending_sessions` also written by `accept_session` L498 (removes) and `revoke_session` L506 (removes).
- Invariant couplings: one pending proposal per owner (map keyed by owner); a stranger can be proposed as a key without consent, but `actor_for` ignores proposals (L475), and the gtest at L313-L328 asserts a proposed stranger's own stake stays theirs.

**Open Questions:**
- none.
