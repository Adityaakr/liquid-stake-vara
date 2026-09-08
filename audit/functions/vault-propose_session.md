## `VaultState::propose_session` in programs/vault/app/src/lib.rs (L525-L532)

**Purpose:** An owner proposes `key` to act for it for `duration_secs`, limited to `actions`. Nothing takes effect until `key` itself calls `accept_session` (doc L97-L100, regression test L359-L371). Called by `Vault::create_session` L926.

**Inputs & Assumptions:**
- `owner` (ActorId): `message_source()` of `create_session` (L925). Trust: trusted identity.
- `key` (ActorId), `duration_secs` (u64), `actions` (Vec<SessionAction>): payload. Trust: untrusted.
- `now_ms` (u64): trusted.
- Implicit state read: `session_keys`.

**Outputs & Effects:**
- State write: `pending_sessions[owner] = session` (L530), replacing any earlier proposal by this owner.
- Returns the `Session` or `Err(BadSession)` (L526, L528). No other writes; a proposal does not touch `sessions`/`session_keys`.

**Block-by-Block:**

```rust
// L526
if key.is_zero() || key == owner || duration_secs == 0 || duration_secs > MAX_SESSION_SECS || actions.is_empty() { return Err(VaultError::BadSession); }
```
- **What:** Shape checks: non-zero key, not self, 1 s..30 days (L44), at least one action.
- **Why here:** Before writing.
- **Assumes:** duplicate entries in `actions` are harmless (`contains` at L520); nothing dedups or bounds the vector length except message size.
- **Establishes:** `expires_at` at L529 is at most 30 days out and strictly in the future.
- **Depended on by:** L529.

```rust
// L528
if let Some(other) = self.session_keys.get(&key) { if *other != owner { return Err(VaultError::BadSession); } }
```
- **What:** A key already *accepted* for another owner cannot be proposed.
- **Why here:** Preserves "a key serves one owner" (comment L527) at proposal time, for accepted keys only.
- **Assumes:** pending proposals do not need the same exclusivity: two owners may both have a pending proposal naming the same key; whichever the key accepts first wins, and the second `accept_session` is then refused at L539. Established by the code shape; no test covers two pending proposals for one key.
- **Establishes:** if `key` is bound, it is bound to `owner` (re-proposal by the same owner is allowed).
- **Depended on by:** `accept_session` L539 re-checks the same condition at acceptance time.

```rust
// L529-L531
let session = Session { key, expires_at: now_ms.saturating_add(duration_secs.saturating_mul(1000)), actions };
self.pending_sessions.insert(owner, session.clone());
Ok(session)
```
- **What:** Record the proposal.
- **Why here:** One pending proposal per owner; a new one overwrites the old (the old key, if it never accepted, is simply forgotten).
- **Assumes:** the expiry clock starts at proposal, not at acceptance (L529; `accept_session` L538 refuses an already-expired proposal). A key that accepts late gets the remaining time only.
- **Establishes:** `pending_sessions[owner]`.
- **Depended on by:** `accept_session` L537, `pending_session` query L957-L960, `revoke_session` L548.

**Cross-Function Dependencies:**
- Callees: none.
- Callers: `Vault::create_session` L926, which emits `SessionProposed` (L927).
- Shared state: `pending_sessions` with `accept_session` (removes on accept, L540) and `revoke_session` (removes, L548).
- Invariant couplings: pending proposals are never expired-pruned; an expired one is refused at L538 and lingers until overwritten or revoked.

**Open Questions:**
- none.
