## `Pool::accept_session` in programs/vara-pool/app/src/lib.rs (L872-L878)

**Purpose:** The sender, as the proposed key, activates the session `owner` proposed for it (comment L870-L871).

**Inputs & Assumptions:**
- `owner` (ActorId): untrusted parameter; must match a pending proposal whose `key` is the sender (L495).
- Implicit: `message_source` is the key (L874); `block_timestamp` (L875).
- Preconditions: in `PoolState::accept_session` (proposal exists for this key, unexpired, key not bound elsewhere).

**Outputs & Effects:**
- On `Ok`: `pending_sessions[owner]` removed, `sessions[owner]` and `session_keys[key]` written, previous binding removed (L498-L501); emits `SessionAccepted {owner, key, expires_at}` (L876); reply `Ok(session)`.
- On `Err` (`BadSession`, `SessionExpired`): no write.

**Block-by-Block:**

```rust
// L874-L877
let key = Syscall::message_source();
let session = self.state.borrow_mut().accept_session(key, owner, now_ms())?;
self.emit_event(PoolEvent::SessionAccepted { owner, key, expires_at: session.expires_at }).expect("event");
Ok(session)
```
- **What:** Thin wrapper; borrow dropped at end of L875.
- **Establishes:** from this message on, every `Pool` command sent by `key` is routed to `owner` by `actor_for` (L475-L479) and `key` can no longer act for itself there until `owner` revokes or replaces the session.

**Cross-Function Dependencies:**
- Callee `PoolState::accept_session` (internal): see record.
- Callee `emit_event`: trap on failure.
- Callers: dispatcher only.
- Shared state: the three session maps.
- Invariant couplings: the consent step of the session scheme; `Vft` transfers are not routed through sessions, so a key cannot move the owner's kVARA as a token, only exit it through `unstake`/`request_unbond` with payout to the owner.

**Open Questions:**
- none.
