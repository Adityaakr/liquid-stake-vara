## `Admin::transfer_admin` in programs/demo-token/app/src/lib.rs (L352-L363)

**Purpose:** Exported `TransferAdmin(to) -> bool throws TokenError` (IDL L92). Single-step, immediate handover of the admin role.

**Inputs & Assumptions:**
- `to` (ActorId): new admin. Trust: untrusted argument from the admin. Zero rejected (L358). No other validation (no acceptance step, no check that `to` differs from the current admin).
- Implicit: `caller = message_source()` (L354); `admin`.
- Preconditions: caller is admin — `ensure_admin` L357, evaluated before the zero check. Not gated by pause.

**Outputs & Effects:**
- Success: `admin = to` (L359); `AdminTransferred { from: caller, to }` (L361); reply `true` (L362).
- Errors: `Unauthorized` (L357), `ZeroAddress` (L358) — the early `return Err` inside the block drops the `RefMut` before the structured panic (L352). No writes.

**Block-by-Block:**

```rust
// L357-L359
s.ensure_admin(caller)?;
if to.is_zero() { return Err(TokenError::ZeroAddress); }
s.admin = to;
```
- **What:** authorize, validate, overwrite.
- **Why here:** the write is the last statement in the block.
- **Assumes:** `to` is an address whose controller exists; nothing establishes this (an ActorId with no key or a program that never calls `Admin` ends role administration permanently — there is no recovery path in the file).
- **Establishes:** `ensure_admin(to)` succeeds and `ensure_admin(caller)` fails from the next message on. The former admin keeps minter authority only if it is in `minters` (L160); the admin address is not automatically in the set.
- **Depended on by:** every `ensure_admin` caller.

**Cross-Function Dependencies:**
- Callee `ensure_admin` (internal, L155-L157); `emit_event`.
- Callers: admin only. No test in tests/gtest.rs covers it.
- Shared state: `admin` (also `Program::new` L478).
- Invariant couplings: exactly one admin; handover is atomic with the reply.

**Open Questions:**
- None.
