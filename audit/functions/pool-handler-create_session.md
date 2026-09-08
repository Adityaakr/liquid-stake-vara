## `Pool::create_session` in programs/vara-pool/app/src/lib.rs (L862-L868)

**Purpose:** The sender proposes `key` to act for it, for `duration_secs`, limited to `actions`. Nothing binds until the key calls `accept_session` (comment L860-L861).

**Inputs & Assumptions:**
- `key` (ActorId), `duration_secs` (u64), `actions` (Vec<SessionAction>): untrusted.
- Implicit: `message_source` is the owner (L864); `block_timestamp` (L865).
- Preconditions: all in `PoolState::propose_session` (L484, L486).

**Outputs & Effects:**
- On `Ok`: `pending_sessions[owner]` written (L488); emits `SessionProposed {owner, key, expires_at}` (L866); reply `Ok(session)`.
- On `Err` (`BadSession`): no write, normal `Err` reply.
- No value handling; any attached value stays with the program uncounted.

**Block-by-Block:**

```rust
// L864-L867
let owner = Syscall::message_source();
let session = self.state.borrow_mut().propose_session(owner, key, duration_secs, actions, now_ms())?;
self.emit_event(PoolEvent::SessionProposed { owner, key, expires_at: session.expires_at }).expect("event");
Ok(session)
```
- **What:** Thin wrapper. The temporary `RefMut` from `borrow_mut()` is dropped at the end of the statement (L865) before `emit_event`.
- **Establishes:** the proposal is attributable only to the sender; a stranger's address can be named as `key` without effect on that address's own position (`actor_for` L475; gtest L313-L328).

**Cross-Function Dependencies:**
- Callee `PoolState::propose_session` (internal): see record.
- Callee `emit_event`: trap on failure.
- Callers: dispatcher only.
- Shared state: `pending_sessions`.

**Open Questions:**
- none.
