# logic-2: Expired sessions leave a stale `session_keys` entry that locks the key out of acting for itself, reserves it against every other owner, and only the (possibly absent) owner can clear it

**Location**
- `/Users/adityakrx/liquid-stake/programs/vara-pool/app/src/lib.rs:474-480` (`actor_for`: returns `Err(SessionExpired)` instead of falling back to the sender once `now_ms >= expires_at`), `:483-490` (`propose_session`: `session_keys.get(&key)` blocks any other owner even when the mapped session is expired), `:505-510` + `:882-887` (`revoke_session`: keyed by `message_source()` as *owner*; a key has no way to detach), `:890-906` (`session` / `session_owner` queries hide expired sessions, so the on-chain view disagrees with what `actor_for` and `propose_session` enforce).
- `/Users/adityakrx/liquid-stake/programs/vault/app/src/lib.rs:516-522`, `:525-532`, `:547-552` + `:943-948`, `:951-967` (identical code).

**Bug class**: Logic / state-machine lifecycle (no expiry cleanup, asymmetric revoke rights).

**Severity**: Low

**Description**
`accept_session` inserts `session_keys[key] = owner` and `sessions[owner] = Session { expires_at, .. }`. Expiry is only ever *checked*, never *applied*: no path removes the two entries when `now_ms >= expires_at`. Consequences, all reachable by any account:

1. `actor_for(key, ..)` finds `session_keys[key]`, then `sessions[owner]`, sees it expired and returns `Err(SessionExpired)`. The key can no longer `stake` / `unstake` / `request_unbond` / `claim` **for itself** either (it is never treated as a plain sender again). Any kVARA the key address holds (it can receive shares through `Vft::transfer` from anyone) is stuck behind that error; the only exit is `Vft::transfer` to a different address, because the `Vft` service does not go through `actor_for`.
2. `propose_session(other_owner, key, ..)` returns `BadSession` while the stale mapping points at the old owner, so the key address is reserved for that owner indefinitely.
3. Only `revoke_session()` called **by the owner** clears the mapping (`self.state.borrow_mut().revoke_session(owner)` where `owner = message_source()`). The key itself calling `revoke_session` removes nothing (it looks up `pending_sessions[key]` / `sessions[key]`, which are keyed by owner) and returns `false`. If the owner account is lost or uncooperative, states 1 and 2 are permanent.
4. `session(owner)` / `session_owner(key)` filter on `now_ms < expires_at` and return `None`, so a client that reads state sees "no session" while writes are still gated by it.

Code path for 1: `accept_session` at `T0`, `expires_at = T0 + 60_000`; at `T0 + 60_000` the key sends `Pool::unstake(1)`: `actor_for` -> `session_keys.get(&key) = Some(owner)` -> `sessions.get(owner)` present -> `now_ms >= expires_at` -> `Err(SessionExpired)`. The same message from an address with no history would succeed as a plain sender.

**Concrete trigger**
Owner `O` calls `create_session(K, 60, [Unstake])`; `K` calls `accept_session(O)`; someone `Vft::transfer(K, shares)`; after 60 s `K` calls `unstake(shares)` -> `SessionExpired`; `K` calls `revoke_session()` -> `false`; another owner `P` calls `create_session(K, ..)` -> `BadSession`. Repeats until `O` calls `revoke_session()`.

**Impact**
Key addresses become unusable as first-class pool users after their session lapses, and cannot be re-bound elsewhere, with no self-service recovery. Funds are recoverable via `Vft::transfer`, so this is availability/usability rather than loss.

**Confidence**: high

**Suggested fix**
- In `actor_for`, treat an expired session as absent: `if now_ms >= session.expires_at { return Ok(sender) }` (or lazily delete both map entries and fall through).
- Make `propose_session` / `accept_session` ignore a `session_keys` entry whose session is expired, and prune it.
- Let the key detach: in `revoke_session`, also check `session_keys.get(&caller)` and, if present, remove that owner's session (`sessions.remove(owner)`, `session_keys.remove(caller)`).
- Keep the read queries and the write gates on the same predicate.
