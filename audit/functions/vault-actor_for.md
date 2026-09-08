## `VaultState::actor_for` in programs/vault/app/src/lib.rs (L516-L522)

**Purpose:** Decide who a message acts for: the sender itself, or — if the sender is a registered session key — the owner who registered it, provided the session is unexpired and permits `action`. Every user-facing command that touches a position calls it pre-await (`deposit` L743, `redeem` L766, `request_unbond` L791, `claim` L806). `fund_rewards` and the admin commands do not.

**Inputs & Assumptions:**
- `sender` (ActorId): `Syscall::message_source()`. Trust: trusted as an identity (runtime-provided).
- `action` (SessionAction): the calling handler's constant. Trust: trusted.
- `now_ms` (u64): trusted.
- Implicit state read: `session_keys` (key → owner, L202), `sessions` (owner → Session, L200).
- Precondition: `session_keys[k] == o` implies `sessions[o].key == k`. Established by `accept_session` L541-L543 (inserts both, removing the previous key of `o` first) and `revoke_session` L549-L550 (removes both). `propose_session` writes neither map. Nothing else writes them.

**Outputs & Effects:**
- Returns `Ok(sender)` when the sender is not a key (L517); `Ok(owner)` when the session is live and allows the action (L521); `Err(SessionExpired)` when the key maps to an owner with no session (L518, unreachable under the precondition) or an expired one (L519); `Err(SessionNotAllowed)` when the action is not in the list (L520). No writes.

**Block-by-Block:**

```rust
// L517
let Some(owner) = self.session_keys.get(&sender) else { return Ok(sender) };
```
- **What:** Plain senders act for themselves.
- **Why here:** Cheapest path first; the common case.
- **Assumes:** an account that has *accepted* a session is a key and is never treated as itself again for any action while the mapping exists — including actions the session does not permit (those get `SessionNotAllowed` at L520, not `Ok(sender)`). A proposed-but-not-accepted key is a plain sender (test L1248, gtest L329-L330).
- **Establishes:** `sender` is a key with `owner`.
- **Depended on by:** L518-L521.

```rust
// L518-L521
let session = self.sessions.get(owner).ok_or(VaultError::SessionExpired)?;
if now_ms >= session.expires_at { return Err(VaultError::SessionExpired); }
if !session.actions.contains(&action) { return Err(VaultError::SessionNotAllowed); }
Ok(*owner)
```
- **What:** Liveness and permission.
- **Why here:** Expiry before permission so an expired key gets a uniform error.
- **Assumes:** expired sessions are never pruned — nothing removes an expired entry from `sessions`/`session_keys` (`session`/`session_owner` queries filter by time at L953, L966 but do not write). So an expired key stays a key: it can no longer act for itself either, until the owner revokes (L547-L552) or re-proposes and the key re-accepts (L541 removes the old key). gtest L374-L383 checks the `SessionExpired` outcome, not the key's own standing.
- **Establishes:** `owner` is the actor; payouts and shares go to `owner` in every caller (L747, L772, L810, L792).
- **Depended on by:** the callers' `owner` capture, pre-await.

**Cross-Function Dependencies:**
- Callees: none.
- Callers: `deposit`, `redeem`, `request_unbond`, `claim` — all resolve `owner` before their await, so a session revoked or replaced during the await does not redirect the in-flight flow.
- Shared state: `sessions`, `session_keys` with `accept_session`, `revoke_session`.
- Invariant couplings: a key serves at most one owner (L528, L539), so the lookup at L517 is unambiguous. A key that deposits pulls from `owner`'s token balance (L747) and thus needs `owner`'s token approval, not its own.

**Open Questions:**
- unclear; need to inspect whether "a session key cannot act for itself for non-permitted actions" (L520 returning an error instead of falling back to `Ok(sender)`) is the intended semantics.
